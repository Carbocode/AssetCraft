use std::{
    collections::HashMap,
    io::{ErrorKind, Read},
    path::Path,
};

use assetcraft::{Asset, Format, local_file};
use craft::behavior::{EditPolicy, kurbo::Rect as OutlineBounds};
use craft::egui::{self, Color32, Context, Id, Rect, Ui, epaint::Mesh};
use craft::font_catalog::{FontOutline, sfnt};
use craft::host::queue::{JobQueue, Priority, QueueOptions};
use craft::host::{Host, Wake};
use craft::i18n::Word;
use craft::theme::{ControlSize, Srgba, TypeRole, variant::Tone};
use craft::ui::choice::Segmented;
use craft::ui::contract::{Bind, Entry, Props, Show, ShowContainer, Sizing, Trigger};
use craft::ui::display::{Elide, Shade, Text};
use craft::ui::menu::{Menu, MenuItem};
use craft::ui::notices::Notice;
use craft::ui::runtime::{
    env::{Env, in_room},
    focus,
};
use craft::ui::text::TextField;
use craft::ui::value_drag::Slider;
use harfrust::{ShapeOptions, ShaperData, ShaperInstance, UnicodeBuffer, Variation};
use skrifa::{FontRef, MetadataProvider, string::StringId};

use crate::{UiTokens, chrome_icon};
#[path = "desktop_font_mesh.rs"]
mod font_mesh;
use crate::desktop_documents::woff::{self, MAX_FONT_BYTES};
const MAX_TEXT_CHARS: usize = 768;
const SAMPLE: &str = "Sphinx of black quartz,\njudge my vow.\n0123456789 & @ ? !";

pub(super) struct FontViewer {
    jobs: JobQueue<u64, Result<FontFile, String>>,
    key: u64,
    state: Option<Result<FontFile, String>>,
    selected: Option<(usize, usize)>,
    variations: Vec<f64>,
    custom: Option<String>,
    characters: bool,
    size: f64,
    drawing: Option<Specimen>,
    controls: Vec<Id>,
}

struct FontFile {
    data: Vec<u8>,
    faces: Vec<Face>,
    format: Format,
}

struct Face {
    index: u32,
    family: String,
    styles: Vec<Style>,
    axes: Vec<Axis>,
    sample: String,
    characters: String,
}

struct Style {
    name: String,
    coordinates: Vec<f64>,
}

struct Axis {
    tag: [u8; 4],
    name: String,
    min: f64,
    max: f64,
    hidden: bool,
}

struct Specimen {
    mesh: Mesh,
    bounds: OutlineBounds,
    missing: usize,
    omitted: usize,
    pixels_per_point: f32,
}

impl FontViewer {
    pub(super) fn new(host: &Host, wake: Wake) -> Self {
        Self {
            jobs: JobQueue::new(
                host.jobs_shared().clone(),
                wake,
                QueueOptions {
                    lanes: 1,
                    name: "Open font specimen",
                    ..QueueOptions::default()
                },
            ),
            key: 0,
            state: None,
            selected: None,
            variations: Vec::new(),
            custom: None,
            characters: false,
            size: 48.0,
            drawing: None,
            controls: Vec::new(),
        }
    }

    pub(super) fn open(&mut self, asset: &Asset) {
        let path = asset.path.clone();
        let format = asset.format;
        self.jobs
            .submit(self.key, Priority::VISIBLE, move |cancel| {
                if cancel.is_cancelled() {
                    return Err("Font preview cancelled".to_owned());
                }
                FontFile::open(&path, format)
            });
    }

    pub(super) fn close(&mut self) {
        self.jobs.reprioritize(|_, _| None);
        self.key = self.key.wrapping_add(1);
        self.state = None;
        self.selected = None;
        self.variations.clear();
        self.custom = None;
        self.characters = false;
        self.size = 48.0;
        self.drawing = None;
        self.controls.clear();
    }

