use std::{path::PathBuf, sync::Arc, time::Duration};

use assetcraft::{Asset, Catalog, Format};
use craft::browse::thumbs::ThumbError;
use craft::egui::{
    self, Color32, ColorImage, Context, Id, Rect, Sense, TextureHandle, TextureOptions, Ui, Vec2,
};
use craft::host::queue::{JobQueue, Priority, QueueOptions};
use craft::host::{Host, Wake};
use craft::i18n::Word;
use craft::image_codecs::Image;
use craft::theme::variant::{Tone, Variant};
use craft::theme::{ControlSize, over};
use craft::ui::buttons::Button;
use craft::ui::choice::{Checkbox, Segmented};
use craft::ui::contract::{Bind, Entry, Props, Show, ShowContainer, Sizing, Trigger};
use craft::ui::display::Text;
use craft::ui::menu::{Menu, MenuItem};
use craft::ui::notices::Notice;
use craft::ui::runtime::env::Env;
use craft::ui::runtime::focus;
use craft::ui::value_drag::Slider;

use crate::desktop_scene_render::{Environment, Orbit, Projection, Quality, Scene, Studio};

enum Update {
    Model(Result<Arc<Scene>, ThumbError>, Vec<Asset>),
    Environment(Result<Arc<Environment>, ThumbError>, PathBuf),
    Frame(Result<Box<Image>, ThumbError>),
}

pub struct SceneViewer {
    jobs: JobQueue<u64, Update>,
    key: u64,
    scene: Option<Arc<Scene>>,
    model_job: Option<u64>,
    environments: Vec<Asset>,
    environment: Option<Arc<Environment>>,
    environment_path: Option<PathBuf>,
    environment_job: Option<u64>,
    frame_job: Option<u64>,
    texture: Option<TextureHandle>,
    pixels: Option<ColorImage>,
    view: Orbit,
    projection_control: Option<Id>,
    dirty: bool,
    edge: u32,
    quality: Quality,
    settle_until: f64,
    error: Option<String>,
    frame_error: Option<String>,
}

impl SceneViewer {
    pub fn new(host: &Host, wake: Wake) -> Self {
        Self {
            jobs: JobQueue::new(
                host.jobs_shared().clone(),
                wake,
                QueueOptions {
                    lanes: 1,
                    name: "Render 3D scene",
                    ..Default::default()
                },
            ),
            key: 0,
            scene: None,
            model_job: None,
            environments: Vec::new(),
            environment: None,
            environment_path: None,
            environment_job: None,
            frame_job: None,
            texture: None,
            pixels: None,
            view: Orbit::default(),
            projection_control: None,
            dirty: false,
            edge: 0,
            quality: Quality::Interactive,
            settle_until: 0.0,
            error: None,
            frame_error: None,
        }
    }

    fn job_key(&mut self) -> u64 {
        self.key = self.key.wrapping_add(1);
        self.key
    }

    pub fn open(&mut self, asset: &Asset, catalog: PathBuf) {
        self.close();
        let key = self.job_key();
        self.model_job = Some(key);
        let path = asset.path.clone();
        let format = asset.format;
        self.jobs.submit(key, Priority::VISIBLE, move |cancel| {
            let scene = Scene::open(&path, format, cancel).map(Arc::new);
            let sources = if scene.is_ok() && !cancel.is_cancelled() {
                futures_executor::block_on(async {
                    let mut catalog = Catalog::open(&catalog).await?;
                    let mut sources = catalog.find("").await?;
                    sources.retain(|asset| {
                        matches!(asset.format, Format::Hdr | Format::OpenExr)
                            && asset.path.is_file()
                    });
                    Ok::<_, assetcraft::Error>(sources)
                })
                .unwrap_or_default()
            } else {
                Vec::new()
            };
            Update::Model(scene, sources)
        });
    }

