use std::fs::File;

use assetcraft::Format;
use craft::audio::{AudioBuffer, DecodeOptions, Decoder, wav_range::WavRange};
use craft::browse::thumbs::ThumbError;
use craft::host::queue::Cancel;
use craft::image_codecs::{ChannelLayout, Image};
use craft::media::ReadSeekSource;

use crate::desktop_documents::{StillPreview, failed};

pub type Waveform = Vec<[f32; 2]>;

pub fn render(file: File, format: Format, edge: u32, cancel: &Cancel) -> StillPreview {
    match inspect(file, format, edge, cancel) {
        Ok((mut preview, peaks)) => {
            if let Some(peaks) = peaks {
                preview.image = waveform_image(&peaks);
            }
            preview
        }
        Err(error) => {
            let mut preview = StillPreview::new(Err(error.clone()));
            if let ThumbError::Failed(reason) = error {
                preview.property("Preview", reason);
            }
            preview
        }
    }
}

pub fn inspect(
    file: File,
    format: Format,
    edge: u32,
    cancel: &Cancel,
) -> Result<(StillPreview, Option<Waveform>), ThumbError> {
    if file.metadata().map_err(failed)?.len() <= 64 << 20 {
        audio(file, format, edge, cancel)
    } else if format == Format::Wav {
        large_wav(file, edge, cancel)
    } else {
        Err(failed("Original exceeds the audio preview read limit"))
    }
}

fn audio(
    file: File,
    format: Format,
    edge: u32,
    cancel: &Cancel,
) -> Result<(StillPreview, Option<Waveform>), ThumbError> {
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
    let peaks = waveform(
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
    Ok((preview, peaks))
}

fn large_wav(
    file: File,
    edge: u32,
    cancel: &Cancel,
) -> Result<(StillPreview, Option<Waveform>), ThumbError> {
    let source = ReadSeekSource::new(file).map_err(failed)?;
    let wav = WavRange::open(source, 64, 384_000).map_err(failed)?;
    let mut preview = audio_info("PCM", wav.rate, wav.channels, Some(wav.frames));
    let peaks = waveform(&mut preview, edge, Some(wav.frames), cancel, |frame| {
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
    Ok((preview, peaks))
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
) -> Result<Option<Waveform>, ThumbError> {
    let width = edge.clamp(64, 768);
    let mut peaks = vec![[0.0; 2]; width as usize];
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
                return Ok(None);
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
            peaks[x as usize] = [low.clamp(-1.0, 1.0), high.clamp(-1.0, 1.0)];
        }
    }
    if sampled {
        preview.property(
            "Waveform",
            if length.is_some() {
                "Sampled across the track"
            } else {
                "Opening audio block"
            },
        );
    }
    Ok(sampled.then_some(peaks))
}

fn waveform_image(peaks: &Waveform) -> Result<Image, ThumbError> {
    let width = peaks.len();
    let height = width / 3;
    let mut rgba = [242, 246, 250, 255].repeat(width * height);
    let centre = height / 2;
    let row = |sample: f32| (centre as f32 - sample * (centre - 2) as f32) as usize;
    for (x, &[low, high]) in peaks.iter().enumerate() {
        for y in row(high)..=row(low) {
            let offset = (y * width + x) * 4;
            rgba[offset..offset + 4].copy_from_slice(&[40, 93, 144, 255]);
        }
    }
    Image::from_u8(width as u32, height as u32, ChannelLayout::Rgba, rgba).map_err(failed)
}
