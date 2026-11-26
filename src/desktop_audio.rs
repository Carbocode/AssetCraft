use std::fs::File;

use assetcraft::Format;
use craft::audio::{AudioBuffer, DecodeOptions, Decoder, wav_range::WavRange};
use craft::browse::thumbs::ThumbError;
use craft::host::queue::Cancel;
use craft::image_codecs::{ChannelLayout, Image};
use craft::media::ReadSeekSource;

use crate::desktop_documents::{StillPreview, failed};

pub fn render(file: File, format: Format, edge: u32, cancel: &Cancel) -> StillPreview {
    let result = (|| {
        if file.metadata().map_err(failed)?.len() > 64 << 20 {
            if format == Format::Wav {
                return large_wav(file, edge, cancel);
            }
            return Err(failed("Original exceeds the audio preview read limit"));
        }
        audio(file, format, edge, cancel)
    })();
    match result {
        Ok(preview) => preview,
        Err(error) => {
            let mut preview = StillPreview::new(Err(error.clone()));
            if let ThumbError::Failed(reason) = error {
                preview.property("Preview", reason);
            }
            preview
        }
    }
}

fn audio(
    file: File,
    format: Format,
    edge: u32,
    cancel: &Cancel,
) -> Result<StillPreview, ThumbError> {
    let mut decoder =
        Decoder::open(file, Some(format.as_str()), &DecodeOptions::default()).map_err(failed)?;
    let info = decoder.info().clone();
    let known = decoder.length_known();
    let mut preview = audio_info(
        info.codec.name(),
        info.sample_rate,
        info.channels,
        known.then_some(info.frames),
    );
    if let Some(bwf) = &info.bwf
        && !bwf.description.is_empty()
    {
        preview.property("Description", &bwf.description);
    }
    waveform(
        &mut preview,
        edge,
        known.then_some(info.frames),
        cancel,
        |frame| {
            if frame > 0 {
                decoder.seek(frame).map_err(failed)?;
            }
            decoder.next_block().map_err(failed)
        },
    )?;
    Ok(preview)
}

fn large_wav(file: File, edge: u32, cancel: &Cancel) -> Result<StillPreview, ThumbError> {
    let source = ReadSeekSource::new(file).map_err(failed)?;
    let wav = WavRange::open(source, 64, 384_000).map_err(failed)?;
    let mut preview = audio_info("PCM", wav.rate, wav.channels, Some(wav.frames));
    waveform(&mut preview, edge, Some(wav.frames), cancel, |frame| {
        let count = wav.frames.saturating_sub(frame).min(4096);
        if count == 0 {
            return Ok(None);
        }
        let raw = wav.read_frames(frame, frame + count - 1).map_err(failed)?;
        let mut block = AudioBuffer::new(wav.rate, wav.channels, count as usize);
        for (channel, samples) in block.channels.iter_mut().enumerate() {
            for (i, sample) in samples.iter_mut().enumerate() {
                *sample = wav.sample(&raw, i, channel);
            }
        }
        Ok(Some(block))
    })?;
    Ok(preview)
}

fn audio_info(codec: &str, rate: u32, channels: u16, frames: Option<u64>) -> StillPreview {
    let mut preview = StillPreview::new(Err(ThumbError::Unsupported));
    preview.property("Audio codec", codec);
    preview.property("Sample rate", format!("{rate} Hz"));
    preview.property("Channels", channels.to_string());
    if let Some(frames) = frames.filter(|_| rate != 0) {
        preview.property(
            "Duration",
            format!("{:.3} s", frames as f64 / f64::from(rate)),
        );
    }
    preview
}

fn waveform(
    preview: &mut StillPreview,
    edge: u32,
    length: Option<u64>,
    cancel: &Cancel,
    mut sample_at: impl FnMut(u64) -> Result<Option<AudioBuffer>, ThumbError>,
) -> Result<(), ThumbError> {
    let width = edge.clamp(64, 768);
    let height = width / 3;
    let mut rgba = [242, 246, 250, 255].repeat((width * height) as usize);
    let centre = height / 2;
    let span = length.unwrap_or(0);
    // Read at most a short block per strip when sampling a long track.
    let strips = if length.is_some() { width.min(32) } else { 1 };
    let mut sampled = false;
    for strip in 0..strips {
        if cancel.is_cancelled() {
            return Err(ThumbError::Cancelled);
        }
        let first = strip * width / strips;
        let last = (strip + 1) * width / strips;
        let frame = (u128::from(span) * u128::from(strip) / u128::from(strips)) as u64;
        let block = match sample_at(frame) {
            Ok(Some(block)) => block,
            Ok(None) => continue,
            Err(error) => {
                preview.image = Err(error.clone());
                if let ThumbError::Failed(reason) = error {
                    preview.property("Preview", reason);
                }
                break;
            }
        };
        let frames = block.frames();
        if frames == 0 {
            continue;
        }
        sampled = true;
        let columns = (last - first) as usize;
        for (i, x) in (first..last).enumerate() {
            let start = i * frames / columns;
            let end = ((i + 1) * frames / columns).max(start + 1).min(frames);
            let mut low = 0.0f32;
            let mut high = 0.0f32;
            for channel in &block.channels {
                let Some(slice) = channel.get(start..end) else {
                    continue;
                };
                for &sample in slice {
                    low = low.min(sample);
                    high = high.max(sample);
                }
            }
            let row = |sample: f32| {
                (centre as f32 - sample.clamp(-1.0, 1.0) * (centre - 2) as f32) as u32
            };
            for y in row(high)..=row(low) {
                let offset = ((y * width + x) * 4) as usize;
                rgba[offset..offset + 4].copy_from_slice(&[40, 93, 144, 255]);
            }
        }
    }
    if sampled {
        preview.image = Image::from_u8(width, height, ChannelLayout::Rgba, rgba).map_err(failed);
        preview.property(
            "Waveform",
            if length.is_some() {
                "Sampled across the track"
            } else {
                "Opening audio block"
            },
        );
    }
    Ok(())
}
