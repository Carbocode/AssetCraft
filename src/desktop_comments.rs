use assetcraft::{Error, PointComment};
use craft::behavior::{
    CanvasView, Cursor, CursorIcon, CursorPriority, EditPolicy, Metrics, PressPhase, Sense,
    kurbo::{Point, Size},
};
use craft::egui::{CornerRadius, Pos2, Rect, Response, Ui, Vec2, vec2};
use craft::i18n::Word;
use craft::theme::variant::Variant;
use craft::theme::{ControlSize, TypeRole};
use craft::ui::buttons::Button;
use craft::ui::contract::overlay::{Dismiss, Outside};
use craft::ui::contract::{
    Bind, Common, Component, Cx, MeasureCx, NoValue, Outcome, PaintUi, Pointer, Props, Role, Show,
    ShowContainer, Sizing,
};
use craft::ui::display::Text;
use craft::ui::popover::Popover;
use craft::ui::primitives::{HAlign, PaintShape, StrokeWidth, TextRun};
use craft::ui::runtime::env::Env;
use craft::ui::runtime::focus::FocusRequest;
use craft::ui::style::{ComponentStyle, StyleColor, StyleCx};
use craft::ui::text::TextField;

pub enum CommentEdit {
    Add([f64; 2], String),
    Text(i64, String),
    Move(i64, [f64; 2]),
    Delete(i64),
}

pub enum CommentChange {
    Upsert(PointComment),
    Deleted(i64),
}

enum DraftKind {
    New([f64; 2]),
    Edit(i64),
}

struct Draft {
    kind: DraftKind,
    body: String,
    focus: bool,
}

#[derive(Default)]
pub struct PointComments {
    pub placing: bool,
    pub saving: bool,
    comments: Option<Vec<PointComment>>,
    selected: Option<i64>,
    confirming_delete: bool,
    draft: Option<Draft>,
    pending_move: Option<(i64, [f64; 2])>,
    error: Option<String>,
}

impl PointComments {
    pub fn toggle_tool(&mut self) {
        self.placing = !self.placing;
        self.select(None);
    }

    fn select(&mut self, id: Option<i64>) {
        self.selected = id;
        self.confirming_delete = false;
        self.error = None;
    }

    pub fn available(&self) -> bool {
        self.comments.is_some()
    }

    pub fn loaded(&mut self, result: Result<Vec<PointComment>, Error>) {
        self.comments = result.ok();
    }

    pub fn changed(&mut self, retry: Option<i64>, result: Result<CommentChange, Error>) {
        self.saving = false;
        self.pending_move = None;
        match result {
            Ok(CommentChange::Upsert(comment)) => {
                let comments = self.comments.get_or_insert_with(Vec::new);
                match comments.binary_search_by_key(&comment.id, |entry| entry.id) {
                    Ok(index) => comments[index] = comment,
                    Err(index) => comments.insert(index, comment),
                }
                self.draft = None;
                self.error = None;
            }
            Ok(CommentChange::Deleted(id)) => {
                if let Some(comments) = &mut self.comments {
                    comments.retain(|comment| comment.id != id);
                }
                self.select(None);
            }
            Err(error) => {
                if let Some(id) = retry {
                    self.select(Some(id));
                }
                self.error = Some(error.to_string());
            }
        }
    }

    pub fn editing(&self) -> bool {
        self.draft.is_some()
    }

    pub fn cancel(&mut self) {
        if !self.saving {
            self.draft = None;
            self.error = None;
        }
    }

    pub fn click_canvas(&mut self, view: &CanvasView, size: Size, at: Pos2, writable: bool) {
        if !self.placing || !self.available() || self.saving || !writable || !view.rect.contains(at)
        {
            return;
        }
        let point = relative_point(view, size, at);
        if !point.iter().all(|value| (0.0..=1.0).contains(value)) {
            return;
        }
        let draft = self.draft.get_or_insert_with(|| Draft {
            kind: DraftKind::New(point),
            body: String::new(),
            focus: true,
        });
        if let DraftKind::New(previous) = &mut draft.kind {
            *previous = point;
            draft.focus = true;
            self.selected = None;
        }
    }