    pub fn close(&mut self) {
        self.jobs.reprioritize(|_, _| None);
        self.model_job = None;
        self.scene = None;
        self.environments.clear();
        self.environment = None;
        self.environment_path = None;
        self.environment_job = None;
        self.frame_job = None;
        self.texture = None;
        self.pixels = None;
        self.view = Orbit {
            projection: self.view.projection,
            ..Orbit::default()
        };
        self.projection_control = None;
        self.dirty = false;
        self.edge = 0;
        self.quality = Quality::Interactive;
        self.settle_until = 0.0;
        self.error = None;
        self.frame_error = None;
    }

    pub fn projection_focused(&self, ctx: &Context) -> bool {
        self.projection_control
            .is_some_and(|id| focus::has(ctx, id))
    }

    pub fn receive(&mut self) -> Option<Word> {
        let mut announcement = None;
        while let Some(job) = self.jobs.try_recv() {
            match job.result {
                Update::Model(scene, sources) if self.model_job == Some(job.key) => {
                    self.model_job = None;
                    self.environments = sources;
                    match scene {
                        Ok(scene) => {
                            self.scene = Some(scene);
                            self.dirty = true;
                        }
                        Err(error) => {
                            self.error = Some(error_text(error));
                            announcement = Some(Word::plain("Could not open 3D preview"));
                        }
                    }
                }
                Update::Environment(result, path) if self.environment_job == Some(job.key) => {
                    self.environment_job = None;
                    match result {
                        Ok(environment) => {
                            self.environment = Some(environment);
                            self.environment_path = Some(path);
                            self.error = None;
                            self.dirty = true;
                            announcement = Some(Word::plain("Environment loaded"));
                        }
                        Err(error) => {
                            self.error = Some(error_text(error));
                            announcement = Some(Word::plain("Could not open environment"));
                        }
                    }
                }
                Update::Frame(result) if self.frame_job == Some(job.key) => {
                    self.frame_job = None;
                    match result {
                        Ok(image) => {
                            self.frame_error = None;
                            let (w, h) = image.dimensions();
                            self.pixels = Some(ColorImage::from_rgba_unmultiplied(
                                [w as usize, h as usize],
                                &image.to_rgba8(),
                            ));
                            if self.texture.is_none() {
                                announcement = Some(Word::plain("3D preview ready"));
                            }
                        }
                        Err(error) => self.frame_error = Some(error_text(error)),
                    }
                }
                _ => {}
            }
        }
        announcement
    }

    fn pick_environment(&mut self, path: Option<PathBuf>) {
        if let Some(key) = self.environment_job.take() {
            self.jobs
                .reprioritize(|job, priority| (*job != key).then_some(priority));
        }
        self.error = None;
        if let Some(path) = path {
            let key = self.job_key();
            self.environment_job = Some(key);
            self.jobs.submit(key, Priority::VISIBLE, move |cancel| {
                Update::Environment(Environment::open(&path, cancel).map(Arc::new), path)
            });
        } else {
            self.environment = None;
            self.environment_path = None;
            self.pixels = None;
            if let Some(key) = self.frame_job.take() {
                self.jobs
                    .reprioritize(|job, priority| (*job != key).then_some(priority));
            }
            self.dirty = true;
        }
    }

