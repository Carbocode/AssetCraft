use std::{cell::Cell, fs::File, io, time::Duration};

use assetcraft::Format;
use craft::behavior::PixelSize;
use craft::browse::thumbs::ThumbError;
use craft::color::{Matrix, Range, normalize_c, normalize_y, ycbcr_to_rgb};
use craft::host::queue::Cancel;
use craft::image_codecs::{ChannelLayout, Image};
use craft::media::{
    ByteSource, CodecId, CodecParams, ReadSeekSource, avi, isobmff, matroska, mpegts, mxf,
};
use craft::video::{
    self, TrackDecoder,
    frame::{ColorFamily, Plane, VideoFrame},
};

use crate::desktop_documents::StillPreview;

const HEADER_BUDGET: u64 = 24 << 20;
const READ_BUDGET: usize = 64 << 20;
const MAX_SAMPLE_BYTES: u64 = 16 << 20;
const MAX_PIXELS: u64 = 12_000_000;
const MAX_EDGE: u32 = 4096;
const MAX_KEYFRAMES: usize = 4;

struct Source<'a> {
    stream: ReadSeekSource<File>,
    bytes_left: Cell<usize>,
    reads_left: Cell<usize>,
    cancel: &'a Cancel,
}

impl ByteSource for Source<'_> {
    fn len(&self) -> u64 {
        self.stream.len()
    }

    fn read_at(&self, offset: u64, bytes: &mut [u8]) -> io::Result<()> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::other("Video preview cancelled"));
        }
        let (Some(remaining), Some(reads)) = (
            self.bytes_left.get().checked_sub(bytes.len()),
            self.reads_left.get().checked_sub(1),
        ) else {
            return Err(io::Error::other("Video preview read budget exceeded"));
        };
        self.bytes_left.set(remaining);
        self.reads_left.set(reads);
        self.stream.read_at(offset, bytes)
    }
}

pub fn render(file: File, format: Format, edge: u32, cancel: &Cancel) -> StillPreview {
    let result = (|| {
        let src = Source {
            stream: ReadSeekSource::new(file).map_err(failed)?,
            bytes_left: Cell::new(READ_BUDGET),
            reads_left: Cell::new(110_000),
            cancel,
        };
        match format {
            Format::Mp4 | Format::Mov => mp4(&src, edge.max(1)),
            Format::WebM | Format::Mkv => mkv(&src, edge.max(1)),
            Format::Avi => avi_preview(&src, edge.max(1)),
            Format::Mxf => mxf_preview(&src, edge.max(1)),
            Format::MpegTs => transport_preview(&src, edge.max(1)),
            _ => Err(ThumbError::Unsupported),
        }
    })();
    if cancel.is_cancelled() {
        return StillPreview::new(Err(ThumbError::Cancelled));
    }
    result.unwrap_or_else(|error| {
        let mut preview = StillPreview::new(Err(error));
        preview
            .properties
            .push(("Preview", "Could not read video metadata".into()));
        preview
    })
}

fn failed(error: impl std::fmt::Display) -> ThumbError {
    ThumbError::Failed(error.to_string())
}

