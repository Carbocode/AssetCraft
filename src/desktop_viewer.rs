use crate::{
    desktop_comments::{CommentChange, CommentEdit, PointComments},
    desktop_player::Player,
    desktop_scene_viewer::SceneViewer,
    desktop_thumbnails::decode_preview,
    desktop_transport::Transport,
};
use assetcraft::{Asset, Catalog, Error, Format, PointComment};
use craft::behavior::{
    CancelReason, CanvasView, Cursor, CursorIcon, IconId, PixelSize, ZoomScheme,
    canvas::zoom,
    kurbo::{Point, Size},
};
use craft::browse::thumbs::ThumbError;
use craft::egui::{Align, Color32, ColorImage, Id, Layout, Rect, Ui, UiBuilder};
use craft::host::queue::{JobQueue, Priority, QueueOptions};
use craft::host::{Host, Wake};
use craft::i18n::Word;
use craft::image_codecs::Image;
use craft::theme::variant::{Tone, Variant};
use craft::theme::{ControlSize, TypeRole};
use craft::ui::buttons::Button;
use craft::ui::canvas::{
    Canvas, CanvasTool, HandTool, NavigationOptions, PixelRect, RasterDisplay, RasterSource,
    ToolAction, ToolContext, ToolHost, ToolKey, ToolPath, ToolPointer, ViewCommands, ViewTarget,
    tool_pass,
};
use craft::ui::contract::{Bind, Props, Show, ShowContainer};
use craft::ui::display::{Elide, Shade, Text};
use craft::ui::layout_components::Row as ControlRow;
use craft::ui::notices::{Notice, NoticeAction};
use craft::ui::primitives::textures::Revision;
use craft::ui::primitives::{Draw, IconSpec};
use craft::ui::runtime::commands::Binding;
use craft::ui::runtime::env::{Env, Label};
use craft::ui::runtime::focus::{self, FocusRequest};
use craft::ui::runtime::keys::{self, Intent};
use craft::ui::runtime::overlay::{OpenFilter, any_open};
use craft::ui::value_drag::{Slider, SliderScale};
use futures_executor::block_on;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

#[path = "desktop_font_viewer.rs"]
mod desktop_font_viewer;
use desktop_font_viewer::FontViewer;

pub enum ViewerAction {
    Back,
    Previous,
    Next,
    ToggleCommentTool,
    EditComment(CommentEdit),
}

struct ViewerPan {
    hand: HandTool,
    dragging: bool,
}

impl CanvasTool for ViewerPan {
    fn each_pass(&mut self, cx: &mut ToolContext<'_>) {
        self.hand.each_pass(cx);
    }

    fn press(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) -> ToolAction {
        self.hand.press(pointer, cx)
    }

    fn drag(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) {
        self.dragging = true;
        self.hand.drag(pointer, cx);
    }

    fn release(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) {
        self.dragging = false;
        self.hand.release(pointer, cx);
    }

    fn cancel(&mut self, reason: CancelReason, cx: &mut ToolContext<'_>) {
        self.dragging = false;
        self.hand.cancel(reason, cx);
    }

    fn key(&mut self, key: &ToolKey, cx: &mut ToolContext<'_>) -> bool {
        self.hand.key(key, cx)
    }

    fn paint(&self, draw: &Draw<'_>, cx: &ToolContext<'_>) {
        self.hand.paint(draw, cx);
    }

    fn view_only(&self) -> bool {
        self.hand.view_only()
    }

    fn autoscrolls(&self) -> bool {
        self.hand.autoscrolls()
    }

    fn cursor(&self, _: &ToolContext<'_>) -> Cursor {
        Cursor::System(if self.dragging {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Grab
        })
    }

    fn command_form(&self) -> ToolPath {
        self.hand.command_form()
    }
}

struct DetailFrame {
    pixels: ColorImage,
    duration: Duration,
}

impl DetailFrame {
    fn new(image: Image, delay_ms: u32) -> Self {
        let (width, height) = image.dimensions();
        Self {
            pixels: ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                &image.to_rgba8(),
            ),
            duration: Duration::from_millis(u64::from(delay_ms.max(20))),
        }
    }
}

