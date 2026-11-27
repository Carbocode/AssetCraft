//! Range-backed audio for MP4 and QuickTime playback.
use craft::audio::{LOOKAHEAD_PACKETS, TrackDecoder};
use craft::host::queue::Cancel;
use craft::media::{ByteSource, ReadSeekSource, isobmff};
use std::fs::File;

const MAX_PACKET_BYTES: u32 = 4 << 20;
const MAX_PREROLL_PACKETS: usize = 512;

pub(super) struct MovieAudio {
    source: ReadSeekSource<File>,
    movie: isobmff::Mp4File,
    track: usize,
    decoder: TrackDecoder,
    next: usize,
    from: u64,
    skip: u64,
    end: u64,
}

impl MovieAudio {
    pub(super) fn open(file: File, cancel: &Cancel) -> Result<Self, String> {
        let movie = crate::desktop_video::open_movie(&file, cancel)
            .map_err(|error| format!("Could not read the audio index: {error:?}"))?;
        let source = ReadSeekSource::new(file).map_err(|error| error.to_string())?;
        let track = movie
            .track_of_kind(isobmff::TrackKind::Audio)
            .ok_or_else(|| String::from("No audio track"))?;
        let audio = &movie.tracks[track];
        if audio.samples.is_empty()
            || audio.timescale == 0
            || audio.samples.iter().any(|sample| {
                sample.size > MAX_PACKET_BYTES
                    || sample
                        .offset
                        .checked_add(u64::from(sample.size))
                        .is_none_or(|end| end > source.len())
            })
            || !audio.samples.is_sorted_by_key(|sample| sample.dts)
        {
            return Err(String::from(
                "Movie audio packets exceed the preview limits",
            ));
        }
        let lookahead = (0..audio.samples.len().min(LOOKAHEAD_PACKETS))
            .map(|index| movie.read_sample(&source, track, index))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let packets = lookahead.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let decoder = TrackDecoder::new(&audio.codec_params(), &packets)
            .map_err(|error| error.to_string())?;
        let rate = decoder.sample_rate();
        let last = audio
            .samples
            .last()
            .ok_or_else(|| String::from("No audio packets"))?;
        let media_end = ticks_to_frames(
            last.dts
                .saturating_sub(audio.samples[0].dts)
                .unsigned_abs()
                .saturating_add(u64::from(last.duration)),
            audio.timescale,
            rate,
        );
        let edit = match audio.edits.as_slice() {
            [edit] => Some(edit),
            [empty, edit] if empty.media_time < 0 => Some(edit),
            _ => None,
        };
        let (skip, end) = edit
            .filter(|edit| {
                edit.media_time >= 0 && edit.media_rate == 0x0001_0000 && movie.timescale > 0
            })
            .map_or((0, media_end), |edit| {
                let skip = ticks_to_frames(edit.media_time.unsigned_abs(), audio.timescale, rate);
                let end = if edit.segment_duration == 0 {
                    media_end
                } else {
                    skip.saturating_add(ticks_to_frames(
                        edit.segment_duration,
                        movie.timescale,
                        rate,
                    ))
                    .min(media_end)
                };
                (skip, end)
            });
        let mut reader = Self {
            source,
            movie,
            track,
            decoder,
            next: 0,
            from: skip,
            skip,
            end,
        };
        reader.seek(0);
        Ok(reader)
    }

    pub(super) fn info(&self) -> (u32, u16, Option<u64>) {
        (
            self.decoder.sample_rate(),
            self.decoder.channels(),
            Some(self.end.saturating_sub(self.skip)),
        )
    }

    pub(super) fn seek(&mut self, frame: u64) {
        let track = &self.movie.tracks[self.track];
        self.from = self.skip.saturating_add(frame).min(self.end);
        let decode_from = self.from.saturating_sub(self.decoder.preroll() as u64);
        let base = track.samples[0].dts;
        self.next = track
            .samples
            .partition_point(|sample| {
                ticks_to_frames(
                    sample.dts.saturating_sub(base).unsigned_abs(),
                    track.timescale,
                    self.decoder.sample_rate(),
                ) <= decode_from
            })
            .saturating_sub(1);
        self.decoder.reset();
    }

    pub(super) fn read(&mut self, write: &mut impl FnMut(f32)) -> Result<bool, String> {
        let track = &self.movie.tracks[self.track];
        for _ in 0..MAX_PREROLL_PACKETS {
            if self.next >= track.samples.len() || self.from >= self.end {
                return Ok(false);
            }
            let start = sample_start(track, self.next, self.decoder.sample_rate());
            if start >= self.end {
                return Ok(false);
            }
            let packet = self
                .movie
                .read_sample(&self.source, self.track, self.next)
                .map_err(|error| error.to_string())?;
            self.next += 1;
            let block = self
                .decoder
                .decode(&packet)
                .map_err(|error| error.to_string())?;
            let from = self.from.saturating_sub(start).min(block.frames() as u64) as usize;
            let end = (self.end - start).min(block.frames() as u64) as usize;
            if from < end {
                for frame in from..end {
                    for channel in &block.channels {
                        write(channel[frame]);
                    }
                }
                return Ok(true);
            }
        }
        Err(String::from("Audio seek exceeds the preview packet limit"))
    }
}

fn sample_start(track: &isobmff::Track, index: usize, rate: u32) -> u64 {
    let ticks = track.samples[index]
        .dts
        .saturating_sub(track.samples[0].dts)
        .unsigned_abs();
    ticks_to_frames(ticks, track.timescale, rate)
}

fn ticks_to_frames(ticks: u64, scale: u32, rate: u32) -> u64 {
    u64::try_from(u128::from(ticks) * u128::from(rate) / u128::from(scale)).unwrap_or(u64::MAX)
}