fn size_allowed(width: u32, height: u32) -> bool {
    (1..=MAX_EDGE).contains(&width)
        && (1..=MAX_EDGE).contains(&height)
        && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

fn preview_info(codec: &str, size: (u32, u32), duration: Option<Duration>) -> StillPreview {
    let mut preview = StillPreview::new(Err(ThumbError::Unsupported));
    preview.properties.push(("Video codec", codec.to_owned()));
    if size.0 > 0 && size.1 > 0 {
        preview.dimensions = Some(PixelSize::new(size.0, size.1));
    }
    if let Some(duration) = duration {
        let seconds = duration.as_secs();
        preview.properties.push((
            "Duration",
            format!(
                "{}:{:02}:{:02}.{:03}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60,
                duration.subsec_millis(),
            ),
        ));
    }
    preview
}

fn no_frame(preview: &mut StillPreview, reason: &'static str) {
    preview.properties.push(("Preview", reason.into()));
}

fn timescale_duration(duration: u64, timescale: u32) -> Option<Duration> {
    if timescale == 0 || duration == 0 || matches!(duration, u64::MAX | 0xffff_ffff) {
        return None;
    }
    let ticks = u64::from(timescale);
    Some(Duration::new(
        duration / ticks,
        ((duration % ticks) * 1_000_000_000 / ticks) as u32,
    ))
}

fn mp4(src: &Source<'_>, edge: u32) -> Result<StillPreview, ThumbError> {
    check_movie_headers(src)?;
    let movie = isobmff::open(src).map_err(failed)?;
    let Some(index) = movie.track_of_kind(isobmff::TrackKind::Video) else {
        let mut preview = StillPreview::new(Err(ThumbError::Unsupported));
        no_frame(&mut preview, "No video track");
        return Ok(preview);
    };
    let track = &movie.tracks[index];
    let first = track.codec_params();
    let rotation = track.display_rotation().unwrap_or(0);
    let mut size = (
        first.width.unwrap_or(track.width),
        first.height.unwrap_or(track.height),
    );
    if !rotation.is_multiple_of(2) {
        size = (size.1, size.0);
    }
    let duration = timescale_duration(movie.duration, movie.timescale)
        .or_else(|| timescale_duration(track.duration, track.timescale));
    let mut preview = preview_info(first.codec.name(), size, duration);
    if !supported(first.codec) {
        no_frame(&mut preview, "No preview decoder for this video codec");
        return Ok(preview);
    }
    let frames = track
        .samples
        .iter()
        .enumerate()
        .filter(|(_, sample)| sample.is_sync && u64::from(sample.size) <= MAX_SAMPLE_BYTES)
        .take(MAX_KEYFRAMES);
    for (sample_index, sample) in frames {
        let params = track.codec_params_for(sample.description_index);
        let data = movie.read_sample(src, index, sample_index).map_err(failed);
        if set_frame(&mut preview, &params, data, sample.pts, rotation, edge) {
            return Ok(preview);
        }
    }
    no_frame(
        &mut preview,
        "No supported keyframe within the preview limits",
    );
    Ok(preview)
}

fn check_movie_headers(src: &Source<'_>) -> Result<(), ThumbError> {
    use isobmff::boxes::BoxHeader;
    let mut offset = 0;
    let mut left = HEADER_BUDGET;
    let mut samples_left = 1_000_000usize;
    let mut buffer = [0u8; BoxHeader::MAX_LEN];
    for _ in 0..50_000 {
        let left_in_file = src.len() - offset;
        if left_in_file < 8 {
            return Ok(());
        }
        let bytes = &mut buffer[..left_in_file.min(BoxHeader::MAX_LEN as u64) as usize];
        src.read_at(offset, bytes).map_err(failed)?;
        let header = BoxHeader::parse(bytes).map_err(failed)?;
        let size = header.size.unwrap_or(left_in_file);
        if size > left_in_file || size < header.header_len {
            return Err(failed("Invalid movie box length"));
        }
        if matches!(&header.kind.0, b"moov" | b"moof") {
            let payload_size = size - header.header_len;
            left = left
                .checked_sub(payload_size)
                .ok_or_else(|| failed("Movie metadata exceeds the preview limit"))?;
            let mut payload = vec![0u8; payload_size as usize];
            src.read_at(offset + header.header_len, &mut payload)
                .map_err(failed)?;
            check_sample_counts(&payload, &mut samples_left, 0)?;
        }
        offset += size;
    }
    Err(failed("Too many movie boxes for a preview"))
}

fn check_sample_counts(
    bytes: &[u8],
    remaining: &mut usize,
    depth: usize,
) -> Result<(), ThumbError> {
    use isobmff::boxes::boxes;
    if depth > 6 {
        return Err(failed("Movie metadata exceeds the nesting limit"));
    }
    for item in boxes(bytes) {
        let item = item.map_err(failed)?;
        let count_offset = match &item.kind.0 {
            b"trak" | b"mdia" | b"minf" | b"stbl" | b"traf" => {
                check_sample_counts(item.payload, remaining, depth + 1)?;
                continue;
            }
            b"stsz" | b"stz2" => 8,
            b"trun" => 4,
            _ => continue,
        };
        let value = item
            .payload
            .get(count_offset..count_offset + 4)
            .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
            .ok_or_else(|| failed("Truncated movie sample table"))?;
        *remaining = remaining
            .checked_sub(u32::from_be_bytes(value) as usize)
            .ok_or_else(|| failed("Movie sample index exceeds the preview limit"))?;
    }
    Ok(())
}

fn mkv(src: &Source<'_>, edge: u32) -> Result<StillPreview, ThumbError> {
    let movie = matroska::open_with(
        src,
        &matroska::OpenOptions {
            index_limit: Some(8 << 20),
            ..Default::default()
        },
    )
    .map_err(failed)?;
    let Some(index) = movie.track_of_kind(matroska::TrackKind::Video) else {
        let mut preview = StillPreview::new(Err(ThumbError::Unsupported));
        no_frame(&mut preview, "No video track");
        return Ok(preview);
    };
    let track = &movie.tracks[index];
    let params = track.codec_params();
    let duration = if movie.indexed || movie.info.duration.is_some() {
        movie.duration_ns().map(Duration::from_nanos)
    } else {
        None
    };
    let mut preview = preview_info(
        params.codec.name(),
        (params.width.unwrap_or(0), params.height.unwrap_or(0)),
        duration,
    );
    if !supported(params.codec) || !track.frames_readable() {
        no_frame(&mut preview, "No preview decoder for this video track");
        return Ok(preview);
    }
    let frames = track
        .samples
        .iter()
        .enumerate()
        .filter(|(index, sample)| {
            sample.keyframe
                && track
                    .sample_size(*index)
                    .is_some_and(|bytes| bytes <= MAX_SAMPLE_BYTES)
        })
        .take(MAX_KEYFRAMES);
    for (sample_index, sample) in frames {
        let data = movie.read_sample(src, index, sample_index).map_err(failed);
        if set_frame(&mut preview, &params, data, sample.pts, 0, edge) {
            return Ok(preview);
        }
    }
    no_frame(
        &mut preview,
        "No supported keyframe within the preview limits",
    );
    Ok(preview)
}

fn supported(codec: CodecId) -> bool {
    matches!(
        codec,
        CodecId::H264
            | CodecId::ProRes
            | CodecId::Vp9
            | CodecId::Vp8
            | CodecId::Hevc
            | CodecId::Av1
            | CodecId::Dnxhd
            | CodecId::Mjpeg
            | CodecId::Mpeg2Video
    )
}

fn avi_preview(src: &Source<'_>, edge: u32) -> Result<StillPreview, ThumbError> {
    let file = avi::open(src).map_err(failed)?;
    let (index, track) = file
        .streams
        .iter()
        .enumerate()
        .find(|(_, track)| track.kind == avi::StreamKind::Video)
        .ok_or_else(|| failed("No video track"))?;
    let params = track.codec_params();
    let duration = timescale_duration(u64::from(track.length) * u64::from(track.scale), track.rate);
    Ok(indexed_preview(
        &params,
        duration,
        edge,
        track
            .chunks
            .iter()
            .enumerate()
            .filter(|(_, chunk)| chunk.key && u64::from(chunk.size) <= MAX_SAMPLE_BYTES)
            .map(|(i, _)| (i as i64, file.read_chunk(src, index, i).map_err(failed))),
    ))
}

fn mxf_preview(src: &Source<'_>, edge: u32) -> Result<StillPreview, ThumbError> {
    let file = mxf::open(src).map_err(failed)?;
    let index = file
        .track_of_kind(mxf::TrackKind::Picture)
        .ok_or_else(|| failed("No video track"))?;
    let track = &file.tracks[index];
    let params = track.codec_params();
    let duration = track
        .duration
        .filter(|_| track.edit_rate.is_valid())
        .and_then(|duration| {
            Duration::try_from_secs_f64(duration as f64 / track.edit_rate.as_f64()).ok()
        });
    Ok(indexed_preview(
        &params,
        duration,
        edge,
        track
            .samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.key && u64::from(sample.size) <= MAX_SAMPLE_BYTES)
            .map(|(i, sample)| (sample.pts, file.read_sample(src, index, i).map_err(failed))),
    ))
}