    pub(super) fn receive(&mut self) -> Option<Word> {
        let mut announcement = None;
        while let Some(job) = self.jobs.try_recv() {
            if job.key == self.key {
                announcement = Some(Word::plain(if job.result.is_ok() {
                    "Font preview ready"
                } else {
                    "Font preview unavailable"
                }));
                if let Ok(file) = &job.result {
                    self.selected = Some((0, 0));
                    self.variations
                        .clone_from(&file.faces[0].styles[0].coordinates);
                }
                self.state = Some(job.result);
            }
        }
        announcement
    }

    pub(super) fn control_focused(&self, ctx: &Context) -> bool {
        self.controls.iter().any(|&id| focus::has(ctx, id))
    }

    pub(super) fn show(&mut self, ui: &mut Ui) {
        let pad = f32::from(crate::panel_padding(ui));
        let room = ui.available_rect_before_wrap().shrink(pad);
        let minimum = if room.width() < 234.0 { 274.0 } else { 240.0 };
        if matches!(self.state, Some(Ok(_))) && room.height() < minimum {
            egui::ScrollArea::vertical()
                .id_salt("compact-font-specimen")
                .show(ui, |ui| {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), minimum + 2.0 * pad),
                        egui::Sense::hover(),
                    );
                    in_room(ui, rect, |ui| self.show_specimen(ui));
                });
        } else {
            self.show_specimen(ui);
        }
    }

    fn show_specimen(&mut self, ui: &mut Ui) {
        let file = match &self.state {
            Some(Ok(file)) => file,
            Some(Err(error)) => {
                Notice::new(Tone::Bad, Word::plain("Font preview unavailable"))
                    .lines([error.clone()])
                    .show(ui);
                return;
            }
            None => {
                Text::new(Word::plain("Loading font…")).show(ui);
                return;
            }
        };
        self.controls.clear();
        let Some((face_index, style_index)) = self.selected else {
            return;
        };
        let face = &file.faces[face_index];
        let style = &face.styles[style_index];
        let room = ui
            .available_rect_before_wrap()
            .shrink(f32::from(crate::panel_padding(ui)));
        if room.height() < 168.0 || room.width() < 90.0 {
            return;
        }
        let env = Env::of(ui);
        let colors = &env.tokens.color;
        let color = |value| Color32::from(Srgba::from(value));
        let band = |top, bottom| {
            Rect::from_min_max(
                egui::pos2(room.left(), top),
                egui::pos2(room.right(), bottom),
            )
        };
        let header = band(room.top(), room.top() + 50.0);
        in_room(ui, header, |ui| {
            Text::new(face.family.clone())
                .role(TypeRole::Heading)
                .elide(Elide::End)
                .show(ui);
            let style = if self.variations == style.coordinates {
                &style.name
            } else {
                "Custom"
            };
            Text::new(format!("{style} · {}", file.format.name()))
                .role(TypeRole::Caption)
                .shade(Shade::Muted)
                .elide(Elide::End)
                .show(ui);
        });
        let toolbar = band(header.bottom(), header.bottom() + 34.0);
        let width = toolbar.width();
        let mode_width = 138.0;
        let tools_width = 96.0;
        let compact = width < mode_width + tools_width;
        let modes =
            Rect::from_min_size(toolbar.left_top(), egui::vec2(mode_width, toolbar.height()));
        let tools = Rect::from_min_max(
            egui::pos2(
                toolbar.right() - tools_width,
                if compact {
                    toolbar.bottom()
                } else {
                    toolbar.top()
                },
            ),
            toolbar.right_bottom() + egui::vec2(0.0, if compact { toolbar.height() } else { 0.0 }),
        );
        let mut choice = None;
        let mut changed = false;
        let old_mode = self.characters;
        in_room(ui, modes, |ui| {
            let options = [Entry::new(false, "Sample"), Entry::new(true, "Glyphs")];
            let response = Segmented::new(
                Word::plain("Font preview mode"),
                Bind::bound(&mut self.characters),
                &options,
            )
            .size(ControlSize::Sm)
            .sizing(Sizing::Content)
            .show(ui);
            self.controls.push(response.response.id);
        });
        changed |= self.characters != old_mode;

        in_room(ui, tools, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if chrome_icon("reset", "Reset specimen")
                    .size(ControlSize::Sm)
                    .show(ui)
                    .clicked()
                {
                    self.size = 48.0;
                    self.characters = false;
                    self.variations.clone_from(&style.coordinates);
                    self.custom = None;
                    changed = true;
                }
                let axes = chrome_icon("adjustments", "Variable axes")
                    .size(ControlSize::Sm)
                    .enabled(face.axes.iter().any(|axis| !axis.hidden))
                    .show(ui);
                Menu::new(Word::plain("Variable axes"))
                    .opened_by(&axes, Trigger::Click)
                    .show(ui, |ui, _| {
                        for (axis, value) in face.axes.iter().zip(&mut self.variations) {
                            if !axis.hidden {
                                Text::new(axis.name.clone())
                                    .role(TypeRole::Caption)
                                    .show(ui);
                                changed |= slider(
                                    ui,
                                    axis.name.as_str(),
                                    value,
                                    axis.min..=axis.max,
                                    &mut self.controls,
                                );
                            }
                        }
                    });
                let styles = chrome_icon("font_variable", "Font styles")
                    .size(ControlSize::Sm)
                    .enabled(file.faces.len() > 1 || face.styles.len() > 1)
                    .show(ui);
                Menu::new(Word::plain("Font faces and styles"))
                    .opened_by(&styles, Trigger::Click)
                    .show(ui, |ui, _| {
                        for (i, face) in file.faces.iter().enumerate() {
                            for (j, style) in face.styles.iter().enumerate() {
                                let label = if file.faces.len() > 1 {
                                    format!("{} · {}", face.family, style.name)
                                } else {
                                    style.name.clone()
                                };
                                if MenuItem::new(label)
                                    .id_salt((i, j))
                                    .check(
                                        self.selected == Some((i, j))
                                            && self.variations == style.coordinates,
                                    )
                                    .show(ui)
                                    .ran
                                {
                                    choice = Some((i, j));
                                }
                            }
                        }
                    });
            });
        });
        let sizes_top = toolbar.bottom().max(tools.bottom());
        let sizes = Rect::from_min_size(
            egui::pos2(room.left(), sizes_top),
            egui::vec2(width.min(286.0), 32.0),
        );
        let size_track = Rect::from_min_max(
            egui::pos2(sizes.left() + 37.0, sizes.top()),
            egui::pos2(sizes.right() - 48.0, sizes.bottom()),
        );
        ui.painter().text(
            egui::pos2(sizes.left(), sizes.center().y),
            egui::Align2::LEFT_CENTER,
            "Size",
            egui::FontId::proportional(12.0),
            color(colors.text.muted),
        );
        in_room(ui, size_track, |ui| {
            changed |= slider(
                ui,
                "Preview size",
                &mut self.size,
                12.0..=144.0,
                &mut self.controls,
            );
        });
        ui.painter().text(
            egui::pos2(sizes.right(), sizes.center().y),
            egui::Align2::RIGHT_CENTER,
            format!("{:.0} px", self.size),
            egui::FontId::monospace(12.0),
            color(colors.text.muted),
        );
        if let Some((i, j)) = choice {
            self.selected = Some((i, j));
            self.variations
                .clone_from(&file.faces[i].styles[j].coordinates);
            changed = true;
        }
        let face = &file.faces[self.selected.map_or(face_index, |(i, _)| i)];
        let mut sample = self.custom.clone().unwrap_or_else(|| face.sample.clone());
        let editor_height = 72.0;
        let editor = band(room.bottom() - editor_height, room.bottom());
        in_room(ui, editor, |ui| {
            Text::new(Word::plain("Sample text"))
                .role(TypeRole::Caption)
                .shade(Shade::Muted)
                .show(ui);
            let field = TextField::new(
                Word::plain("Sample text"),
                Bind::bound(&mut sample).policy(EditPolicy::WriteThrough),
            )
            .lines(3)
            .max_lines(3)
            .placeholder("Type a sample…")
            .sizing(Sizing::FillWidth);
            let response = field.show(ui);
            self.controls.push(response.response.id);
        });
        if sample != *self.custom.as_ref().unwrap_or(&face.sample) {
            if let Some((end, _)) = sample.char_indices().nth(MAX_TEXT_CHARS) {
                sample.truncate(end);
            }
            self.custom = Some(sample.clone());
            self.characters = false;
            changed = true;
        }
        let sample = if self.characters {
            &face.characters
        } else {
            &sample
        };
        let pixels_per_point = ui.ctx().pixels_per_point();
        if changed
            || self
                .drawing
                .as_ref()
                .is_none_or(|drawing| drawing.pixels_per_point != pixels_per_point)
        {
            self.drawing = Specimen::shape(
                file,
                face,
                &self.variations,
                self.size,
                sample,
                pixels_per_point,
            );
        }
        let canvas = band(sizes.bottom() + 4.0, editor.top() - 8.0);
        if canvas.height() > 10.0 {
            let radius = UiTokens::corner(env.tokens.shape.radius.panel);
            ui.painter().rect(
                canvas,
                radius,
                color(colors.surface.canvas),
                egui::Stroke::new(1.0, color(colors.line.border)),
                egui::StrokeKind::Inside,
            );
            in_room(ui, canvas.shrink(1.0), |ui| match &self.drawing {
                Some(specimen) => specimen.show(ui, sample),
                None => {
                    Text::new(Word::plain("Could not draw the selected face")).show(ui);
                }
            });
        }
    }
}