    pub fn show(&mut self, ui: &mut Ui, name: &str) {
        if self.model_job.is_some() {
            Text::new(Word::plain("Loading 3D model…")).show(ui);
            return;
        }
        if self.scene.is_none() {
            self.notice(ui);
            return;
        }
        let measured = craft::ui::runtime::measure::active(ui.ctx());
        let original = self.view;
        self.toolbar(ui);
        self.notice(ui);
        if self.environment_job.is_some() {
            Text::new(Word::plain("Loading environment…")).show(ui);
        }
        let room = ui.available_rect_before_wrap();
        let size = room.width().min(room.height()).max(1.0);
        let rect = Rect::from_center_size(room.center(), Vec2::splat(size));
        let response = ui.allocate_rect(room, Sense::drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Image, true, format!("3D preview: {name}"))
        });
        if measured {
            return;
        }
        self.view.studio = studio_palette(ui);
        if let Some(pixels) = self.pixels.take() {
            if let Some(texture) = &mut self.texture {
                texture.set(pixels, TextureOptions::LINEAR);
            } else {
                self.texture = Some(ui.ctx().load_texture(
                    "asset-3d",
                    pixels,
                    TextureOptions::LINEAR,
                ));
            }
        }
        if let Some(texture) = &self.texture {
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let delta = response.drag_delta();
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
            || (response.dragged() && ui.input(|input| input.modifiers.shift))
        {
            for (offset, delta) in self.view.offset.iter_mut().zip([delta.x, delta.y]) {
                *offset = (*offset + f64::from(delta / size)).clamp(-2.0, 2.0);
            }
        } else if response.dragged_by(egui::PointerButton::Primary) {
            self.view.azimuth = wrap(self.view.azimuth - f64::from(delta.x) * 0.55);
            self.view.elevation =
                (self.view.elevation + f64::from(delta.y) * 0.55).clamp(-89.0, 89.0);
        }
        let (now, scroll) = ui.input(|input| (input.time, input.smooth_scroll_delta.y));
        if response.hovered() && scroll != 0.0 {
            self.view.zoom = (self.view.zoom * f64::from(scroll * 0.006).exp()).clamp(0.2, 8.0);
            self.settle_until = now + 0.12;
        }
        let settling = self.settle_until > now;
        if settling {
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f64(self.settle_until - now));
        }
        let quality = if self.texture.is_none() || response.dragged() || settling {
            Quality::Interactive
        } else {
            Quality::Settled
        };
        let physical_edge = size * ui.ctx().pixels_per_point();
        let edge = if quality == Quality::Interactive {
            (physical_edge * 0.55).ceil().clamp(192.0, 480.0) as u32
        } else {
            physical_edge.ceil().clamp(240.0, 2048.0) as u32
        };
        if (self.view.projection != original.projection
            || self.view.studio != original.studio
            || (quality == Quality::Interactive && self.quality == Quality::Settled))
            && let Some(key) = self.frame_job.take()
        {
            self.jobs
                .reprioritize(|job, priority| (*job != key).then_some(priority));
        }
        if edge != self.edge || quality != self.quality || self.view != original {
            self.dirty = true;
            self.edge = edge;
            self.quality = quality;
        }
        if self.dirty
            && self.frame_job.is_none()
            && let Some(scene) = self.scene.clone()
        {
            let key = self.job_key();
            self.frame_job = Some(key);
            self.dirty = false;
            let view = self.view;
            let environment = self.environment.clone();
            self.jobs.submit(key, Priority::VISIBLE, move |cancel| {
                Update::Frame(
                    scene
                        .render(view, environment.as_deref(), edge, quality, cancel)
                        .map(Box::new),
                )
            });
        }
    }

    fn notice(&self, ui: &mut Ui) {
        if let Some(error) = self.error.as_ref().or(self.frame_error.as_ref()) {
            Notice::new(Tone::Bad, Word::plain("3D preview"))
                .lines([error.clone()])
                .show(ui);
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            if button(ui, "Fit model").clicked() {
                let home = Orbit::default();
                self.view.azimuth = home.azimuth;
                self.view.elevation = home.elevation;
                self.view.zoom = home.zoom;
                self.view.offset = home.offset;
            }
            let options = [
                Entry::new(Projection::Orthographic, Word::plain("Orthographic")),
                Entry::new(Projection::Perspective, Word::plain("Perspective")),
            ];
            let projection = Segmented::new(
                Word::plain("Projection"),
                Bind::bound(&mut self.view.projection),
                &options,
            )
            .size(ControlSize::Sm)
            .sizing(Sizing::Content)
            .show(ui);
            self.projection_control = Some(projection.response.id);
            checkbox(ui, "Grid", &mut self.view.grid);
            checkbox(ui, "Wireframe", &mut self.view.wire);
            let camera = button(ui, "Camera");
            let _ = Menu::new(Word::plain("Camera"))
                .opened_by(&camera, Trigger::Click)
                .show(ui, |ui, _| {
                    slider(ui, "Camera azimuth", &mut self.view.azimuth, -180.0..=180.0);
                    slider(
                        ui,
                        "Camera elevation",
                        &mut self.view.elevation,
                        -89.0..=89.0,
                    );
                    slider(ui, "Zoom", &mut self.view.zoom, 0.2..=8.0);
                    slider(ui, "Horizontal pan", &mut self.view.offset[0], -2.0..=2.0);
                    slider(ui, "Vertical pan", &mut self.view.offset[1], -2.0..=2.0);
                });
            let light = button(ui, "Lighting");
            let _ = Menu::new(Word::plain("Lighting"))
                .opened_by(&light, Trigger::Click)
                .show(ui, |ui, _| {
                    slider(
                        ui,
                        "Light azimuth",
                        &mut self.view.light_azimuth,
                        -180.0..=180.0,
                    );
                    slider(
                        ui,
                        "Light elevation",
                        &mut self.view.light_elevation,
                        -90.0..=90.0,
                    );
                    slider(
                        ui,
                        "Light intensity",
                        &mut self.view.light_strength,
                        0.0..=4.0,
                    );
                    if self.environment.is_some() {
                        slider(
                            ui,
                            "Environment rotation",
                            &mut self.view.environment_rotation,
                            -180.0..=180.0,
                        );
                        slider(
                            ui,
                            "Environment exposure",
                            &mut self.view.exposure,
                            -6.0..=6.0,
                        );
                        checkbox(ui, "Show environment", &mut self.view.show_environment);
                    }
                });
            let opener = button(ui, "Environment");
            let mut choice = None;
            let _ = Menu::new(Word::plain("Environment"))
                .id_salt("scene-environment-menu")
                .opened_by(&opener, Trigger::Click)
                .show(ui, |ui, _| {
                    if MenuItem::new(Word::plain("Studio lighting"))
                        .check(self.environment.is_none())
                        .show(ui)
                        .ran
                    {
                        choice = Some(None);
                    }
                    if self.environments.is_empty() {
                        Text::new(Word::plain(
                            "Import an HDR or EXR panorama into this library to use it here.",
                        ))
                        .show(ui);
                    }
                    for entry in &self.environments {
                        if MenuItem::new(entry.display_name.clone())
                            .id_salt(entry.id)
                            .check(self.environment_path.as_ref() == Some(&entry.path))
                            .show(ui)
                            .ran
                        {
                            choice = Some(Some(entry.path.clone()));
                        }
                    }
                });
            if let Some(path) = choice {
                self.pick_environment(path);
            }
        });
    }
}