    pub fn pins(
        &mut self,
        ui: &mut Ui,
        view: &CanvasView,
        size: Size,
        writable: bool,
    ) -> (bool, Option<CommentEdit>) {
        let Some(comments) = &self.comments else {
            return (false, None);
        };
        let project = |point: [f64; 2]| {
            view.to_screen(Point::new(point[0] * size.width, point[1] * size.height))
        };
        let enabled = !self.editing() && !self.saving;
        let mut selected = self.selected;
        let mut handled = false;
        let mut opener = None;
        let mut movement = None;
        for (index, comment) in comments.iter().enumerate() {
            let number = index + 1;
            let point = self
                .pending_move
                .filter(|(id, _)| *id == comment.id)
                .map_or(comment.point, |(_, point)| point);
            let anchor = project(point);
            let active = selected == Some(comment.id);
            if !(view.rect.contains(anchor) || active && self.editing()) {
                if active {
                    selected = None;
                }
                continue;
            }
            let anchor = view.rect.clamp(anchor);
            let pin = bubble(
                ui,
                view.rect,
                anchor,
                Some(comment.id),
                number,
                active,
                if !enabled {
                    BubbleInteraction::Disabled
                } else if writable {
                    BubbleInteraction::Move
                } else {
                    BubbleInteraction::Read
                },
            );
            match pin.event {
                BubbleEvent::Click => {
                    selected = (!active).then_some(comment.id);
                    handled = true;
                }
                BubbleEvent::Drag(_) => {
                    selected = None;
                    handled = true;
                }
                BubbleEvent::Drop(delta) => {
                    let point = relative_point(view, size, view.rect.clamp(anchor + delta))
                        .map(|value| value.clamp(0.0, 1.0));
                    selected = None;
                    movement = Some((comment.id, point));
                }
                BubbleEvent::None => {}
            }
            if selected == Some(comment.id) {
                opener = Some((number, comment, pin.response));
            }
        }

        if let Some((id, point)) = movement {
            self.pending_move = Some((id, point));
            self.select(None);
            return (true, Some(CommentEdit::Move(id, point)));
        }

        if let Some(draft) = &self.draft {
            let (number, anchor) = match draft.kind {
                DraftKind::New(point) => {
                    let anchor = view.rect.clamp(project(point));
                    let number = comments.len() + 1;
                    bubble(
                        ui,
                        view.rect,
                        anchor,
                        None,
                        number,
                        true,
                        BubbleInteraction::Disabled,
                    );
                    (number, anchor)
                }
                DraftKind::Edit(id) => {
                    let Some((number, comment, _)) =
                        opener.filter(|(_, comment, _)| comment.id == id)
                    else {
                        return (handled, None);
                    };
                    (number, view.rect.clamp(project(comment.point)))
                }
            };
            return self.compose(ui, view.rect, number, anchor, writable);
        }

        let opener = opener
            .filter(|(_, comment, _)| Some(comment.id) == selected)
            .map(|(number, comment, response)| {
                (number, comment.id, comment.body.clone(), response)
            });
        if selected != self.selected {
            self.select(selected);
        }
        if let Some((number, id, body, response)) = opener {
            let label = format!("Comment {number}");
            let mut open = true;
            let popup = Popover::new(label.clone(), Bind::bound(&mut open))
                .id_salt(("comment-popup", id))
                .opened_by(&response)
                .dismiss(Dismiss::new().outside(if self.saving {
                    Outside::Keep
                } else {
                    Outside::Close
                }))
                .escape(!self.saving)
                .overlay(|overlay| overlay.bounds(view.rect))
                .show(ui, |ui, _| {
                    if heading(ui, view.rect, &label, "Close comment", !self.saving) {
                        return PopupAction::Close;
                    }
                    if self.confirming_delete {
                        Text::new(Word::plain("Delete this comment?")).show(ui);
                    } else {
                        Text::new(body.clone()).sizing(Sizing::FillWidth).show(ui);
                    }
                    self.show_error(ui);
                    ui.horizontal_wrapped(|ui| {
                        if self.confirming_delete {
                            if button(ui, "Cancel", Variant::GHOST, !self.saving) {
                                PopupAction::Read
                            } else if button(ui, "Delete comment", Variant::DANGER, writable) {
                                PopupAction::Delete
                            } else {
                                PopupAction::None
                            }
                        } else if button(ui, "Edit comment", Variant::GHOST, writable) {
                            PopupAction::Edit
                        } else if button(ui, "Delete comment", Variant::GHOST, writable) {
                            PopupAction::ConfirmDelete
                        } else {
                            PopupAction::None
                        }
                    })
                    .inner
                });
            match popup.inner.unwrap_or(PopupAction::None) {
                _ if !open => self.select(None),
                PopupAction::Close => self.select(None),
                PopupAction::Edit => {
                    self.draft = Some(Draft {
                        kind: DraftKind::Edit(id),
                        body,
                        focus: true,
                    });
                    self.error = None;
                }
                PopupAction::ConfirmDelete => self.confirming_delete = true,
                PopupAction::Read => self.confirming_delete = false,
                PopupAction::Delete => return (true, Some(CommentEdit::Delete(id))),
                PopupAction::None => return (handled, None),
            }
            return (true, None);
        }
        (handled, None)
    }