fn slider(
    ui: &mut Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    ids: &mut Vec<Id>,
) -> bool {
    let old = *value;
    let output = Slider::new(Bind::bound(value), range)
        .label(label.to_owned())
        .sizing(Sizing::FillWidth)
        .size(ControlSize::Sm)
        .show(ui);
    ids.push(output.response.id);
    *value != old
}

impl FontFile {
    fn open(path: &Path, format: Format) -> Result<Self, String> {
        let file_error = |err: std::io::Error| {
            if err.kind() == ErrorKind::NotFound {
                "Original unavailable".to_owned()
            } else {
                err.to_string()
            }
        };
        let file = local_file::open_read(path).map_err(file_error)?;
        let metadata = file.metadata().map_err(file_error)?;
        if !metadata.is_file() {
            return Err("Original unavailable".into());
        }
        let mut data = Vec::new();
        if metadata.len() > MAX_FONT_BYTES as u64 {
            return Err("Original exceeds the font preview read limit".into());
        }
        file.take(MAX_FONT_BYTES as u64 + 1)
            .read_to_end(&mut data)
            .map_err(file_error)?;
        if data.len() > MAX_FONT_BYTES {
            return Err("Original exceeds the font preview read limit".into());
        }
        let data = if matches!(format, Format::Woff | Format::Woff2) {
            woff::decode(&data).ok_or("Invalid or unsupported web font")?
        } else {
            data
        };
        let faces: Vec<_> = sfnt::read_faces_bytes(&data)
            .into_iter()
            .filter_map(|names| Face::read(&data, names))
            .collect();
        if faces.is_empty() {
            return Err("No readable OpenType faces in this file".into());
        }
        Ok(Self {
            data,
            faces,
            format,
        })
    }
}

