//! Play Craft-decoded PCM through the system PipeWire client when available.
use assetcraft::{Format, local_file};
use craft::audio::{DecodeOptions, Decoder, wav_range::WavRange};
use craft::media::ReadSeekSource;
use std::{
    fs::File,
    path::{Path, PathBuf},
    time::Instant,
};

use craft::host::{Host, Wake};

#[cfg(target_os = "linux")]
use std::{
    io::{BufRead, BufReader, Write},
    os::{fd::OwnedFd, unix::net::UnixStream},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    time::Duration,
};

#[cfg(target_os = "linux")]
use craft::host::queue::{JobQueue, Priority, QueueOptions};

use craft::host::queue::Cancel;

#[cfg(target_os = "linux")]
use crate::desktop_movie_audio::MovieAudio;

pub struct Sound {
    #[cfg(target_os = "linux")]
    jobs: JobQueue<u64, Result<(), String>>,
    #[cfg(target_os = "linux")]
    key: u64,
    #[cfg(target_os = "linux")]
    child: Arc<Mutex<Option<(u64, Child)>>>,
    #[cfg(target_os = "linux")]
    volume: Arc<AtomicU32>,
    #[cfg(target_os = "linux")]
    started: Arc<Mutex<Option<(u64, Instant)>>>,
    #[cfg(target_os = "linux")]
    wake: Wake,
}

