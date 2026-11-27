use std::time::{Duration, Instant};

use craft::egui::{Id, Ui};
use craft::i18n::Word;
use craft::theme::variant::Variant;
use craft::ui::buttons::Button;
use craft::ui::choice::Checkbox;
use craft::ui::contract::{Bind, Props, Show};
use craft::ui::display::Text;
use craft::ui::value_drag::Slider;

#[derive(Default)]
pub struct Transport {
    pub position: i64,
    pub looping: bool,
    anchor: Option<(Instant, i64)>,
}

#[derive(Default)]
pub struct Input {
    pub toggle: bool,
    pub seek: Option<i64>,
}

impl Transport {
    pub fn playing(&self) -> bool {
        self.anchor.is_some()
    }

    pub fn pause(&mut self) {
        self.anchor = None;
    }

    pub fn play(&mut self, duration: i64) {
        if self.position >= duration {
            self.position = 0;
        }
        self.anchor = Some((Instant::now(), self.position));
    }

    pub fn seek(&mut self, position: i64) {
        self.position = position.max(0);
        if self.playing() {
            self.anchor = Some((Instant::now(), self.position));
        }
    }

    /// Advance the playhead, reporting when the decoder must jump back to the start.
    pub fn advance(&mut self, ui: &Ui, duration: i64) -> bool {
        let Some((started, initial)) = self.anchor else {
            return false;
        };
        let elapsed = i64::try_from(started.elapsed().as_nanos()).unwrap_or(i64::MAX);
        let time = initial.saturating_add(elapsed);
        if time >= duration && !self.looping {
            self.position = duration;
            self.pause();
        } else {
            self.position = time.rem_euclid(duration.max(1));
            ui.ctx().request_repaint_after(Duration::from_millis(20));
            if time >= duration {
                self.anchor = Some((Instant::now(), self.position));
                return true;
            }
        }
        false
    }

    pub fn controls(&mut self, ui: &mut Ui, duration: i64, timeline: &[i64]) -> (Input, Id) {
        let mut input = Input::default();
        ui.horizontal_wrapped(|ui| {
            let play = if self.playing() { "Pause" } else { "Play" };
            input.toggle = button(ui, play);
            if !timeline.is_empty() {
                let at = timeline
                    .partition_point(|&ns| ns <= self.position)
                    .saturating_sub(1);
                if button(ui, "Previous frame") {
                    input.seek = Some(timeline[at.saturating_sub(1)]);
                }
                if button(ui, "Next frame") {
                    input.seek = timeline
                        .get(at + 1)
                        .copied()
                        .or_else(|| timeline.last().copied());
                }
            }
            Checkbox::new(Bind::bound(&mut self.looping))
                .label(Word::plain("Loop"))
                .show(ui);
            Text::new(format!(
                "{} / {}",
                timestamp(self.position),
                timestamp(duration)
            ))
            .show(ui);
        });
        let before = self.position as f64 / 1e9;
        let mut time = before;
        let response = Slider::new(Bind::bound(&mut time), 0.0..=(duration.max(1) as f64 / 1e9))
            .label(Word::plain("Playback position"))
            .value_text(timestamp(self.position))
            .show(ui);
        if time != before {
            input.seek = Some((time * 1e9) as i64);
        }
        (input, response.response.id)
    }
}

fn button(ui: &mut Ui, label: &'static str) -> bool {
    let response = Button::new(Word::plain(label))
        .variant(Variant::GHOST)
        .show(ui);
    if response.clicked() {
        response.response.request_focus();
        true
    } else {
        false
    }
}

fn timestamp(ns: i64) -> String {
    let ms = ns.max(0) / 1_000_000;
    let seconds = ms / 1000;
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}.{:03}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
            ms % 1000
        )
    } else {
        format!("{:02}:{:02}.{:03}", seconds / 60, seconds % 60, ms % 1000)
    }
}