impl Face {
    fn read(data: &[u8], names: sfnt::FaceNames) -> Option<Self> {
        let font = FontRef::from_index(data, names.index).ok()?;
        let name = |id: StringId| {
            font.localized_strings(id)
                .english_or_first()
                .map(|value| value.to_string())
        };
        let mut axes = Vec::new();
        let mut defaults = Vec::new();
        for axis in font.axes().iter() {
            let tag = axis.tag().to_be_bytes();
            axes.push(Axis {
                tag,
                name: name(axis.name_id())
                    .unwrap_or_else(|| String::from_utf8_lossy(&tag).into_owned()),
                min: f64::from(axis.min_value()),
                max: f64::from(axis.max_value()),
                hidden: axis.is_hidden() || axis.min_value() >= axis.max_value(),
            });
            defaults.push(f64::from(axis.default_value()));
        }
        let mut styles = vec![Style {
            name: names.style,
            coordinates: defaults,
        }];
        for instance in font.named_instances().iter() {
            let Some(label) = name(instance.subfamily_name_id()) else {
                continue;
            };
            let coordinates: Vec<f64> = instance.user_coords().map(f64::from).collect();
            if !styles.iter().any(|style| style.coordinates == coordinates) {
                styles.push(Style {
                    name: label,
                    coordinates,
                });
            }
        }
        let map = font.charmap();
        let mut characters = String::new();
        let mut seen = std::collections::HashSet::new();
        for (pos, value) in map
            .mappings()
            .take(16_384)
            .filter_map(|(code, glyph)| {
                char::from_u32(code)
                    .filter(|ch| !ch.is_control() && !ch.is_whitespace())
                    .map(|ch| (ch, glyph))
            })
            .filter(|(_, glyph)| glyph.to_u32() != 0 && seen.insert(glyph.to_u32()))
            .map(|(ch, _)| ch)
            .take(150)
            .enumerate()
        {
            if pos > 0 && pos % 15 == 0 {
                characters.push('\n');
            } else if pos > 0 {
                characters.push(' ');
            }
            characters.push(value);
        }
        let sample = if "Sphinx".chars().all(|ch| map.map(ch).is_some()) {
            SAMPLE.to_owned()
        } else {
            characters.lines().take(3).collect::<Vec<_>>().join("\n")
        };
        Some(Self {
            index: names.index,
            family: names.family,
            styles,
            axes,
            sample,
            characters,
        })
    }
}

