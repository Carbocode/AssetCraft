use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use assetcraft::{Asset, Format, local_file};
use craft::behavior::EditPolicy;
use craft::browse::thumbs::ThumbError;
use craft::egui::{
    self, Color32, ColorImage, Context, Id, Rect, Sense, TextureHandle, TextureOptions, Ui,
};
use craft::host::queue::{JobQueue, Priority, QueueOptions};
use craft::host::{Host, Wake};
use craft::i18n::Word;
use craft::image_codecs::Image;
use craft::theme::{
    ControlSize, Srgba, TypeRole,
    variant::{Tone, Variant},
};
use craft::ui::contract::{Bind, Props, Show, Sizing};
use craft::ui::display::{Elide, Shade, Text};
use craft::ui::notices::Notice;
use craft::ui::runtime::{
    env::{Env, in_room},
    focus,
};
use craft::ui::value_drag::Slider;

use crate::{
    chrome_icon,
    desktop_audio::{self, Waveform},
    desktop_sound::{self, Sound},
    desktop_thumbnails::decode_preview,
    desktop_transport::Transport,
    desktop_video::stream::{Frame, Quality, Video},
};

type Session = Arc<Mutex<Video>>;

struct Clip {
    session: Option<Session>,
    marks: Arc<[i64]>,
    duration: i64,
    audio: bool,
    waveform: Option<Waveform>,
}

enum Update {
    Opened {
        clip: Option<Clip>,
        frame: Option<Frame>,
        error: Option<String>,
    },
    Frame(Result<Option<Frame>, ThumbError>),
}

pub struct Player {
    jobs: JobQueue<u64, Update>,
    key: u64,
    pending: Option<u64>,
    clip: Option<Clip>,
    source: Option<(PathBuf, Format)>,
    started: bool,
    last_request: Option<(i64, Quality)>,
    seek: bool,
    transport: Transport,
    volume: f64,
    muted: bool,
    volume_control: Option<Id>,
    sound: Sound,
    error: Option<String>,
    audio_error: Option<String>,
    pixels: Option<ColorImage>,
    texture: Option<TextureHandle>,
}

impl Player {
    pub fn new(host: &Host, wake: Wake) -> Self {
        Self {
            jobs: JobQueue::new(
                host.jobs_shared().clone(),
                wake.clone(),
                QueueOptions {
                    lanes: 1,
                    name: "Decode video playback",
                    ..Default::default()
                },
            ),
            sound: Sound::new(host, wake),
            key: 0,
            pending: None,
            clip: None,
            source: None,
            started: false,
            last_request: None,
            seek: false,
            transport: Transport::default(),
            volume: 1.0,
            muted: false,
            volume_control: None,
            error: None,
            audio_error: None,
            pixels: None,
            texture: None,
        }
    }

    fn next_key(&mut self) -> u64 {
        self.key = self.key.wrapping_add(1);
        self.pending = Some(self.key);
        self.key
    }

    pub fn open(&mut self, asset: &Asset) {
        self.close();
        let key = self.next_key();
        let path = asset.path.clone();
        let format = asset.format;
        self.source = Some((path.clone(), format));
        self.jobs.submit(key, Priority::VISIBLE, move |cancel| {
            let opened = (|| {
                if format.is_audio() {
                    let duration =
                        desktop_sound::duration(&path, format).map_err(ThumbError::Failed)?;
                    let waveform = local_file::open_read(&path)
                        .ok()
                        .and_then(|file| desktop_audio::inspect(file, format, 768, cancel).ok())
                        .and_then(|(_, waveform)| waveform);
                    return Ok((
                        Clip {
                            marks: Arc::from([]),
                            duration,
                            audio: true,
                            session: None,
                            waveform,
                        },
                        None,
                    ));
                }
                let mut video = Video::open(&path, format, cancel)?;
                let first = video
                    .frame(0, true, Quality::Still, cancel)?
                    .ok_or(ThumbError::Unsupported)?;
                Ok::<_, ThumbError>((
                    Clip {
                        marks: video.timeline(),
                        duration: video.duration(),
                        audio: video.has_audio(),
                        waveform: None,
                        session: Some(Arc::new(Mutex::new(video))),
                    },
                    Some(first),
                ))
            })();
            match opened {
                Ok((clip, frame)) => Update::Opened {
                    clip: Some(clip),
                    frame,
                    error: None,
                },
                Err(error) => {
                    let frame = (!format.is_audio() && !cancel.is_cancelled())
                        .then(|| decode_preview(&path, cancel).ok())
                        .flatten()
                        .map(|preview| Frame {
                            image: preview.image,
                            ns: 0,
                        });
                    Update::Opened {
                        clip: None,
                        frame,
                        error: Some(error_text(error)),
                    }
                }
            }
        });
    }

