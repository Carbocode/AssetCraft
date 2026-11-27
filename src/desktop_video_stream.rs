//! Indexed video playback. Packet access and codec state remain on the viewer worker.
use std::{collections::VecDeque, path::Path, sync::Arc};

use super::{
    AUDIO_FALLBACK_MAX_BYTES, ByteSource, Cancel, CodecId, CodecParams, File, Format, Image,
    MAX_SAMPLE_BYTES, ReadSeekSource, Source, ThumbError, TrackDecoder, VideoFrame, avi, failed,
    isobmff, matroska, mpegts, mxf, open_movie, playback_decoder, playback_packet_safe, poster,
    supported,
};
use assetcraft::local_file;

const VIDEO_READ_BYTES: usize = 128 << 20;
const MAX_PACKETS_PER_UPDATE: usize = 360;
const MAX_BUFFERED_FRAMES: usize = 16;
const DEFAULT_FRAME_NS: i64 = 33_333_333;
const PLAYBACK_EDGE: u32 = 1440;
const STILL_EDGE: u32 = 2160;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Playback,
    Still,
}

impl Quality {
    fn edge(self) -> u32 {
        match self {
            Self::Playback => PLAYBACK_EDGE,
            Self::Still => STILL_EDGE,
        }
    }
}

enum Container {
    Mp4(isobmff::Mp4File),
    Mkv(Box<matroska::MkvFile>),
    Avi(avi::AviFile),
    Mxf(mxf::MxfFile),
    Ts(mpegts::File, Vec<u8>),
}

#[derive(Clone, Copy)]
struct Sample {
    ns: i64,
    duration_ns: i64,
    size: u64,
    key: bool,
    entry: u32,
}

pub struct Video {
    file: File,
    container: Container,
    track: usize,
    rotation: u8,
    samples: Vec<Sample>,
    timeline: Arc<[i64]>,
    sorted_keys: Vec<(i64, usize)>,
    duration: i64,
    audio_offset: Option<i64>,
    cursor: usize,
    decoder: Option<TrackDecoder>,
    entry: u32,
    ready: VecDeque<VideoFrame>,
    current: Option<VideoFrame>,
    presented: Option<(i64, Quality)>,
    large: bool,
    draft: bool,
    drained: bool,
}

pub enum FrameUpdate {
    Pending,
    Ready(Option<Box<Image>>),
}

