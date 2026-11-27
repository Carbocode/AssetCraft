//! Play Craft-decoded PCM through the system PipeWire client when available.
use assetcraft::{Format, local_file};
use craft::audio::{DecodeOptions, Decoder, wav_range::WavRange};
use craft::media::ReadSeekSource;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

use craft::host::{Host, Wake};

#[cfg(target_os = "linux")]
use std::{
    io::Write,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

#[cfg(target_os = "linux")]
use craft::host::queue::{JobQueue, Priority, QueueOptions};

pub struct Sound {
    #[cfg(target_os = "linux")]
    jobs: JobQueue<u64, Result<(), String>>,
    #[cfg(target_os = "linux")]
    key: u64,
    #[cfg(target_os = "linux")]
    child: Arc<Mutex<Option<(u64, Child)>>>,
    #[cfg(target_os = "linux")]
    volume: Arc<AtomicU32>,
}

impl Sound {
    pub fn new(host: &Host, wake: Wake) -> Self {
        #[cfg(not(target_os = "linux"))]
        let _ = (host, wake);
        Self {
            #[cfg(target_os = "linux")]
            jobs: JobQueue::new(
                host.jobs_shared().clone(),
                wake,
                QueueOptions {
                    lanes: 1,
                    name: "Play preview audio",
                    ..Default::default()
                },
            ),
            #[cfg(target_os = "linux")]
            key: 0,
            #[cfg(target_os = "linux")]
            child: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "linux")]
            volume: Arc::new(AtomicU32::new(1.0f32.to_bits())),
        }
    }

    pub fn available() -> bool {
        cfg!(target_os = "linux")
    }

    pub fn start(&mut self, path: PathBuf, format: Format, position: i64) {
        self.stop();
        #[cfg(not(target_os = "linux"))]
        let _ = (path, format, position);
        #[cfg(target_os = "linux")]
        {
            self.key = self.key.wrapping_add(1);
            let key = self.key;
            let child = Arc::clone(&self.child);
            let volume = Arc::clone(&self.volume);
            self.jobs.submit(key, Priority::VISIBLE, move |cancel| {
                let result = (|| {
                    let mut decoder = AudioReader::open(&path, format)?;
                    let (rate, channels, _) = decoder.info();
                    if channels == 0 || channels > 8 || !(8_000..=192_000).contains(&rate) {
                        return Err(String::from(
                            "Audio channel count or sample rate is outside the playback range",
                        ));
                    }
                    if position > 0 {
                        let frame =
                            (u128::from(position as u64) * u128::from(rate) / 1_000_000_000) as u64;
                        decoder.seek(frame)?;
                    }
                    let mut stdin = {
                        let mut active = child.lock().map_err(|e| e.to_string())?;
                        if cancel.is_cancelled() {
                            return Ok(());
                        }
                        let mut process = Command::new("pw-cat")
                            .args([
                                "--playback",
                                "--raw",
                                "--latency",
                                "50ms",
                                "--format",
                                "s16",
                                "--rate",
                            ])
                            .arg(rate.to_string())
                            .arg("--channels")
                            .arg(channels.to_string())
                            .arg("-")
                            .stdin(Stdio::piped())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()
                            .map_err(|error| {
                                format!("PipeWire audio output is unavailable: {error}")
                            })?;
                        let stdin = process.stdin.take();
                        *active = Some((key, process));
                        stdin
                            .ok_or_else(|| String::from("Could not open the system audio stream"))?
                    };
                    let mut bytes = Vec::new();
                    while !cancel.is_cancelled() {
                        let volume = f32::from_bits(volume.load(Ordering::Relaxed));
                        bytes.clear();
                        if !decoder.read_pcm(&mut bytes, volume)? {
                            break;
                        }
                        stdin.write_all(&bytes).map_err(|error| {
                            format!("Could not send samples to PipeWire: {error}")
                        })?;
                    }
                    drop(stdin);
                    Ok(())
                })();
                loop {
                    let Ok(mut active) = child.lock() else {
                        break;
                    };
                    let Some((running, process)) = active.as_mut() else {
                        break;
                    };
                    if *running != key {
                        break;
                    }
                    if cancel.is_cancelled() || result.is_err() {
                        let _ = process.kill();
                        let _ = process.wait();
                        *active = None;
                        break;
                    }
                    if let Ok(Some(status)) = process.try_wait() {
                        *active = None;
                        if !status.success() {
                            return Err(String::from("The system audio stream stopped"));
                        }
                        break;
                    }
                    drop(active);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                result
            });
        }
    }

    pub fn stop(&mut self) {
        #[cfg(target_os = "linux")]
        {
            self.jobs.reprioritize(|_, _| None);
            if let Ok(mut active) = self.child.lock()
                && let Some((_, mut child)) = active.take()
            {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    pub fn set_volume(&self, volume: f64) {
        #[cfg(not(target_os = "linux"))]
        let _ = volume;
        #[cfg(target_os = "linux")]
        self.volume
            .store((volume as f32).to_bits(), Ordering::Relaxed);
    }

    pub fn receive_error(&self) -> Option<String> {
        #[cfg(not(target_os = "linux"))]
        return None;
        #[cfg(target_os = "linux")]
        self.jobs.try_recv().and_then(|result| result.result.err())
    }
}

impl Drop for Sound {
    fn drop(&mut self) {
        self.stop();
    }
}

enum AudioReader {
    Decode(Box<Decoder<'static>>),
    Wav {
        wav: WavRange<ReadSeekSource<File>>,
        #[cfg(target_os = "linux")]
        at: u64,
    },
}

impl AudioReader {
    fn open(path: &Path, format: Format) -> Result<Self, String> {
        let file = local_file::open_read(path).map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err(String::from("Original unavailable"));
        }
        if metadata.len() > 128 << 20 {
            if format != Format::Wav {
                // Some Craft fallback decoders read the entire file.
                return Err(String::from("Audio preview exceeds the file size limit"));
            }
            let source = ReadSeekSource::new(file).map_err(|e| e.to_string())?;
            let wav = WavRange::open(source, 8, 192_000).map_err(|e| e.to_string())?;
            return Ok(Self::Wav {
                wav,
                #[cfg(target_os = "linux")]
                at: 0,
            });
        }
        Decoder::open(
            file,
            path.extension().and_then(|ext| ext.to_str()),
            &DecodeOptions::default(),
        )
        .map(|decoder| Self::Decode(Box::new(decoder)))
        .map_err(|e| e.to_string())
    }

    fn info(&self) -> (u32, u16, Option<u64>) {
        match self {
            Self::Decode(decoder) => {
                let info = decoder.info();
                (
                    info.sample_rate,
                    info.channels,
                    decoder.length_known().then_some(info.frames),
                )
            }
            Self::Wav { wav, .. } => (wav.rate, wav.channels, Some(wav.frames)),
        }
    }

    #[cfg(target_os = "linux")]
    fn seek(&mut self, frame: u64) -> Result<(), String> {
        match self {
            Self::Decode(decoder) => decoder.seek(frame).map(|_| ()).map_err(|e| e.to_string()),
            Self::Wav { wav, at } => {
                *at = frame.min(wav.frames);
                Ok(())
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn read_pcm(&mut self, bytes: &mut Vec<u8>, volume: f32) -> Result<bool, String> {
        let mut write = |sample: f32| {
            let sample = ((sample * volume).clamp(-1.0, 1.0) * 32767.0).round() as i16;
            bytes.extend_from_slice(&sample.to_le_bytes());
        };
        match self {
            Self::Decode(decoder) => {
                let Some(block) = decoder.next_block().map_err(|e| e.to_string())? else {
                    return Ok(false);
                };
                for frame in 0..block.frames() {
                    for channel in &block.channels {
                        write(channel[frame]);
                    }
                }
            }
            Self::Wav { wav, at } => {
                let count = wav.frames.saturating_sub(*at).min(2048);
                if count == 0 {
                    return Ok(false);
                }
                let raw = wav
                    .read_frames(*at, *at + count - 1)
                    .map_err(|e| e.to_string())?;
                for frame in 0..count as usize {
                    for channel in 0..usize::from(wav.channels) {
                        write(wav.sample(&raw, frame, channel));
                    }
                }
                *at += count;
            }
        }
        Ok(true)
    }
}

pub fn duration(path: &Path, format: Format) -> Result<i64, String> {
    let reader = AudioReader::open(path, format)?;
    let (rate, _, frames) = reader.info();
    frames
        .filter(|_| rate > 0)
        .and_then(|frames| {
            i64::try_from(u128::from(frames) * 1_000_000_000 / u128::from(rate)).ok()
        })
        .filter(|duration| *duration > 0)
        .ok_or_else(|| String::from("Duration is unavailable for this audio stream"))
}