fn transport_preview(src: &Source<'_>, edge: u32) -> Result<StillPreview, ThumbError> {
    let mut opening = vec![0; src.len().min(16 << 20) as usize];
    src.read_at(0, &mut opening).map_err(failed)?;
    let file = mpegts::open(opening.as_slice()).map_err(failed)?;
    let index = file
        .find(mpegts::Kind::Video, |_| true)
        .ok_or_else(|| failed("No video track in the opening segment"))?;
    let track = &file.streams[index];
    let params = track.codec_params();
    let duration = (src.len() == opening.len() as u64)
        .then(|| track.pts_range())
        .flatten()
        .and_then(|(start, end)| timescale_duration(end.saturating_sub(start) as u64, 90_000));
    Ok(indexed_preview(
        &params,
        duration,
        edge,
        track
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| unit.key && u64::from(unit.size) <= MAX_SAMPLE_BYTES)
            .map(|(i, unit)| {
                (
                    unit.pts.unwrap_or(0),
                    file.read_unit(opening.as_slice(), index, i).map_err(failed),
                )
            }),
    ))
}

fn indexed_preview(
    params: &CodecParams,
    duration: Option<Duration>,
    edge: u32,
    frames: impl Iterator<Item = (i64, Result<Vec<u8>, ThumbError>)>,
) -> StillPreview {
    let mut preview = preview_info(
        params.codec.name(),
        (params.width.unwrap_or(0), params.height.unwrap_or(0)),
        duration,
    );
    if !supported(params.codec) {
        no_frame(&mut preview, "No preview decoder for this video codec");
    } else if !frames
        .take(MAX_KEYFRAMES)
        .any(|(pts, packet)| set_frame(&mut preview, params, packet, pts, 0, edge))
    {
        no_frame(&mut preview, "No decodable keyframe in the preview budget");
    }
    preview
}