impl Video {
    pub fn open(path: &Path, format: Format, cancel: &Cancel) -> Result<Self, ThumbError> {
        let file = local_file::open_read(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ThumbError::Missing
            } else {
                failed(error)
            }
        })?;
        if !file.metadata().map_err(failed)?.is_file() {
            return Err(ThumbError::Missing);
        }
        let src = source(&file, cancel)?;
        let container = match format {
            Format::Mp4 | Format::Mov => Container::Mp4(open_movie(&file, cancel)?),
            Format::WebM | Format::Mkv => {
                let file = matroska::open_with(
                    &src,
                    &matroska::OpenOptions {
                        index_limit: Some(96 << 20),
                        ..Default::default()
                    },
                )
                .map_err(failed)?;
                if !file.indexed {
                    return Err(failed("The video index exceeds the playback scan budget"));
                }
                Container::Mkv(Box::new(file))
            }
            Format::Avi => Container::Avi(avi::open(&src).map_err(failed)?),
            Format::Mxf => Container::Mxf(mxf::open(&src).map_err(failed)?),
            Format::MpegTs => {
                if src.len() > 48 << 20 {
                    return Err(failed(
                        "The transport stream exceeds the playback scan budget",
                    ));
                }
                let mut bytes = vec![0; src.len() as usize];
                src.read_at(0, &mut bytes).map_err(failed)?;
                let file = mpegts::open(bytes.as_slice()).map_err(failed)?;
                Container::Ts(file, bytes)
            }
            _ => return Err(ThumbError::Unsupported),
        };
        let track = container
            .video_track()
            .ok_or_else(|| failed("No playable video track"))?;
        let (samples, rotation, duration) = container.describe(track)?;
        let (mut timeline, mut sorted_keys) = (Vec::with_capacity(samples.len()), Vec::new());
        let origin = if matches!(container, Container::Ts(..)) {
            samples.iter().map(|sample| sample.ns).min().unwrap_or(0)
        } else {
            0
        };
        let mut samples = samples;
        for (i, sample) in samples.iter_mut().enumerate() {
            sample.ns = sample.ns.saturating_sub(origin);
            if sample.ns >= 0 {
                timeline.push(sample.ns);
            }
            if sample.key {
                sorted_keys.push((sample.ns, i));
            }
        }
        timeline.sort_unstable();
        timeline.dedup();
        sorted_keys.sort_unstable();
        let Some(&last) = timeline.last() else {
            return Err(ThumbError::Unsupported);
        };
        if sorted_keys.is_empty()
            || !supported(
                container
                    .params(track, samples[sorted_keys[0].1].entry)
                    .codec,
            )
        {
            return Err(ThumbError::Unsupported);
        }
        let sample_end = samples
            .iter()
            .map(|s| s.ns.saturating_add(s.duration_ns.max(1)))
            .max()
            .unwrap_or(last);
        let duration = duration
            .unwrap_or(sample_end.max(last.saturating_add(DEFAULT_FRAME_NS)))
            .max(last.saturating_add(1));
        let audio_offset = container.audio_offset(src.len());
        let params = container.params(track, samples[sorted_keys[0].1].entry);
        let large = params.width.unwrap_or(0).max(params.height.unwrap_or(0)) > STILL_EDGE;
        Ok(Self {
            file,
            container,
            track,
            rotation,
            samples,
            timeline: timeline.into(),
            sorted_keys,
            duration,
            audio_offset,
            cursor: 0,
            decoder: None,
            entry: 0,
            ready: VecDeque::new(),
            current: None,
            presented: None,
            large,
            draft: false,
            drained: false,
        })
    }

    pub fn timeline(&self) -> Arc<[i64]> {
        Arc::clone(&self.timeline)
    }
    pub fn duration(&self) -> i64 {
        self.duration
    }
    pub fn audio_offset(&self) -> Option<i64> {
        self.audio_offset
    }

    /// Decode to the frame being presented at `target`. Seeks rebuild codec state from a keyframe.
    pub fn frame(
        &mut self,
        target: i64,
        seek: bool,
        quality: Quality,
        cancel: &Cancel,
    ) -> Result<FrameUpdate, ThumbError> {
        let target = target.clamp(self.timeline[0], self.duration.saturating_sub(1));
        let draft = self.large && quality == Quality::Playback;
        if seek
            || self.decoder.is_none()
            || self.draft != draft
            || self
                .current
                .as_ref()
                .and_then(|frame| frame.pts)
                .is_some_and(|at| target < at)
        {
            let key = self
                .sorted_keys
                .partition_point(|&(pts, _)| pts <= target)
                .saturating_sub(1);
            self.cursor = self.sorted_keys[key].1;
            self.decoder = None;
            self.ready.clear();
            self.current = None;
            self.presented = None;
            self.drained = false;
            self.draft = draft;
        }
        let src = source(&self.file, cancel)?;
        let mut batches = 0;
        loop {
            if cancel.is_cancelled() {
                return Err(ThumbError::Cancelled);
            }
            while self
                .ready
                .front()
                .is_some_and(|frame| frame.pts.unwrap_or(0) <= target)
            {
                self.current = self.ready.pop_front();
            }
            if !self.ready.is_empty() || self.drained {
                break;
            }
            if batches == MAX_PACKETS_PER_UPDATE {
                return Ok(FrameUpdate::Pending);
            }
            batches += 1;
            let frames = if let Some(sample) = self.samples.get(self.cursor).copied() {
                if sample.size > MAX_SAMPLE_BYTES {
                    return Err(failed("A video packet exceeds the playback limit"));
                }
                let bytes = self.container.packet(&src, self.track, self.cursor)?;
                let params = self.container.params(self.track, sample.entry);
                let fresh = self.decoder.is_none() || self.entry != sample.entry;
                if !playback_packet_safe(&params, &bytes, fresh || sample.key) {
                    return Err(failed(
                        "A video packet is unsupported or exceeds the decoder limits",
                    ));
                }
                if fresh {
                    let mut decoder = playback_decoder(&params, if self.large { 4 } else { 1 })?;
                    decoder.set_draft(draft);
                    self.decoder = Some(decoder);
                    self.entry = sample.entry;
                }
                let Some(decoder) = &mut self.decoder else {
                    return Err(ThumbError::Unsupported);
                };
                self.cursor += 1;
                decoder.decode(&bytes, sample.ns).map_err(failed)?
            } else {
                self.drained = true;
                self.decoder
                    .as_mut()
                    .ok_or(ThumbError::Unsupported)?
                    .flush()
                    .map_err(failed)?
            };
            if frames.len() > MAX_BUFFERED_FRAMES {
                return Err(failed("Too many buffered video frames"));
            }
            self.ready.extend(frames);
            self.ready.make_contiguous().sort_by_key(|frame| frame.pts);
        }
        let frame = self
            .current
            .as_ref()
            .or_else(|| self.ready.front())
            .ok_or_else(|| failed("No video frame at this position"))?;
        let ns = frame.pts.unwrap_or(target);
        if self.presented == Some((ns, quality)) {
            return Ok(FrameUpdate::Ready(None));
        }
        let image = poster(frame, self.rotation, quality.edge())?;
        self.presented = Some((ns, quality));
        Ok(FrameUpdate::Ready(Some(Box::new(image))))
    }
}