    pub fn receive(&mut self) -> Option<Word> {
        if let Some(error) = self.sound.receive_error() {
            self.audio_error = Some(error);
        }
        let mut announcement = None;
        while let Some(job) = self.jobs.try_recv() {
            if self.pending != Some(job.key) {
                continue;
            }
            self.pending = None;
            match job.result {
                Update::Opened { clip, frame, error } => {
                    self.started = true;
                    self.clip = clip;
                    self.error = error;
                    if let Some(frame) = frame {
                        self.set_pixels(frame.image);
                    }
                    announcement = Some(if self.clip.is_some() {
                        Word::plain("Player ready")
                    } else {
                        Word::plain("Playback unavailable")
                    });
                }
                Update::Frame(Ok(Some(frame))) => {
                    if !self.transport.playing() {
                        self.transport.position = frame.ns.max(0);
                    }
                    self.set_pixels(frame.image);
                }
                Update::Frame(Ok(None)) => {}
                Update::Frame(Err(error)) => {
                    self.error = Some(error_text(error));
                    self.transport.pause();
                    self.sound.stop();
                    announcement = Some(Word::plain("Could not decode video frame"));
                }
            }
        }
        announcement
    }

    fn set_pixels(&mut self, image: Image) {
        let (w, h) = image.dimensions();
        self.pixels = Some(ColorImage::from_rgba_unmultiplied(
            [w as usize, h as usize],
            &image.to_rgba8(),
        ));
    }

    fn stop_decode(&mut self) {
        self.jobs.reprioritize(|_, _| None);
        self.pending = None;
    }

    pub fn close(&mut self) {
        self.stop_decode();
        self.sound.stop();
        self.clip = None;
        self.source = None;
        self.started = false;
        self.last_request = None;
        self.seek = false;
        self.volume_control = None;
        self.transport = Transport::default();
        self.error = None;
        self.audio_error = None;
        self.pixels = None;
        self.texture = None;
    }

    fn sound_toggle(&mut self) {
        if self.transport.playing()
            && Sound::available()
            && self.clip.as_ref().is_some_and(|clip| clip.audio)
            && let Some((path, format)) = self.source.clone()
        {
            self.audio_error = None;
            self.sound.start(path, format, self.transport.position);
        } else {
            self.sound.stop();
        }
    }