fn set_frame(
    preview: &mut StillPreview,
    params: &CodecParams,
    packet: Result<Vec<u8>, ThumbError>,
    pts: i64,
    rotation: u8,
    edge: u32,
) -> bool {
    let frame = packet.and_then(|packet| decode_keyframe(params, &packet, pts));
    let Ok(frame) = frame else { return false };
    if preview.dimensions.is_none() {
        let (w, h) = (frame.width(), frame.height());
        let (w, h) = if rotation.is_multiple_of(2) {
            (w, h)
        } else {
            (h, w)
        };
        preview.dimensions = Some(PixelSize::new(w, h));
    }
    preview.image = poster(&frame, rotation, edge);
    preview.image.is_ok()
}

fn decode_keyframe(params: &CodecParams, bytes: &[u8], pts: i64) -> Result<VideoFrame, ThumbError> {
    if !supported(params.codec) || bytes.len() as u64 > MAX_SAMPLE_BYTES {
        return Err(ThumbError::Unsupported);
    }
    let bytes = if params.codec == CodecId::Vp9 {
        vp9_first_frame(bytes).ok_or(ThumbError::Unsupported)?
    } else {
        bytes
    };
    if !playback_packet_safe(params, bytes, true) {
        return Err(ThumbError::Unsupported);
    }
    let mut decoder = playback_decoder(params, 1)?;
    if let Some(frame) = decoder
        .decode(bytes, pts)
        .map_err(failed)?
        .into_iter()
        .next()
    {
        return Ok(frame);
    }
    decoder
        .flush()
        .map_err(failed)?
        .into_iter()
        .next()
        .ok_or(ThumbError::Unsupported)
}

fn playback_decoder(params: &CodecParams, threads: usize) -> Result<TrackDecoder, ThumbError> {
    let mut decoder = TrackDecoder::with_threads(params, threads).map_err(failed)?;
    if params.codec == CodecId::Mjpeg {
        decoder
            .limit_mjpeg_allocation(MAX_PIXELS * 12)
            .map_err(failed)?;
    }
    Ok(decoder)
}

fn playback_packet_safe(params: &CodecParams, bytes: &[u8], keyframe: bool) -> bool {
    if bytes.is_empty() || bytes.len() as u64 > MAX_SAMPLE_BYTES || !supported(params.codec) {
        return false;
    }
    match params.codec {
        CodecId::ProRes => video::prores::probe(bytes)
            .is_ok_and(|header| size_allowed(u32::from(header.width), u32::from(header.height))),
        CodecId::Vp9 => {
            !keyframe
                || vp9_first_frame(bytes)
                    .and_then(video::vp9::decode::keyframe_info)
                    .is_some_and(|header| size_allowed(header.width, header.height))
        }
        CodecId::H264 => check_avc(params, bytes, keyframe),
        CodecId::Hevc => check_hevc(params, bytes, keyframe),
        CodecId::Av1 => check_av1(params, bytes, keyframe),
        CodecId::Vp8 => {
            (!keyframe && bytes[0] & 1 == 1)
                || video::vp8::keyframe_info(bytes)
                    .is_some_and(|header| size_allowed(header.width, header.height))
        }
        CodecId::Dnxhd => video::dnx::probe(bytes).is_ok_and(|header| {
            size_allowed(
                header.width,
                header
                    .lines
                    .saturating_mul(if header.frame_encoding { 1 } else { 2 }),
            )
        }),
        CodecId::Mpeg2Video => video::mpeg2::probe(bytes).map_or(!keyframe, |header| {
            size_allowed(header.width, header.height)
        }),
        CodecId::Mjpeg => video::mjpeg::is_packet(bytes),
        _ => false,
    }
}