struct DetailImage {
    frames: Vec<DetailFrame>,
    index: usize,
    transport: Transport,
    timestamps: Vec<i64>,
    duration_ns: i64,
    generation: u64,
    animation_error: Option<String>,
}

impl DetailImage {
    fn current(&self) -> &DetailFrame {
        &self.frames[self.index]
    }

    fn control_animation(&mut self, ui: &mut Ui) -> Option<Id> {
        if self.frames.len() <= 1 {
            return None;
        }
        let real = !craft::ui::runtime::measure::active(ui.ctx());
        if real {
            self.transport.advance(ui, self.duration_ns);
        }
        let (input, slider) = self
            .transport
            .controls(ui, self.duration_ns, &self.timestamps);
        if real {
            if let Some(ns) = input.seek {
                self.transport.pause();
                self.transport.seek(ns);
            } else if input.toggle {
                if self.transport.playing() {
                    self.transport.pause();
                } else {
                    self.transport.play(self.duration_ns);
                }
                ui.ctx().request_repaint();
            }
            self.index = self
                .timestamps
                .partition_point(|&ns| ns <= self.transport.position)
                .saturating_sub(1);
        }
        Some(slider)
    }
}

impl RasterSource for DetailImage {
    fn size(&self) -> PixelSize {
        let [width, height] = self.current().pixels.size;
        PixelSize::new(width as u32, height as u32)
    }

    fn revision(&self) -> Revision {
        Revision::of(&(self.generation, self.index))
    }

    fn damage(&self, _since: Revision) -> Option<PixelRect> {
        None
    }

    fn read(&self, region: PixelRect, factor: u32, out: &mut ColorImage) {
        let pixels = &self.current().pixels;
        let [width, height] = pixels.size;
        let factor = i64::from(factor);
        let stride = out.size[0];
        if stride == 0 {
            return;
        }
        for (y, row) in out.pixels.chunks_exact_mut(stride).enumerate() {
            let y = i64::from(region.y0) + y as i64 * factor;
            if !(0..height as i64).contains(&y) || y >= i64::from(region.y1) {
                row.fill(Color32::TRANSPARENT);
                continue;
            }
            let source = &pixels.pixels[y as usize * width..][..width];
            for (x, pixel) in row.iter_mut().enumerate() {
                let x = i64::from(region.x0) + x as i64 * factor;
                *pixel = if (0..width as i64).contains(&x) && x < i64::from(region.x1) {
                    source[x as usize]
                } else {
                    Color32::TRANSPARENT
                };
            }
        }
    }
}

enum ImageState {
    Loading,
    Ready(DetailImage),
    Unavailable,
    Unsupported,
    Failed(String),
}

struct OpenAsset {
    id: Uuid,
    format: Format,
    catalog_path: PathBuf,
    state: ImageState,
    camera: CanvasView,
    hand: ViewerPan,
    slider_id: Option<Id>,
    transport_slider: Option<Id>,
    focus_back: bool,
    jumped: bool,
    comments: PointComments,
    comment_load_error: Option<String>,
}

pub struct Viewer {
    open: Option<OpenAsset>,
    generation: u64,
    scene: SceneViewer,
    player: Player,
    font: FontViewer,
    queue: JobQueue<u64, Result<DetailImage, ThumbError>>,
    comment_queue: JobQueue<u64, Result<Vec<PointComment>, Error>>,
}

impl Viewer {
    pub fn new(host: &Host, wake: Wake) -> Self {
        Self {
            open: None,
            generation: 0,
            scene: SceneViewer::new(host, wake.clone()),
            player: Player::new(host, wake.clone()),
            font: FontViewer::new(host, wake.clone()),
            queue: JobQueue::new(
                host.jobs_shared().clone(),
                wake.clone(),
                QueueOptions {
                    lanes: 1,
                    name: "Load asset viewer",
                    ..QueueOptions::default()
                },
            ),
            comment_queue: JobQueue::new(
                host.jobs_shared().clone(),
                wake,
                QueueOptions {
                    lanes: 1,
                    name: "Load image comments",
                    ..QueueOptions::default()
                },
            ),
        }
    }

    pub fn asset(&self) -> Option<Uuid> {
        self.open.as_ref().map(|open| open.id)
    }