impl Sound {
    pub fn new(host: &Host, wake: Wake) -> Self {
        #[cfg(not(target_os = "linux"))]
        let _ = (host, wake);
        Self {
            #[cfg(target_os = "linux")]
            jobs: JobQueue::new(
                host.jobs_shared().clone(),
                wake.clone(),
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
            #[cfg(target_os = "linux")]
            started: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "linux")]
            wake,
        }
    }

    pub fn available() -> bool {
        cfg!(target_os = "linux")
    }

    pub fn start(&mut self, path: PathBuf, format: Format, position: i64, offset: i64) {
        self.stop();
        #[cfg(not(target_os = "linux"))]
        let _ = (path, format, position, offset);
        #[cfg(target_os = "linux")]
        {
            let key = self.key;
            let started = Arc::clone(&self.started);
            let wake = self.wake.clone();
            let child = Arc::clone(&self.child);
            let volume = Arc::clone(&self.volume);
            self.jobs.submit(key, Priority::VISIBLE, move |cancel| {
                let result = (|| {
                    let mut decoder = AudioReader::open(&path, format, cancel)?;
                    let (rate, channels, _) = decoder.info();
                    if channels == 0 || channels > 8 || !(8_000..=192_000).contains(&rate) {
                        return Err(String::from(
                            "Audio channel count or sample rate is outside the playback range",
                        ));
                    }
                    let relative = position.saturating_sub(offset);
                    let frames =
                        |ns: u64| (u128::from(ns) * u128::from(rate) / 1_000_000_000) as u64;
                    if relative > 0 {
                        decoder.seek(frames(relative as u64))?;
                    }
                    let mut silence = if relative < 0 {
                        frames(relative.unsigned_abs())
                    } else {
                        0
                    };
                    let mut bytes = Vec::new();
                    fill_pcm(&mut decoder, &mut bytes, &mut silence, channels, &volume)?;

                    let (log, mut command) = pipewire_command(rate, channels)?;
                    let mut stdin = {
                        let mut active = child.lock().map_err(|e| e.to_string())?;
                        if cancel.is_cancelled() {
                            return Ok(());
                        }
                        let mut process = command.spawn().map_err(|error| {
                            format!("PipeWire audio output is unavailable: {error}")
                        })?;
                        let stdin = process.stdin.take();
                        *active = Some((key, process));
                        stdin
                            .ok_or_else(|| String::from("Could not open the system audio stream"))?
                    };
                    drop(command);
                    let (streaming, ready) = mpsc::sync_channel(1);
                    // Keep draining the client diagnostics while the stream plays.
                    std::thread::Builder::new()
                        .name("PipeWire preview status".into())
                        .spawn(move || {
                            let mut streaming = Some(streaming);
                            let mut reader = BufReader::new(log);
                            let mut line = String::new();
                            while let Ok(size) = reader.read_line(&mut line) {
                                if size == 0 {
                                    break;
                                }
                                if line.trim_end().ends_with(" -> streaming")
                                    && let Some(streaming) = streaming.take()
                                {
                                    let _ = streaming.send(());
                                }
                                line.clear();
                            }
                        })
                        .map_err(|error| error.to_string())?;
                    if !wait_for_stream(&ready, cancel)? {
                        return Ok(());
                    }
                    let write = |stdin: &mut std::process::ChildStdin, bytes: &[u8]| {
                        stdin
                            .write_all(bytes)
                            .map_err(|error| format!("Could not send samples to PipeWire: {error}"))
                    };
                    let first = bytes
                        .len()
                        .min((rate as usize / 100) * usize::from(channels) * 2);
                    write(&mut stdin, &bytes[..first])?;
                    if !cancel.is_cancelled() {
                        *started.lock().map_err(|error| error.to_string())? =
                            Some((key, Instant::now() + OUTPUT_LATENCY));
                        wake.wake();
                    }
                    write(&mut stdin, &bytes[first..])?;
                    while !cancel.is_cancelled() {
                        if !fill_pcm(&mut decoder, &mut bytes, &mut silence, channels, &volume)? {
                            break;
                        }
                        write(&mut stdin, &bytes)?;
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
                    match process.try_wait() {
                        Ok(Some(status)) => {
                            *active = None;
                            if !status.success() {
                                return Err(String::from("The system audio stream stopped"));
                            }
                            break;
                        }
                        Err(error) => {
                            let _ = process.kill();
                            let _ = process.wait();
                            *active = None;
                            return Err(error.to_string());
                        }
                        Ok(None) => {}
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
            self.key = self.key.wrapping_add(1);
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

    pub fn receive_start(&self) -> Option<Instant> {
        #[cfg(not(target_os = "linux"))]
        return None;
        #[cfg(target_os = "linux")]
        self.started
            .lock()
            .ok()?
            .take()
            .and_then(|(key, at)| (self.key == key).then_some(at))
    }

    pub fn receive_error(&self) -> Option<String> {
        #[cfg(not(target_os = "linux"))]
        return None;
        #[cfg(target_os = "linux")]
        std::iter::from_fn(|| self.jobs.try_recv())
            .find(|result| result.key == self.key)
            .and_then(|result| result.result.err())
    }
}

impl Drop for Sound {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "linux")]
const OUTPUT_LATENCY: Duration = Duration::from_millis(50);

#[cfg(target_os = "linux")]
fn pipewire_command(rate: u32, channels: u16) -> Result<(UnixStream, Command), String> {
    static RAW_FLAG: OnceLock<bool> = OnceLock::new();
    let raw = if let Some(raw) = RAW_FLAG.get() {
        *raw
    } else {
        let help = Command::new("pw-cat")
            .arg("--help")
            .output()
            .map_err(|error| format!("PipeWire audio output is unavailable: {error}"))?;
        let raw = [&help.stdout, &help.stderr]
            .into_iter()
            .any(|text| String::from_utf8_lossy(text).contains("--raw"));
        let _ = RAW_FLAG.set(raw);
        raw
    };
    let mut command = if raw {
        let mut command = Command::new("pw-cat");
        command.arg("--raw");
        command
    } else {
        // PipeWire 1.0 uses raw stdin for "-", and reports stream state on stdout.
        let mut command = Command::new("stdbuf");
        command.args(["-oL", "pw-cat"]);
        command
    };
    let (log, output) = UnixStream::pair().map_err(|error| error.to_string())?;
    let stderr = output.try_clone().map_err(|error| error.to_string())?;
    command
        .args(["--verbose", "--playback", "--latency"])
        .arg(format!("{}ms", OUTPUT_LATENCY.as_millis()))
        .args(["--format", "s16", "--rate"])
        .arg(rate.to_string())
        .arg("--channels")
        .arg(channels.to_string())
        .arg("-")
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::from(OwnedFd::from(output)))
        .stderr(Stdio::from(OwnedFd::from(stderr)));
    Ok((log, command))
}

#[cfg(target_os = "linux")]
fn wait_for_stream(ready: &mpsc::Receiver<()>, cancel: &Cancel) -> Result<bool, String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !cancel.is_cancelled() {
        match ready.recv_timeout(Duration::from_millis(20)) {
            Ok(()) => return Ok(true),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(String::from(
                    "The system audio stream stopped before playback",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                return Err(String::from("PipeWire did not start the audio stream"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    Ok(false)
}

#[cfg(target_os = "linux")]
fn fill_pcm(
    decoder: &mut AudioReader,
    bytes: &mut Vec<u8>,
    silence: &mut u64,
    channels: u16,
    volume: &AtomicU32,
) -> Result<bool, String> {
    bytes.clear();
    if *silence > 0 {
        let frames = (*silence).min(2048);
        *silence -= frames;
        bytes.resize(frames as usize * usize::from(channels) * 2, 0);
        return Ok(true);
    }
    decoder.read_pcm(bytes, f32::from_bits(volume.load(Ordering::Relaxed)))
}

enum AudioReader {
    #[cfg(target_os = "linux")]
    Movie(Box<MovieAudio>),
    Decode(Box<Decoder<'static>>),
    Wav {
        wav: WavRange<ReadSeekSource<File>>,
        #[cfg(target_os = "linux")]
        at: u64,
    },
}

impl AudioReader {
    fn open(path: &Path, format: Format, cancel: &Cancel) -> Result<Self, String> {
        let file = local_file::open_read(path).map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err(String::from("Original unavailable"));
        }
        #[cfg(not(target_os = "linux"))]
        let _ = cancel;
        #[cfg(target_os = "linux")]
        if matches!(format, Format::Mp4 | Format::Mov) {
            return Ok(Self::Movie(Box::new(MovieAudio::open(file, cancel)?)));
        }
        if metadata.len() > crate::desktop_video::AUDIO_FALLBACK_MAX_BYTES {
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
            #[cfg(target_os = "linux")]
            Self::Movie(movie) => movie.info(),
            Self::Wav { wav, .. } => (wav.rate, wav.channels, Some(wav.frames)),
        }
    }

    #[cfg(target_os = "linux")]
    fn seek(&mut self, frame: u64) -> Result<(), String> {
        match self {
            Self::Movie(movie) => {
                movie.seek(frame);
                Ok(())
            }
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
            Self::Movie(movie) => return movie.read(&mut write),
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

pub fn duration(path: &Path, format: Format, cancel: &Cancel) -> Result<i64, String> {
    let reader = AudioReader::open(path, format, cancel)?;
    let (rate, _, frames) = reader.info();
    frames
        .filter(|_| rate > 0)
        .and_then(|frames| {
            i64::try_from(u128::from(frames) * 1_000_000_000 / u128::from(rate)).ok()
        })
        .filter(|duration| *duration > 0)
        .ok_or_else(|| String::from("Duration is unavailable for this audio stream"))
}