fn vp9_first_frame(bytes: &[u8]) -> Option<&[u8]> {
    let marker = *bytes.last()?;
    if marker & 0xe0 != 0xc0 {
        return Some(bytes);
    }
    let frames = usize::from(marker & 7) + 1;
    let width = usize::from((marker >> 3) & 3) + 1;
    let Some(start) = bytes.len().checked_sub(2 + frames * width) else {
        return Some(bytes);
    };
    if bytes[start] != marker {
        return Some(bytes);
    }
    // A VP9 superframe ends with an index of little-endian subframe byte counts.
    let mut size = [0u8; 4];
    size[..width].copy_from_slice(&bytes[start + 1..start + 1 + width]);
    bytes[..start].get(..u32::from_le_bytes(size) as usize)
}

fn check_avc(params: &CodecParams, packet: &[u8], keyframe: bool) -> bool {
    use craft::media::bitstream::{annexb_nals, length_prefixed_nals, unescape_rbsp};
    use craft::video::h264::decode::params::Sps;
    let config = isobmff::AvcConfig::parse(&params.extradata).ok();
    let nals = if let Some(config) = &config {
        let Ok(nals) = length_prefixed_nals(packet, usize::from(config.length_size)) else {
            return false;
        };
        nals
    } else {
        annexb_nals(packet)
    };
    let mut parameter_sets = config
        .iter()
        .flat_map(|config| &config.sps)
        .map(Vec::as_slice)
        .chain(
            annexb_nals(&params.extradata)
                .into_iter()
                .filter(|nal| nal.first().is_some_and(|byte| byte & 31 == 7)),
        )
        .chain(
            nals.iter()
                .copied()
                .filter(|nal| nal.first().is_some_and(|byte| byte & 31 == 7)),
        )
        .peekable();
    (!keyframe || parameter_sets.peek().is_some())
        && parameter_sets.all(|nal| {
            nal.get(1..).is_some_and(|bytes| {
                Sps::parse(&unescape_rbsp(bytes)).is_ok_and(|sps| {
                    size_allowed(
                        sps.pic_width_in_mbs * 16,
                        sps.pic_height_in_map_units * if sps.frame_mbs_only { 16 } else { 32 },
                    )
                })
            })
        })
        && !nals.is_empty()
        && (!keyframe
            || nals
                .iter()
                .any(|nal| nal.first().is_some_and(|byte| byte & 31 == 5)))
}

fn check_hevc(params: &CodecParams, packet: &[u8], keyframe: bool) -> bool {
    use craft::media::bitstream::{annexb_nals, length_prefixed_nals, unescape_rbsp};
    use craft::video::hevc::decode::params::Sps;
    let config = isobmff::HevcConfig::parse(&params.extradata).ok();
    let nals = if let Some(config) = &config {
        let Ok(nals) = length_prefixed_nals(packet, usize::from(config.length_size)) else {
            return false;
        };
        nals
    } else {
        annexb_nals(packet)
    };
    let is_sps = |nal: &&[u8]| nal.first().is_some_and(|byte| ((byte >> 1) & 63) == 33);
    let mut parameter_sets = config
        .iter()
        .flat_map(|config| &config.arrays)
        .filter(|array| array.nal_type == 33)
        .flat_map(|array| array.nalus.iter().map(Vec::as_slice))
        .chain(annexb_nals(&params.extradata).into_iter().filter(is_sps))
        .chain(nals.iter().copied().filter(is_sps))
        .peekable();
    !nals.is_empty()
        && (!keyframe || parameter_sets.peek().is_some())
        && parameter_sets.all(|nal| {
            nal.get(2..).is_some_and(|bytes| {
                Sps::parse(&unescape_rbsp(bytes))
                    .is_ok_and(|sps| size_allowed(sps.width, sps.height))
            })
        })
}