    pub fn open(&mut self, asset: &Asset, catalog_path: PathBuf) {
        let focus_back = self.open.is_none();
        self.close();
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.open = Some(OpenAsset {
            id: asset.id,
            format: asset.format,
            catalog_path: catalog_path.clone(),
            state: ImageState::Loading,
            camera: CanvasView::new(Rect::NOTHING, Point::ZERO, 1.0),
            hand: ViewerPan {
                hand: HandTool::new(NavigationOptions::default()),
                dragging: false,
            },
            slider_id: None,
            transport_slider: None,
            focus_back,
            jumped: true,
            comments: PointComments::default(),
            comment_load_error: None,
        });
        if asset.format.is_model() {
            self.scene.open(asset, catalog_path);
            return;
        }
        if asset.format.is_video() || asset.format.is_audio() {
            self.player.open(asset);
            return;
        }
        if asset.format.is_font() {
            self.font.open(asset);
            return;
        }
        let path = asset.path.clone();
        self.queue
            .submit(generation, Priority::VISIBLE, move |cancel| {
                let preview = decode_preview(&path, cancel)?;
                let (animation, animation_error) = match preview.animation {
                    Ok(frames) => (frames, None),
                    Err(error) => (Vec::new(), Some(error.to_string())),
                };
                let frames = if animation.len() > 1 {
                    drop(preview.image);
                    let mut frames = Vec::with_capacity(animation.len());
                    for frame in animation {
                        if cancel.is_cancelled() {
                            return Err(ThumbError::Cancelled);
                        }
                        frames.push(DetailFrame::new(frame.image, frame.delay_ms));
                    }
                    frames
                } else {
                    vec![DetailFrame::new(preview.image, 0)]
                };
                if cancel.is_cancelled() {
                    return Err(ThumbError::Cancelled);
                }
                let mut duration_ns = 0;
                let timestamps = frames
                    .iter()
                    .map(|frame| {
                        let start = duration_ns;
                        duration_ns += frame.duration.as_nanos() as i64;
                        start
                    })
                    .collect();
                let mut transport = Transport::default();
                transport.looping = true;
                Ok(DetailImage {
                    frames,
                    index: 0,
                    transport,
                    timestamps,
                    duration_ns,
                    generation,
                    animation_error,
                })
            });
    }

    pub fn close(&mut self) {
        self.open = None;
        self.scene.close();
        self.player.close();
        self.font.close();
        self.queue.reprioritize(|_, _| None);
        self.comment_queue.reprioritize(|_, _| None);
    }

    fn load_comments(
        queue: &JobQueue<u64, Result<Vec<PointComment>, Error>>,
        generation: u64,
        open: &OpenAsset,
    ) {
        let path = open.catalog_path.clone();
        let id = open.id;
        queue.submit(generation, Priority::VISIBLE, move |_| {
            block_on(async {
                let mut catalog = Catalog::open(&path).await?;
                let comments = catalog.point_comments(id).await?;
                catalog.close().await?;
                Ok(comments)
            })
        });
    }

    pub fn toggle_comment_tool(&mut self) {
        if let Some(open) = &mut self.open {
            open.comments.toggle_tool();
        }
    }