impl Specimen {
    fn shape(
        file: &FontFile,
        face: &Face,
        values: &[f64],
        size: f64,
        text: &str,
        pixels_per_point: f32,
    ) -> Option<Self> {
        let settings: Vec<_> = face
            .axes
            .iter()
            .zip(values)
            .map(|(axis, &value)| (axis.tag, value as f32))
            .collect();
        let outline = FontOutline::from_bytes(&file.data, face.index, &settings).ok()?;
        let font = harfrust::FontRef::from_index(&file.data, face.index).ok()?;
        let data = ShaperData::new(&font);
        let instance = ShaperInstance::from_variations(
            &font,
            settings.iter().map(|&(tag, value)| Variation {
                tag: harfrust::Tag::new(&tag),
                value,
            }),
        );
        let shaper = data.shaper(&font).instance(Some(&instance)).build();
        let metrics = outline.metrics();
        let scale = size / f64::from(metrics.units_per_em).max(1.0);
        let feather = f64::from(pixels_per_point).recip();
        let row_height = (f64::from(metrics.ascent - metrics.descent + metrics.leading) * scale)
            .clamp(size, size * 3.0);
        let mut bounds = OutlineBounds::new(0.0, 0.0, 0.0, 0.0);
        let mut mesh = Mesh::default();
        let mut cache = HashMap::new();
        let mut baseline = (f64::from(metrics.ascent) * scale).clamp(size * 0.6, size * 3.0);
        let mut missing = 0;
        let mut omitted = 0;
        for line in text.split('\n').take(64) {
            let mut buffer = UnicodeBuffer::new();
            buffer.push_str(line);
            buffer.guess_segment_properties();
            let shaped = shaper.shape(buffer, ShapeOptions::new());
            let mut x = 0.0;
            for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                let glyph = info.glyph_id;
                let shape = cache
                    .entry(glyph)
                    .or_insert_with(|| font_mesh::glyph(&outline, glyph, scale, feather));
                let at = egui::vec2(
                    (x + f64::from(pos.x_offset) * scale) as f32,
                    (baseline - f64::from(pos.y_offset) * scale) as f32,
                );
                let over_limit = shape
                    .as_ref()
                    .is_ok_and(|shape| mesh.vertices.len() + shape.vertices.len() > 400_000);
                if let Ok(shape) = shape
                    && !over_limit
                {
                    let points = shape.calc_bounds().translate(at);
                    if points.is_finite() {
                        bounds = bounds.union(OutlineBounds::new(
                            points.left() as f64,
                            points.top() as f64,
                            points.right() as f64,
                            points.bottom() as f64,
                        ));
                    }
                    let first = mesh.vertices.len() as u32;
                    mesh.indices.extend(shape.indices.iter().map(|i| first + i));
                    mesh.vertices.extend(shape.vertices.iter().map(|vertex| {
                        egui::epaint::Vertex {
                            pos: vertex.pos + at,
                            ..*vertex
                        }
                    }));
                }
                match shape {
                    Err(font_mesh::GlyphError::Budget) => omitted += 1,
                    Err(font_mesh::GlyphError::Unavailable) => missing += 1,
                    Ok(_) if over_limit => omitted += 1,
                    Ok(_) if glyph == 0 => missing += 1,
                    Ok(_) => {}
                }
                x += f64::from(pos.x_advance) * scale;
            }
            bounds.x1 = bounds.x1.max(x);
            baseline += row_height;
        }
        bounds.y1 = bounds.y1.max(baseline);
        Some(Self {
            mesh,
            bounds,
            missing,
            omitted,
            pixels_per_point,
        })
    }

    fn show(&self, ui: &mut Ui, sample: &str) {
        if sample.trim().is_empty() {
            Text::new(Word::plain("Enter sample text to preview it in this font.")).show(ui);
            return;
        }
        if self.mesh.is_empty() && self.omitted == 0 {
            Text::new(Word::plain("No drawable outlines in this face")).show(ui);
            return;
        }
        let env = Env::of(ui);
        let color = egui::Color32::from(Srgba::from(env.tokens.color.text.normal));
        let status_ink = egui::Color32::from(Srgba::from(env.tokens.color.text.muted));
        let canvas = ui.available_rect_before_wrap();
        let status = canvas.bottom() - 20.0;
        let specimen_area = Rect::from_min_max(canvas.min, egui::pos2(canvas.right(), status));
        in_room(ui, specimen_area, |ui| {
            ui.style_mut().spacing.scroll.floating = true;
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let padding = 24.0;
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(
                            self.bounds.width() as f32 + padding * 2.0,
                            self.bounds.height() as f32 + padding,
                        ),
                        egui::Sense::hover(),
                    );
                    response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, sample));
                    if ui.is_rect_visible(rect) && !self.mesh.is_empty() {
                        let mut mesh = self.mesh.clone();
                        mesh.translate(
                            rect.min.to_vec2()
                                + egui::vec2(
                                    padding - self.bounds.x0 as f32,
                                    padding - self.bounds.y0 as f32,
                                ),
                        );
                        for vertex in &mut mesh.vertices {
                            vertex.color =
                                color.gamma_multiply(f32::from(vertex.color.a()) / 255.0);
                        }
                        ui.painter().add(egui::Shape::mesh(mesh));
                    }
                });
        });
        if self.missing > 0 || self.omitted > 0 {
            let status_text = if self.omitted > 0 {
                format!(
                    "Preview incomplete · {} omitted · {} unavailable",
                    self.omitted, self.missing
                )
            } else {
                format!("{} glyphs unavailable in this face", self.missing)
            };
            ui.painter().text(
                egui::pos2(canvas.left() + 12.0, status + 10.0),
                egui::Align2::LEFT_CENTER,
                status_text,
                egui::FontId::proportional(12.0),
                status_ink,
            );
        }
    }
}