    fn show_error(&self, ui: &mut Ui) {
        if let Some(error) = &self.error {
            Text::new(error.clone())
                .tone(craft::theme::variant::Tone::Bad)
                .show(ui);
        }
    }

    fn compose(
        &mut self,
        ui: &mut Ui,
        room: Rect,
        number: usize,
        anchor: Pos2,
        writable: bool,
    ) -> (bool, Option<CommentEdit>) {
        let Some(draft) = &mut self.draft else {
            return (false, None);
        };
        let label = match draft.kind {
            DraftKind::New(_) => format!("New comment {number}"),
            DraftKind::Edit(_) => format!("Edit comment {number}"),
        };
        let mut open = true;
        let saving = self.saving;
        let popup = Popover::new(label.clone(), Bind::bound(&mut open))
            .id_salt("comment-composer")
            .at_point(anchor)
            .dismiss(Dismiss::new().outside(Outside::Keep))
            .escape(!saving)
            .overlay(|overlay| overlay.bounds(room))
            .show(ui, |ui, _| {
                let close = heading(ui, room, &label, "Cancel comment", !saving);
                let mut field = TextField::new(
                    Word::plain("Comment"),
                    Bind::bound(&mut draft.body).policy(EditPolicy::WriteThrough),
                )
                .placeholder(Word::plain("Write a comment…"))
                .lines(3)
                .max_lines(4)
                .sizing(Sizing::FillWidth)
                .enabled(!saving);
                if !craft::ui::runtime::measure::active(ui.ctx())
                    && std::mem::take(&mut draft.focus)
                {
                    field = field.request_focus(FocusRequest::Take);
                }
                field.show(ui);
                if let Some(error) = &self.error {
                    Text::new(error.clone())
                        .tone(craft::theme::variant::Tone::Bad)
                        .show(ui);
                }
                let label = if saving { "Saving…" } else { "Save comment" };
                let save = button(
                    ui,
                    label,
                    Variant::PRIMARY,
                    writable && !saving && !draft.body.trim().is_empty(),
                );
                (close, save)
            });
        if popup.inner.is_some_and(|(_, save)| save) {
            let body = draft.body.clone();
            let edit = match draft.kind {
                DraftKind::New(point) => CommentEdit::Add(point, body),
                DraftKind::Edit(id) => CommentEdit::Text(id, body),
            };
            return (true, Some(edit));
        }
        if !open || popup.inner.is_some_and(|(close, _)| close) {
            self.cancel();
            return (true, None);
        }
        (false, None)
    }
}

enum PopupAction {
    None,
    Close,
    Read,
    Edit,
    ConfirmDelete,
    Delete,
}

fn relative_point(view: &CanvasView, size: Size, at: Pos2) -> [f64; 2] {
    let point = view.to_doc(at);
    [point.x / size.width, point.y / size.height]
}

fn button(ui: &mut Ui, label: &'static str, variant: Variant, enabled: bool) -> bool {
    Button::new(Word::plain(label))
        .size(ControlSize::Sm)
        .variant(variant)
        .enabled(enabled)
        .show(ui)
        .clicked()
}

#[derive(Clone, Debug, PartialEq, ComponentStyle)]
struct BubbleStyle {
    #[style(metrics)]
    metrics: Metrics,
    fill: StyleColor,
    active_fill: StyleColor,
    edge: StyleColor,
    active_edge: StyleColor,
}

#[derive(Component)]
struct CommentBubble {
    #[common]
    common: Common,
    number: String,
    size: Vec2,
    radius: CornerRadius,
    active: bool,
    movable: bool,
    event: BubbleEvent,
}

#[derive(Clone, Copy)]
enum BubbleEvent {
    None,
    Click,
    Drag(Vec2),
    Drop(Vec2),
}

struct BubbleOutput {
    response: Response,
    event: BubbleEvent,
}

impl Outcome for BubbleOutput {
    type Value = NoValue;

    fn response(&self) -> &Response {
        &self.response
    }
}

impl Component for CommentBubble {
    const ROLE: Role = Role::BUTTON;
    type Style = BubbleStyle;
    type Output = BubbleOutput;

    fn style(cx: &StyleCx<'_>, _: CommentBubbleStyleKey) -> BubbleStyle {
        let colors = &cx.tokens.color;
        BubbleStyle {
            metrics: Metrics::new(),
            fill: cx.color(colors.surface.raised),
            active_fill: cx.color(colors.accent.soft),
            edge: cx.color(colors.line.border),
            active_edge: cx.color(colors.accent.strong),
        }
    }