    pub fn show(&mut self, ui: &mut Ui, name: &str) -> Option<Id> {
        let audio = self
            .source
            .as_ref()
            .is_some_and(|(_, format)| format.is_audio());
        if audio {
            return self.show_audio(ui, name);
        }
        if !self.started {
            Text::new(Word::plain("Opening media…")).show(ui);
            return None;
        }
        let (real, mut slider) = (!craft::ui::runtime::measure::active(ui.ctx()), None);
        if let Some(clip) = &self.clip {
            let duration = clip.duration;
            let marks = Arc::clone(&clip.marks);
            let was_playing = self.transport.playing();
            let wrapped = real && self.transport.advance(ui, duration);
            if wrapped {
                self.seek = true;
                self.stop_decode();
            }
            let (input, id) = self.transport.controls(ui, duration, &marks);
            slider = Some(id);
            if real && let Some(time) = input.seek {
                self.transport.pause();
                self.transport.seek(time);
                self.seek = true;
                self.error = None;
                self.stop_decode();
            } else if real && input.toggle {
                self.error = None;
                if self.transport.playing() {
                    self.transport.pause();
                } else {
                    self.seek |= self.transport.position >= duration;
                    self.transport.play(duration);
                    ui.ctx().request_repaint();
                }
            }
            if real && (was_playing != self.transport.playing() || wrapped) {
                self.sound_toggle();
            }
            let point = marks
                .partition_point(|&ns| ns <= self.transport.position)
                .saturating_sub(1);
            let quality = if self.transport.playing() {
                Quality::Playback
            } else {
                Quality::Still
            };
            if real
                && self.pending.is_none()
                && let Some(&timestamp) = marks.get(point)
                && (self.seek || self.last_request != Some((timestamp, quality)))
                && let Some(session) = self.clip.as_ref().and_then(|clip| clip.session.clone())
            {
                let seek = std::mem::take(&mut self.seek);
                self.last_request = Some((timestamp, quality));
                let key = self.next_key();
                self.jobs.submit(key, Priority::VISIBLE, move |cancel| {
                    Update::Frame(
                        session
                            .lock()
                            .map_err(|_| ThumbError::Failed("Video decoder is unavailable".into()))
                            .and_then(|mut video| video.frame(timestamp, seek, quality, cancel)),
                    )
                });
            }

            if self.clip.as_ref().is_some_and(|clip| clip.audio) {
                if Sound::available() {
                    let volume_label = format!("{:.0}%", self.volume * 100.0);
                    Slider::new(Bind::bound(&mut self.volume), 0.0..=1.0)
                        .label(Word::plain("Volume"))
                        .value_text(volume_label)
                        .short(true)
                        .show(ui);
                    if real {
                        self.sound.set_volume(self.volume);
                    }
                } else {
                    Text::new(Word::plain("Audio output is unavailable on this platform")).show(ui);
                }
            }
        }
        if let Some(error) = &self.error {
            Notice::new(Tone::Bad, Word::plain("Playback"))
                .lines([error.clone()])
                .show(ui);
        }
        if let Some(error) = &self.audio_error {
            Notice::new(Tone::Bad, Word::plain("Audio playback"))
                .lines([error.clone()])
                .show(ui);
        }
        if real && let Some(pixels) = self.pixels.take() {
            if let Some(texture) = &mut self.texture {
                texture.set(pixels, TextureOptions::LINEAR);
            } else {
                self.texture = Some(ui.ctx().load_texture(
                    "video-frame",
                    pixels,
                    TextureOptions::LINEAR,
                ));
            }
        }
        let room = ui.available_rect_before_wrap();
        if let Some(texture) = &self.texture {
            let response = ui.allocate_rect(room, Sense::hover());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Image, true, format!("Video frame: {name}"))
            });
            let size = texture.size_vec2();
            let scale = (room.width() / size.x).min(room.height() / size.y).max(0.0);
            let image = Rect::from_center_size(room.center(), size * scale);
            ui.painter().image(
                texture.id(),
                image,
                Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                Color32::WHITE,
            );
        }
        slider
    }

    pub fn control_focused(&self, ctx: &Context) -> bool {
        self.volume_control.is_some_and(|id| focus::has(ctx, id))
    }

    fn show_audio(&mut self, ui: &mut Ui, name: &str) -> Option<Id> {
        let real = !craft::ui::runtime::measure::active(ui.ctx());
        let duration = self.clip.as_ref().map_or(0, |clip| clip.duration);
        let available = Sound::available();
        let playing = self.transport.playing();
        let wrapped = real && self.transport.advance(ui, duration);
        let env = Env::of(ui);
        let colors = &env.tokens.color;
        let color = |value| Color32::from(Srgba::from(value));
        let room = ui.available_rect_before_wrap();
        if room.width() < 80.0 || room.height() < 120.0 {
            return None;
        }
        let pad = env.tokens.space.pad.regular;
        let width = (room.width() - 2.0 * pad).clamp(80.0, 760.0);
        let height = (room.height() - 2.0 * pad).clamp(120.0, 320.0);
        let panel = Rect::from_center_size(room.center(), egui::vec2(width, height));
        let radius = crate::UiTokens::corner(env.tokens.shape.radius.panel);
        ui.painter().rect(
            panel,
            radius,
            color(colors.surface.panel),
            egui::Stroke::new(1.0, color(colors.line.border)),
            egui::StrokeKind::Inside,
        );
        let content = panel.shrink2(egui::vec2(pad.min(width * 0.08), pad.min(height * 0.06)));
        let band = |top, bottom| {
            Rect::from_min_max(
                egui::pos2(content.left(), top),
                egui::pos2(content.right(), bottom),
            )
        };
        let header = band(content.top(), content.top() + 44.0);
        in_room(ui, header, |ui| {
            Text::new(name.to_owned())
                .role(TypeRole::Heading)
                .elide(Elide::Middle)
                .show(ui);
            if let Some(message) = self.error.as_ref().or(self.audio_error.as_ref()) {
                Text::new(message.clone())
                    .role(TypeRole::Caption)
                    .tone(Tone::Bad)
                    .elide(Elide::End)
                    .show(ui);
            } else if !available && self.started {
                Text::new(Word::plain("Audio output requires PipeWire"))
                    .role(TypeRole::Caption)
                    .shade(Shade::Muted)
                    .show(ui);
            }
        });

        let controls = band(content.bottom() - 34.0, content.bottom());
        let timeline = band(controls.top() - 34.0, controls.top());
        let time_width = (timeline.width() * 0.22).min(66.0);
        let track = timeline.shrink2(egui::vec2(time_width + 3.0, 0.0));
        let mut position = self.transport.position as f64 / 1e9;
        let scrub = in_room(ui, track, |ui| {
            Slider::new(
                Bind::bound(&mut position).policy(EditPolicy::OnCommit),
                0.0..=(duration.max(1) as f64 / 1e9),
            )
            .label(Word::plain("Playback position"))
            .value_text(audio_timestamp(self.transport.position, duration))
            .sizing(Sizing::FillWidth)
            .enabled(duration > 0)
            .show(ui)
        });
        let shown = scrub
            .shown
            .as_ref()
            .and_then(|value| value.same())
            .map_or(self.transport.position, |seconds| (**seconds * 1e9) as i64);
        let time_font = egui::FontId::monospace(if time_width < 52.0 { 10.0 } else { 12.0 });
        for (x, align, value) in [
            (timeline.left(), egui::Align2::LEFT_CENTER, shown),
            (timeline.right(), egui::Align2::RIGHT_CENTER, duration),
        ] {
            ui.painter().text(
                egui::pos2(x, timeline.center().y),
                align,
                audio_timestamp(value, duration),
                time_font.clone(),
                color(colors.text.muted),
            );
        }

        let waveform = band(header.bottom() + 4.0, timeline.top() - 4.0);
        let mut waveform_seek = None;
        if waveform.height() > 10.0 {
            ui.painter()
                .rect_filled(waveform, radius, color(colors.surface.sunken));
            let plot = waveform.shrink2(egui::vec2(8.0, 6.0));
            let hit = ui.interact(plot, ui.scope_id().with("audio-waveform"), Sense::click());
            let peaks = self.clip.as_ref().and_then(|clip| clip.waveform.as_deref());
            if let Some(peaks) = peaks {
                let progress = (shown as f64 / duration.max(1) as f64).clamp(0.0, 1.0) as f32;
                paint_waveform(
                    ui,
                    plot,
                    peaks,
                    progress,
                    color(colors.accent.strong),
                    color(colors.text.faint),
                );
                if hit.clicked()
                    && let Some(pointer) = hit.interact_pointer_pos()
                {
                    let part = ((pointer.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
                    waveform_seek = Some((f64::from(part) * duration as f64) as i64);
                }
            } else {
                let message = if self.started {
                    "Waveform unavailable"
                } else {
                    "Loading waveform…"
                };
                ui.painter().text(
                    plot.center(),
                    egui::Align2::CENTER_CENTER,
                    message,
                    egui::FontId::proportional(12.0),
                    color(colors.text.muted),
                );
            }
        }

        let mut toggle = false;
        let mut restart = false;
        let (icon, label) = if self.transport.playing() {
            ("pause", "Pause")
        } else {
            ("play", "Play")
        };
        let actions = Rect::from_min_size(
            controls.left_top(),
            egui::vec2((controls.width() * 0.5 - 24.0).max(28.0), controls.height()),
        );
        in_room(ui, actions, |ui| {
            ui.horizontal(|ui| {
                if actions.width() >= 59.0 {
                    restart = chrome_icon("arrow_left_fixed", "Restart")
                        .size(ControlSize::Sm)
                        .enabled(duration > 0)
                        .show(ui)
                        .clicked();
                }
                if chrome_icon("reset", "Loop")
                    .size(ControlSize::Sm)
                    .selected(self.transport.looping)
                    .enabled(duration > 0)
                    .show(ui)
                    .clicked()
                {
                    self.transport.looping = !self.transport.looping;
                }
            })
        });
        let play = Rect::from_center_size(controls.center(), egui::Vec2::splat(32.0));
        ui.painter()
            .circle_filled(play.center(), 16.0, color(colors.state.selected));
        in_room(ui, play, |ui| {
            toggle = chrome_icon(icon, label)
                .id(ui.scope_id().with("play-pause"))
                .variant(Variant::PRIMARY)
                .enabled(available && duration > 0)
                .show_at(ui, play.shrink(4.0))
                .clicked();
        });
        let control_width = (controls.width() * 0.5 - 24.0).clamp(28.0, 164.0);
        let volume = Rect::from_min_max(
            egui::pos2(controls.right() - control_width, controls.top()),
            controls.right_bottom(),
        );
        in_room(ui, volume, |ui| {
            ui.horizontal(|ui| {
                let label = if self.muted { "Unmute" } else { "Mute" };
                if chrome_icon("track_mute", label)
                    .id(ui.scope_id().with("mute"))
                    .size(ControlSize::Sm)
                    .selected(self.muted)
                    .enabled(available)
                    .show(ui)
                    .clicked()
                {
                    self.muted = !self.muted;
                }
                if control_width >= 66.0 {
                    let volume_text = format!("{:.0}%", self.volume * 100.0);
                    let output = Slider::new(Bind::bound(&mut self.volume), 0.0..=1.0)
                        .label(Word::plain("Volume"))
                        .value_text(volume_text)
                        .short(true)
                        .sizing(Sizing::FillWidth)
                        .size(ControlSize::Sm)
                        .enabled(available && !self.muted)
                        .show(ui);
                    self.volume_control = Some(output.response.id);
                } else {
                    self.volume_control = None;
                }
            })
        });
        if real {
            let seek = scrub
                .events
                .committed()
                .map(|time| (*time * 1e9) as i64)
                .or(waveform_seek)
                .or(restart.then_some(0));
            if let Some(next) = seek {
                self.transport.seek(next.clamp(0, duration));
            }
            if toggle {
                if self.transport.playing() {
                    self.transport.pause();
                } else {
                    self.transport.play(duration);
                }
            }
            self.sound
                .set_volume(if self.muted { 0.0 } else { self.volume });
            if playing != self.transport.playing() || wrapped || seek.is_some() {
                self.sound_toggle();
                ui.ctx().request_repaint();
            }
        }
        Some(scrub.response.id)
    }
}

fn audio_timestamp(ns: i64, duration: i64) -> String {
    let seconds = ns.max(0) / 1_000_000_000;
    if duration >= 3_600_000_000_000 {
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

fn paint_waveform(
    ui: &Ui,
    plot: Rect,
    peaks: &[[f32; 2]],
    progress: f32,
    played: Color32,
    pending: Color32,
) {
    let columns = (plot.width() / 3.0).round().max(1.0) as usize;
    let center = plot.center().y;
    let peak = peaks
        .iter()
        .flatten()
        .fold(0.0f32, |peak, value| peak.max(value.abs()));
    let amplitude = plot.height() * 0.42 / peak.max(0.15);
    for column in 0..columns {
        let start = column * peaks.len() / columns;
        let end = ((column + 1) * peaks.len() / columns).max(start + 1);
        let [low, high] = peaks[start..end]
            .iter()
            .fold([0.0f32; 2], |[low, high], &[a, b]| {
                [low.min(a), high.max(b)]
            });
        let t = (column as f32 + 0.5) / columns as f32;
        let x = egui::lerp(plot.x_range(), t);
        ui.painter().line_segment(
            [
                egui::pos2(x, center - (high * amplitude).max(0.75)),
                egui::pos2(x, center + (-low * amplitude).max(0.75)),
            ],
            egui::Stroke::new(1.5, if t <= progress { played } else { pending }),
        );
    }
    let x = egui::lerp(plot.x_range(), progress);
    ui.painter()
        .vline(x, plot.y_range(), egui::Stroke::new(1.0, played));
}

fn error_text(error: ThumbError) -> String {
    match error {
        ThumbError::Failed(error) => error,
        ThumbError::Missing => String::from("Original unavailable"),
        _ => String::from("Playback unavailable for this codec or file variant"),
    }
}