fn studio_palette(ui: &Ui) -> Studio {
    let env = Env::of(ui);
    let color = &env.tokens.color;
    let canvas = color.surface.canvas;
    let rgb = |value| {
        let [r, g, b, _] = over(value, canvas).0;
        [r, g, b]
    };
    let grid = if over(color.surface.canvas_pattern, canvas) == canvas {
        color.line.divider
    } else {
        color.surface.canvas_pattern
    };
    Studio {
        background: rgb(canvas),
        grid: rgb(grid),
        axis: rgb(color.text.muted),
    }
}

fn button(ui: &mut Ui, label: &'static str) -> craft::ui::buttons::ButtonOutput {
    Button::new(Word::plain(label))
        .variant(Variant::GHOST)
        .show(ui)
}

fn checkbox(ui: &mut Ui, label: &'static str, value: &mut bool) {
    Checkbox::new(Bind::bound(value))
        .label(Word::plain(label))
        .show(ui);
}

fn slider(ui: &mut Ui, label: &'static str, value: &mut f64, range: std::ops::RangeInclusive<f64>) {
    Slider::new(Bind::bound(value), range)
        .label(Word::plain(label))
        .show(ui);
}

fn wrap(angle: f64) -> f64 {
    (angle + 180.0).rem_euclid(360.0) - 180.0
}

fn error_text(error: ThumbError) -> String {
    match error {
        ThumbError::Failed(error) => error,
        ThumbError::Missing => String::from("Original unavailable"),
        _ => String::from("Preview unavailable for this format or variant"),
    }
}