    pub fn editing_comment(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.comments.editing())
    }

    pub fn comment_changed(
        &mut self,
        asset: Uuid,
        retry: Option<i64>,
        result: Result<CommentChange, Error>,
    ) {
        if let Some(open) = self.open.as_mut().filter(|open| open.id == asset) {
            open.comments.changed(retry, result);
        }
    }

    pub fn set_saving_comment(&mut self) {
        if let Some(open) = &mut self.open {
            open.comments.saving = true;
        }
    }

    pub fn saving_comment(&self) -> bool {
        self.open.as_ref().is_some_and(|open| open.comments.saving)
    }

    pub fn receive(&mut self) -> Option<Word> {
        let mut announcement = self.scene.receive();
        announcement = self.player.receive().or(announcement);
        announcement = self.font.receive().or(announcement);
        while let Some(finished) = self.queue.try_recv() {
            let Some(open) = self
                .open
                .as_mut()
                .filter(|_| finished.key == self.generation)
            else {
                continue;
            };
            (open.state, announcement) = match finished.result {
                Ok(image) => (ImageState::Ready(image), Some(Word::plain("Preview ready"))),
                Err(ThumbError::Missing) => (
                    ImageState::Unavailable,
                    Some(Word::plain("Original unavailable")),
                ),
                Err(ThumbError::Unsupported) => (
                    ImageState::Unsupported,
                    Some(Word::plain(
                        "Preview unavailable for this format or variant",
                    )),
                ),
                Err(ThumbError::Cancelled) => continue,
                Err(error) => (
                    ImageState::Failed(match error {
                        ThumbError::Failed(detail) => detail,
                        _ => format!("{error:?}"),
                    }),
                    Some(Word::plain("Could not open preview")),
                ),
            };
            if matches!(open.state, ImageState::Ready(_)) {
                Self::load_comments(&self.comment_queue, self.generation, open);
            }
        }
        while let Some(finished) = self.comment_queue.try_recv() {
            let Some(open) = self
                .open
                .as_mut()
                .filter(|_| finished.key == self.generation)
            else {
                continue;
            };
            announcement = Some(match finished.result {
                Ok(comments) => {
                    open.comments.loaded(comments);
                    open.comment_load_error = None;
                    Word::plain("Comments loaded")
                }
                Err(error) => {
                    open.comment_load_error = Some(error.to_string());
                    Word::plain("Could not load comments")
                }
            });
        }
        announcement
    }

    pub fn keyboard(&mut self, ui: &Ui) -> Option<ViewerAction> {
        let open = self.open.as_mut()?;
        let ctx = ui.ctx();
        if any_open(ctx, OpenFilter::Keys) {
            return None;
        }
        let rtl = Env::of(ui).direction.is_rtl();
        if keys::with_window_intents(ctx, rtl, |intents| intents.take(Intent::Cancel)) {
            if open.comments.saving {
                return None;
            }
            if open.comments.editing() {
                open.comments.cancel();
                return None;
            }
            if std::mem::take(&mut open.comments.placing) {
                return None;
            }
            return Some(ViewerAction::Back);
        }
        let slider_focused = [open.slider_id, open.transport_slider]
            .into_iter()
            .flatten()
            .any(|id| focus::has(ctx, id));
        if slider_focused
            || open.comments.editing()
            || open.comments.saving
            || (open.format.is_font() && self.font.control_focused(ctx))
            || (open.format.is_audio() && self.player.control_focused(ctx))
            || (open.format.is_model() && self.scene.projection_focused(ctx))
            || focus::focused_rec(ctx).is_some_and(|focused| focused.stop.text)
        {
            return None;
        }
        let owner = Id::unique("assetcraft-viewer-navigation");
        let mut action = None;
        for (key, direction) in [
            ("Left", ViewerAction::Previous),
            ("Right", ViewerAction::Next),
        ] {
            if let Ok(binding) = Binding::parse(key) {
                let chord = binding.trigger.chord;
                if keys::take_claimed(ctx, owner, chord) && action.is_none() {
                    action = Some(direction);
                }
                // Reserve these keys for the viewer on the next pass even when a toolbar button has focus.
                keys::capture(ctx, owner, chord);
            }
        }
        action
    }

    pub fn header(
        &mut self,
        ui: &mut Ui,
        name: &str,
        position: usize,
        count: usize,
        writable: bool,
    ) -> Option<ViewerAction> {
        let open = self.open.as_mut()?;
        let browsing = !open.comments.editing() && !open.comments.saving;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let tool_label = if open.comments.placing {
                "Stop adding comments"
            } else {
                "Add point comment"
            };
            let comment_tool = !open.format.is_model()
                && !open.format.is_video()
                && !open.format.is_audio()
                && !open.format.is_font()
                && crate::chrome_icon("note", tool_label)
                    .selected(open.comments.placing)
                    .enabled(browsing && writable && open.comments.available())
                    .show(ui)
                    .clicked();
            let next = crate::chrome_icon("chevron_right", "Next asset")
                .enabled(browsing && position + 1 < count)
                .show(ui)
                .clicked();
            Text::new(format!("{} / {count}", position + 1))
                .role(TypeRole::Caption)
                .show(ui);
            let previous = crate::chrome_icon("chevron_left", "Previous asset")
                .enabled(browsing && position > 0)
                .show(ui)
                .clicked();
            let back = ui
                .with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let mut back = Button::new(Word::plain("Library"))
                        .tooltip(Word::plain("Back to library"))
                        .icon(IconSpec::svg(IconId::new("arrow_left_fixed")))
                        .enabled(browsing)
                        .variant(Variant::GHOST);
                    if !craft::ui::runtime::measure::active(ui.ctx())
                        && std::mem::take(&mut open.focus_back)
                    {
                        back = back.request_focus(FocusRequest::Take);
                    }
                    let back = back.show(ui).clicked();
                    if !open.format.is_audio() {
                        Text::new(name.to_owned())
                            .role(TypeRole::BodyStrong)
                            .elide(Elide::Middle)
                            .show(ui);
                    }
                    back
                })
                .inner;
            if back {
                Some(ViewerAction::Back)
            } else if previous {
                Some(ViewerAction::Previous)
            } else if next {
                Some(ViewerAction::Next)
            } else if comment_tool {
                Some(ViewerAction::ToggleCommentTool)
            } else {
                None
            }
        })
        .inner
    }

    pub fn body(&mut self, ui: &mut Ui, name: &str, writable: bool) -> Option<ViewerAction> {
        let open = self.open.as_mut()?;
        if open.format.is_model() {
            self.scene.show(ui, name);
            return None;
        }
        if open.format.is_video() || open.format.is_audio() {
            open.transport_slider = self.player.show(ui, name);
            return None;
        }
        if open.format.is_font() {
            self.font.show(ui);
            return None;
        }
        if let Some(error) = &open.comment_load_error {
            let notice = Notice::new(Tone::Bad, Word::plain("Could not load comments"))
                .lines([error.clone()])
                .action(NoticeAction::new(Word::plain("Retry loading comments")))
                .show(ui);
            if notice.action.is_some() {
                open.comment_load_error = None;
                Self::load_comments(&self.comment_queue, self.generation, open);
            }
        } else if matches!(open.state, ImageState::Ready(_)) && !open.comments.available() {
            Text::new(Word::plain("Loading comments…")).show(ui);
        }
        if let ImageState::Ready(image) = &mut open.state {
            open.transport_slider = image.control_animation(ui);
            if let Some(error) = &image.animation_error {
                Notice::new(Tone::Bad, Word::plain("Could not play animation"))
                    .lines([error.clone()])
                    .show(ui);
            }
        }
        Self::image(open, ui, name, writable)
    }

    fn image(
        open: &mut OpenAsset,
        ui: &mut Ui,
        name: &str,
        writable: bool,
    ) -> Option<ViewerAction> {
        let image = match &open.state {
            ImageState::Ready(image) => image,
            state => {
                let (label, detail) = match state {
                    ImageState::Loading => (Word::plain("Loading preview…"), None),
                    ImageState::Unavailable => (Word::plain("Original unavailable"), None),
                    ImageState::Unsupported => (
                        Word::plain("Preview unavailable for this format or variant"),
                        None,
                    ),
                    ImageState::Failed(error) => {
                        (Word::plain("Could not open preview"), Some(error.as_str()))
                    }
                    ImageState::Ready(_) => return None,
                };
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        Text::new(label).show(ui);
                        if let Some(detail) = detail {
                            Text::new(detail.to_owned())
                                .role(TypeRole::Caption)
                                .shade(Shade::Muted)
                                .show(ui);
                        }
                    });
                });
                return None;
            }
        };
        let size = image.size();
        let document = Size::new(f64::from(size.width), f64::from(size.height));
        let mut canvas = Canvas::new(Word::plain("Asset image"))
            .view(open.camera)
            .document(document);
        let real = !craft::ui::runtime::measure::active(ui.ctx());
        if real && std::mem::take(&mut open.jumped) {
            canvas = canvas.jumped();
        }
        let room = ui.available_rect_before_wrap();
        let mut tool_view = None;
        let shown = canvas.show_at(ui, room, |ui, body| {
            RasterDisplay::new(Label::from(name.to_owned()), image)
                .picture()
                .show_at(ui, body.rect);
            if let Some(pass) = tool_pass(&body.env) {
                tool_view = ToolHost::new().run(ui, &pass, &mut open.hand).view;
            }
        });
        let mut action = None;
        if real && let Some(space) = &shown.output.space {
            let (handled, save) = ui
                .scope_builder(
                    UiBuilder::new()
                        .id_salt("viewer-comment-pins")
                        .max_rect(room),
                    |ui| open.comments.pins(ui, &space.view, document, writable),
                )
                .inner;
            action = save.map(ViewerAction::EditComment);
            let mut at = shown.output.click_at();
            if open.comments.placing
                && !open.comments.editing()
                && let Ok(binding) = Binding::parse("Enter")
            {
                let ctx = ui.ctx();
                let canvas_id = shown.output.response.id;
                let chord = binding.trigger.chord;
                keys::claim(ctx, canvas_id, chord);
                if keys::take_claimed(ctx, canvas_id, chord) {
                    let center = space.view.to_doc(space.view.rect.center());
                    at = Some(space.view.to_screen(Point::new(
                        center.x.clamp(0.0, document.width),
                        center.y.clamp(0.0, document.height),
                    )));
                }
            }
            if !handled && let Some(at) = at {
                open.comments
                    .click_canvas(&space.view, document, at, writable);
                ui.ctx().request_repaint();
            }
        }
        if real && let Some(camera) = tool_view.or(shown.output.view) {
            if !open.camera.rect.is_positive() && camera.rect.is_positive() {
                ui.ctx().request_repaint();
            }
            open.camera = camera;
        }
        action
    }

    pub fn status(&mut self, ui: &mut Ui, format: Format) {
        let Some(open) = &mut self.open else { return };
        let size = match &open.state {
            ImageState::Ready(image) => Some(image.size()),
            _ => None,
        };
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(size) = size.filter(|_| open.camera.rect.is_positive()) {
                ControlRow::new().show(ui, |ui, _| open.zoom_controls(ui, size));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let label = format.name();
                let metadata = size.filter(|_| format.codec().is_some()).map_or_else(
                    || label.to_owned(),
                    |size| format!("{label} · {} × {} px", size.width, size.height),
                );
                Text::new(metadata)
                    .role(TypeRole::Caption)
                    .shade(Shade::Muted)
                    .elide(Elide::End)
                    .show(ui);
            });
        });
    }
}