fn source<'a>(file: &File, cancel: &'a Cancel) -> Result<Source<'a>, ThumbError> {
    Ok(Source {
        stream: ReadSeekSource::new(file.try_clone().map_err(failed)?).map_err(failed)?,
        bytes_left: std::cell::Cell::new(VIDEO_READ_BYTES),
        reads_left: std::cell::Cell::new(300_000),
        cancel,
    })
}

fn ticks_ns(ticks: i64, scale: u32) -> i64 {
    (i128::from(ticks) * 1_000_000_000 / i128::from(scale.max(1)))
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

impl Container {
    fn video_track(&self) -> Option<usize> {
        match self {
            Self::Mp4(file) => file.track_of_kind(isobmff::TrackKind::Video),
            Self::Mkv(file) => file
                .track_of_kind(matroska::TrackKind::Video)
                .filter(|&i| file.tracks[i].frames_readable()),
            Self::Avi(file) => file
                .streams
                .iter()
                .position(|stream| stream.kind == avi::StreamKind::Video),
            Self::Mxf(file) => file.track_of_kind(mxf::TrackKind::Picture),
            Self::Ts(file, _) => file.find(mpegts::Kind::Video, |_| true),
        }
    }

    fn audio_offset(&self, bytes: u64) -> Option<i64> {
        let (codec, offset) = match self {
            Self::Mp4(file) => {
                let track = &file.tracks[file.track_of_kind(isobmff::TrackKind::Audio)?];
                let offset = track
                    .edits
                    .iter()
                    .take_while(|edit| edit.media_time < 0)
                    .fold(0i64, |total, edit| {
                        total.saturating_add(
                            i64::try_from(edit.segment_duration).unwrap_or(i64::MAX),
                        )
                    });
                (track.codec_params().codec, ticks_ns(offset, file.timescale))
            }
            Self::Mkv(file) if bytes <= AUDIO_FALLBACK_MAX_BYTES => {
                let track = &file.tracks[file.track_of_kind(matroska::TrackKind::Audio)?];
                if !track.frames_readable() {
                    return None;
                }
                (track.codec_params().codec, 0)
            }
            _ => return None,
        };
        matches!(
            codec,
            CodecId::Aac | CodecId::Opus | CodecId::Alac | CodecId::Mp3 | CodecId::Pcm
        )
        .then_some(offset)
    }

    fn describe(&self, track: usize) -> Result<(Vec<Sample>, u8, Option<i64>), ThumbError> {
        let (samples, rotation, duration) = match self {
            Self::Mp4(file) => {
                let track = &file.tracks[track];
                let samples = track
                    .samples
                    .iter()
                    .enumerate()
                    .map(|(i, s)| Sample {
                        ns: ticks_ns(track.presentation_pts(i).unwrap_or(s.pts), track.timescale),
                        duration_ns: ticks_ns(i64::from(s.duration), track.timescale),
                        size: u64::from(s.size),
                        key: s.is_sync,
                        entry: s.description_index,
                    })
                    .collect();
                let duration = i64::try_from(file.duration)
                    .ok()
                    .map(|d| ticks_ns(d, file.timescale))
                    .filter(|&d| d > 0);
                (samples, track.display_rotation().unwrap_or(0), duration)
            }
            Self::Mkv(file) => {
                let track = &file.tracks[track];
                let samples = track
                    .samples
                    .iter()
                    .enumerate()
                    .map(|(i, s)| Sample {
                        ns: track.ticks_to_ns(s.pts),
                        duration_ns: track.ticks_to_ns(i64::try_from(s.duration).unwrap_or(0)),
                        size: track.sample_size(i).unwrap_or(u64::MAX),
                        key: s.keyframe,
                        entry: 0,
                    })
                    .collect();
                let rotation = track
                    .video
                    .as_ref()
                    .and_then(|video| video.display_rotation())
                    .unwrap_or(0);
                (
                    samples,
                    rotation,
                    file.duration_ns().and_then(|d| i64::try_from(d).ok()),
                )
            }
            Self::Avi(file) => {
                let track = &file.streams[track];
                let intra = matches!(
                    track.codec_params().codec,
                    CodecId::Mjpeg | CodecId::ProRes | CodecId::Dnxhd
                );
                let samples = track
                    .chunks
                    .iter()
                    .enumerate()
                    .map(|(i, s)| Sample {
                        ns: ticks_ns(
                            (i as i64).saturating_mul(i64::from(track.scale)),
                            track.rate,
                        ),
                        duration_ns: ticks_ns(i64::from(track.scale), track.rate),
                        size: u64::from(s.size),
                        key: s.key || intra,
                        entry: 0,
                    })
                    .collect();
                (samples, 0, None)
            }
            Self::Mxf(file) => {
                let track = &file.tracks[track];
                let rate = track.edit_rate.as_f64();
                if rate <= 0.0 || !rate.is_finite() {
                    return Err(failed("Invalid MXF frame rate"));
                }
                let ns = |pts: i64| (pts as f64 * 1_000_000_000.0 / rate) as i64;
                let samples = track
                    .samples
                    .iter()
                    .map(|s| Sample {
                        ns: ns(s.pts.saturating_sub(track.first_edit_unit())),
                        duration_ns: ns(1),
                        size: u64::from(s.size),
                        key: s.key,
                        entry: 0,
                    })
                    .collect();
                (samples, 0, track.duration.map(ns))
            }
            Self::Ts(file, _) => {
                let samples = file.streams[track]
                    .units
                    .iter()
                    .filter(|unit| unit.pts.is_some())
                    .count();
                if samples != file.streams[track].units.len() {
                    return Err(failed("The transport stream has incomplete timestamps"));
                }
                (
                    file.streams[track]
                        .units
                        .iter()
                        .map(|unit| Sample {
                            ns: ticks_ns(unit.pts.unwrap_or_default(), 90_000),
                            duration_ns: DEFAULT_FRAME_NS,
                            size: u64::from(unit.size),
                            key: unit.key,
                            entry: 0,
                        })
                        .collect(),
                    0,
                    None,
                )
            }
        };
        Ok((samples, rotation, duration))
    }

    fn packet(&self, source: &Source<'_>, track: usize, i: usize) -> Result<Vec<u8>, ThumbError> {
        match self {
            Self::Mp4(file) => file.read_sample(source, track, i).map_err(failed),
            Self::Mkv(file) => file.read_sample(source, track, i).map_err(failed),
            Self::Avi(file) => file.read_chunk(source, track, i).map_err(failed),
            Self::Mxf(file) => file.read_sample(source, track, i).map_err(failed),
            Self::Ts(file, bytes) => file.read_unit(bytes.as_slice(), track, i).map_err(failed),
        }
    }

    fn params(&self, track: usize, entry: u32) -> CodecParams {
        match self {
            Self::Mp4(file) => file.tracks[track].codec_params_for(entry),
            Self::Mkv(file) => file.tracks[track].codec_params(),
            Self::Avi(file) => file.streams[track].codec_params(),
            Self::Mxf(file) => file.tracks[track].codec_params(),
            Self::Ts(file, _) => file.streams[track].codec_params(),
        }
    }
}