fn check_av1(params: &CodecParams, packet: &[u8], keyframe: bool) -> bool {
    let config = isobmff::Av1Config::parse(&params.extradata).ok();
    let config = config
        .as_ref()
        .map_or(params.extradata.as_slice(), |config| &config.config_obus);
    let mut found = false;
    for mut data in [config, packet] {
        while let Some((&header, tail)) = data.split_first() {
            let Some(tail) = tail.get(usize::from(header & 4 != 0)..) else {
                return false;
            };
            let unit = if header & 2 != 0 {
                av1_obu_payload(tail)
            } else {
                Some((tail, &[][..]))
            };
            let Some((payload, remaining)) = unit else {
                return false;
            };
            if (header >> 3) & 15 == 1 {
                use craft::video::av1::decode::SequenceHeader;
                if !SequenceHeader::parse(payload)
                    .is_ok_and(|seq| size_allowed(seq.max_frame_width, seq.max_frame_height))
                {
                    return false;
                }
                found = true;
            }
            data = remaining;
        }
    }
    found || !keyframe
}

fn av1_obu_payload(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut length = 0u64;
    for (i, &byte) in bytes.iter().take(8).enumerate() {
        length |= u64::from(byte & 127) << (i * 7);
        if byte & 128 == 0 {
            return bytes[i + 1..].split_at_checked(usize::try_from(length).ok()?);
        }
    }
    None
}

fn sample(plane: &Plane, x: u32, y: u32) -> u32 {
    if let Some(row) = plane.row_u8(y) {
        return row.get(x as usize).copied().map_or(0, u32::from);
    }
    plane
        .row_u16(y)
        .and_then(|row| row.get(x as usize))
        .copied()
        .map_or(0, u32::from)
}

fn poster(frame: &VideoFrame, rotation: u8, edge: u32) -> Result<Image, ThumbError> {
    let (width, height) = (frame.width(), frame.height());
    if !size_allowed(width, height) {
        return Err(ThumbError::Unsupported);
    }
    let (w, h) = if rotation.is_multiple_of(2) {
        (width, height)
    } else {
        (height, width)
    };
    let scale = edge.min(w.max(h));
    let (out_w, out_h) = ((w * scale / w.max(h)).max(1), (h * scale / w.max(h)).max(1));
    let mut pixels = Vec::with_capacity((out_w * out_h * 4) as usize);
    let range = if frame.color.full_range {
        Range::Full
    } else {
        Range::Limited
    };
    let matrix = Matrix::from_code(frame.color.matrix).unwrap_or_default();
    let subsampling = frame.subsampling();
    let planes = frame.planes();
    let luma = |plane: &Plane, x, y| {
        normalize_y(sample(plane, x, y), u32::from(plane.bit_depth()), range).unwrap_or(0.0)
    };
    let chroma = |plane: &Plane, x, y| {
        normalize_c(
            sample(plane, x >> subsampling.x, y >> subsampling.y),
            u32::from(plane.bit_depth()),
            range,
        )
        .unwrap_or(0.0)
    };
    let full = |plane: &Plane, x, y| {
        normalize_y(
            sample(plane, x, y),
            u32::from(plane.bit_depth()),
            Range::Full,
        )
        .unwrap_or(0.0)
    };
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    for y in 0..out_h {
        for x in 0..out_w {
            let (px, py) = (x * w / out_w, y * h / out_h);
            let (x, y) = match rotation {
                1 => (py, height - 1 - px),
                2 => (width - 1 - px, height - 1 - py),
                3 => (width - 1 - py, px),
                _ => (px, py),
            };
            let rgb = match frame.family() {
                ColorFamily::Gray => [luma(&planes[0], x, y); 3],
                ColorFamily::Rgb => [
                    full(&planes[0], x, y),
                    full(&planes[1], x, y),
                    full(&planes[2], x, y),
                ],
                ColorFamily::YCbCr => ycbcr_to_rgb(
                    luma(&planes[0], x, y),
                    chroma(&planes[1], x, y),
                    chroma(&planes[2], x, y),
                    matrix,
                ),
            };
            pixels.extend(rgb.map(byte));
            pixels.push(frame.alpha().map_or(255, |plane| byte(full(plane, x, y))));
        }
    }
    Image::from_u8(out_w, out_h, ChannelLayout::Rgba, pixels).map_err(failed)
}

#[path = "desktop_video_stream.rs"]
pub mod stream;