    fn sense(&self) -> Sense {
        let sense = Sense::CLICK | Sense::FOCUS;
        if self.movable {
            sense | Sense::DRAG
        } else {
            sense
        }
    }

    fn measure(&mut self, _: &MeasureCx<'_, Self>) -> Vec2 {
        self.size
    }

    fn update(&mut self, cx: &mut Cx<'_, Self>) {
        self.event = match cx.press().phase {
            PressPhase::DragStart { start, pos, .. } | PressPhase::Drag { start, pos, .. }
                if self.movable =>
            {
                cx.cursor(Cursor::System(CursorIcon::Grabbing), CursorPriority::Drag);
                BubbleEvent::Drag(pos - start.origin)
            }
            PressPhase::Release {
                start,
                pos,
                click: false,
                ..
            } if self.movable => BubbleEvent::Drop(pos - start.origin),
            _ if cx.activation().is_some() => BubbleEvent::Click,
            _ => BubbleEvent::None,
        };
        if self.movable && cx.response.hovered() {
            cx.cursor(Cursor::System(CursorIcon::Grab), CursorPriority::Hover);
        }
    }

    fn paint(&mut self, ui: &mut PaintUi<'_>, cx: &Cx<'_, Self>) {
        let draw = ui.draw();
        let mut rect = cx.rect;
        if let BubbleEvent::Drag(delta) = self.event {
            rect = rect.translate(delta);
        }
        let active = self.active || cx.response.hovered() || cx.response.has_focus();
        let fill = if active {
            cx.style.active_fill
        } else {
            cx.style.fill
        };
        let edge = if active {
            cx.style.active_edge
        } else {
            cx.style.edge
        };
        let shape = PaintShape::Rect(rect, self.radius);
        draw.fill(&shape, fill);
        draw.stroke(&shape, StrokeWidth::Thin, edge);
        draw.text(
            rect,
            &TextRun::new(self.number.as_str())
                .role(TypeRole::Caption)
                .align(HAlign::Center),
        );
    }

    fn finish(self, cx: Cx<'_, Self>) -> BubbleOutput {
        BubbleOutput {
            response: cx.response.clone(),
            event: self.event,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BubbleInteraction {
    Disabled,
    Read,
    Move,
}

fn bubble(
    ui: &mut Ui,
    room: Rect,
    anchor: Pos2,
    id: Option<i64>,
    number: usize,
    active: bool,
    interaction: BubbleInteraction,
) -> BubbleOutput {
    let side = Env::of(ui).tokens.size.control.md;
    let width = side * (1.0 + number.ilog10().saturating_sub(1) as f32 * 0.35);
    let leftward = anchor.x + width > room.right();
    let downward = anchor.y - side < room.top();
    let origin = anchor
        - vec2(
            if leftward { width } else { 0.0 },
            if downward { 0.0 } else { side },
        );
    let rect = Rect::from_min_size(origin, vec2(width, side));
    let mut radius = CornerRadius::same((side / 2.0).round() as u8);
    match (leftward, downward) {
        (false, false) => radius.sw = 1,
        (false, true) => radius.nw = 1,
        (true, false) => radius.se = 1,
        (true, true) => radius.ne = 1,
    }
    CommentBubble {
        common: Common::new(),
        number: number.to_string(),
        size: rect.size(),
        radius,
        active,
        movable: interaction == BubbleInteraction::Move,
        event: BubbleEvent::None,
    }
    .id_salt(("comment-bubble", id))
    .label(if id.is_some() {
        format!("Open comment {number}")
    } else {
        format!("Draft comment {number}")
    })
    .enabled(interaction != BubbleInteraction::Disabled)
    .show_at(ui, rect)
}

fn heading(ui: &mut Ui, room: Rect, label: &str, close: &'static str, enabled: bool) -> bool {
    let tokens = &Env::of(ui).tokens;
    let width = (tokens.size.dialog.least * 0.75).min(room.width() * 0.85);
    let title_width = (width - tokens.size.control.sm - ui.spacing().item_spacing.x).max(1.0);
    ui.set_width(width);
    ui.horizontal_top(|ui| {
        ui.scope(|ui| {
            ui.set_width(title_width);
            Text::new(label.to_owned())
                .role(TypeRole::BodyStrong)
                .show(ui);
        });
        crate::chrome_icon("close", close)
            .size(ControlSize::Sm)
            .enabled(enabled)
            .show(ui)
            .clicked()
    })
    .inner
}