impl OpenAsset {
    fn zoom_controls(&mut self, ui: &mut Ui, size: PixelSize) {
        let target = ViewTarget::new(Size::new(f64::from(size.width), f64::from(size.height)));
        let env = Env::of(ui);
        let units = env.units();
        let scheme = env.view().get::<ZoomScheme>();
        let scale = env.scale.0;
        let before = self.camera;
        let mut command = |action: ViewCommands| {
            let _ = action.apply(&mut self.camera, &target, None, scale, &scheme);
        };
        if Button::new(Word::plain("Fit"))
            .tooltip(Word::plain("Fit image to view"))
            .variant(Variant::GHOST)
            .show(ui)
            .clicked()
        {
            command(ViewCommands::FitOnScreen);
        }
        if Button::new(Word::plain("100%"))
            .tooltip(Word::plain("Show actual pixels"))
            .variant(Variant::GHOST)
            .show(ui)
            .clicked()
        {
            command(ViewCommands::ActualPixels);
        }
        if crate::chrome_icon("minus", "Zoom out").show(ui).clicked() {
            command(ViewCommands::ZoomOut);
        }
        let device = zoom::device(&self.camera, scale);
        let mut changed = device;
        let range = scheme.min(target.document).min(device)..=scheme.max_device().max(device);
        let response = Slider::new(Bind::bound(&mut changed), range)
            .scale(SliderScale::Log)
            .short(true)
            .label(Word::plain("Image zoom"))
            .value_text(units.zoom_percent(scheme.percent(device)))
            .size(ControlSize::Md)
            .show(ui);
        self.slider_id = Some(response.response.id);
        if changed != device {
            self.camera = zoom::with_device(&self.camera, changed, target.document, scale, &scheme);
        }
        if crate::chrome_icon("plus", "Zoom in").show(ui).clicked() {
            let _ = ViewCommands::ZoomIn.apply(&mut self.camera, &target, None, scale, &scheme);
        }
        Text::new(units.zoom_percent(scheme.percent(zoom::device(&self.camera, scale))))
            .role(TypeRole::Caption)
            .show(ui);
        if self.camera != before {
            self.jumped = true;
            ui.ctx().request_repaint();
        }
    }
}
