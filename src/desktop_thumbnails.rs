use assetcraft::local_file;
use craft::behavior::{IconId, PixelSize};
use craft::browse::thumbs::{
    DecodeRequest, DecodedThumb, FileStat, FileThumbKey, FileThumbSource, ThumbDecoder, ThumbError,
};
use craft::egui::{
    Color32, Context, Pos2, Rect, Stroke,
    emath::Rot2,
    epaint::{Mesh, Shape, tessellator::Path as PaintPath},
    vec2,
};
use craft::host::queue::Cancel;
use craft::image_codecs::{DecodeOptions, Image, Limits, decode_as_with};
use craft::sniff::{Kind, identify};
use craft::theme::{Srgba, TypeRole};
use craft::ui::primitives::textures::{
    Priority, TextureState, ThumbFit, ThumbPaint, ThumbRequest, ThumbnailOptions, Thumbnails,
    fit_rect, thumb_image,
};
use craft::ui::primitives::{ContentPainterCx, IconSpec, PaintShape, TextRun};
use craft::ui::style::UiTokens;
use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const LIMITS: Limits = Limits {
    max_width: 32_768,
    max_height: 32_768,
    max_pixels: 32_000_000,
    max_alloc: 128 * 1024 * 1024,
};

#[derive(Default)]
struct ImageDecoder {
    originals: Mutex<HashMap<PathBuf, Original>>,
}

struct Original {
    stat: Option<FileStat>,
    dimensions: Option<PixelSize>,
}

impl ThumbDecoder for ImageDecoder {
    fn decode(&self, request: &DecodeRequest, cancel: &Cancel) -> Result<DecodedThumb, ThumbError> {
        let path = &request.key.path;
        let image = decode_original(path, cancel)?;

        let (width, height) = image.dimensions();
        let size = PixelSize::new(width, height);
        let rgba = image.to_rgba8();
        let thumb = thumb_image(size, request.side.min(width.max(height)), |x, y| {
            let start = (y as usize * width as usize + x as usize) * 4;
            [
                rgba[start],
                rgba[start + 1],
                rgba[start + 2],
                rgba[start + 3],
            ]
        });
        let stat = FileStat {
            len: request.len,
            modified: request.modified,
        };
        if let Ok(mut originals) = self.originals.lock()
            && let Some(original) = originals.get_mut(path)
            && original.stat == Some(stat)
        {
            original.dimensions = Some(size);
        }
        Ok(DecodedThumb {
            width: thumb.size[0] as u32,
            height: thumb.size[1] as u32,
            rgba: thumb
                .pixels
                .into_iter()
                .flat_map(|pixel| pixel.to_srgba_unmultiplied())
                .collect(),
        })
    }
}

pub fn decode_original(path: &Path, cancel: &Cancel) -> Result<Image, ThumbError> {
    let bytes = read_original(path)?;
    if cancel.is_cancelled() {
        return Err(ThumbError::Cancelled);
    }
    use craft::image_codecs::Format;
    let format = match identify(&bytes) {
        Some(Kind::Png) => Format::Png,
        Some(Kind::Jpeg) => Format::Jpeg,
        _ => return Err(ThumbError::Unsupported),
    };
    let image = decode_as_with(
        format,
        &bytes,
        &DecodeOptions {
            limits: LIMITS,
            ..DecodeOptions::default()
        },
    )
    .map_err(|error| ThumbError::Failed(error.to_string()))?;
    if cancel.is_cancelled() {
        return Err(ThumbError::Cancelled);
    }
    Ok(image)
}

fn read_original(path: &Path) -> Result<Vec<u8>, ThumbError> {
    let file = local_file::open_read(path).map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            ThumbError::Missing
        } else {
            ThumbError::Failed(error.to_string())
        }
    })?;
    let metadata = file
        .metadata()
        .map_err(|error| ThumbError::Failed(error.to_string()))?;
    if !metadata.is_file() {
        return Err(ThumbError::Missing);
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(ThumbError::Failed(
            "Original exceeds the image preview read limit".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ThumbError::Failed(error.to_string()))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ThumbError::Failed(
            "Original exceeds the image preview read limit".into(),
        ));
    }
    Ok(bytes)
}

#[derive(Clone, Copy)]
pub enum PreviewSurface {
    Grid,
    Inspector,
    Drag,
}

pub struct Previews {
    decoder: Arc<ImageDecoder>,
    files: FileThumbSource,
    textures: Thumbnails<FileThumbKey>,
}

impl Previews {
    pub fn new(ctx: &Context) -> Self {
        let decoder = Arc::new(ImageDecoder::default());
        Self {
            files: FileThumbSource::new(decoder.clone(), None),
            decoder,
            textures: Thumbnails::in_context(ctx, "assetcraft-images", ThumbnailOptions::new()),
        }
    }

    pub fn dimensions(&self, path: &Path) -> Option<PixelSize> {
        self.decoder.originals.lock().ok()?.get(path)?.dimensions
    }

    pub fn observe(&self, path: &Path, stat: Option<FileStat>) {
        let Ok(mut originals) = self.decoder.originals.lock() else {
            return;
        };
        let original = originals.entry(path.to_path_buf()).or_insert(Original {
            stat,
            dimensions: None,
        });
        if original.stat != stat {
            *original = Original {
                stat,
                dimensions: None,
            };
            self.files.invalidate(path);
        }
    }

    pub fn clear(&self) {
        self.files.clear();
        if let Ok(mut originals) = self.decoder.originals.lock() {
            originals.clear();
        }
    }

    pub fn paint(
        &self,
        cx: &ContentPainterCx<'_>,
        rect: Rect,
        path: &Path,
        surface: PreviewSurface,
    ) {
        let state = self.state(cx, rect, path, surface);
        self.textures.paint(
            cx,
            rect,
            &state,
            &ThumbPaint::default().fit(ThumbFit::Contain).checker(false),
        );
    }

    pub fn paint_selection(
        &self,
        cx: &ContentPainterCx<'_>,
        rect: Rect,
        paths: [Option<&Path>; 3],
        count: usize,
        surface: PreviewSurface,
    ) {
        if count == 1
            && let Some(path) = paths[0]
        {
            self.paint(cx, rect, path, surface);
            return;
        }
        if count == 0 {
            return;
        }
        let style = cx.style_cx();
        let paper = Color32::from(Srgba::from(style.tokens.color.surface.raised));
        let edge = Color32::from(Srgba::from(style.tokens.color.line.frame));
        let side = rect.size().min_elem();
        if side <= 0.0 {
            return;
        }
        let size = craft::egui::Vec2::splat(side * if count > 1 { 0.72 } else { 0.84 });
        let gap = style.tokens.space.gap.tight.min(size.x / 4.0);
        let offset = side * 0.07;
        let placement: &[(f32, f32, f32)] = match count {
            1 => &[(0.0, 0.0, 0.0)],
            2 => &[(1.0, 0.55, 0.12), (-1.0, -0.55, -0.15)],
            _ => &[(0.9, 0.85, 0.09), (-1.2, 0.0, -0.15), (0.55, -1.25, 0.16)],
        };
        let mut badge_anchor = rect.right_top();
        for (path, &(x, y, angle)) in paths.into_iter().zip(placement).rev() {
            let center = rect.center() + vec2(x, y) * offset;
            let slot = Rect::from_center_size(center, size).shrink(gap);
            let texture = path.and_then(|path| self.state(cx, slot, path, surface).shown());
            let aspect = path
                .and_then(|path| self.dimensions(path))
                .map(|size| vec2(size.width as f32, size.height as f32))
                .or_else(|| texture.as_ref().map(|texture| texture.size_vec2()));
            let picture = aspect.map_or(slot, |aspect| fit_rect(slot, aspect));
            let card = picture.expand(gap);
            let rotation = Rot2::from_angle(angle);
            let corners = [
                card.left_top(),
                card.right_top(),
                card.right_bottom(),
                card.left_bottom(),
            ]
            .map(|corner| center + rotation * (corner - center));
            badge_anchor = corners[1];
            cx.painter.add(Shape::convex_polygon(
                corners.to_vec(),
                paper,
                Stroke::new(style.tokens.stroke.thin, edge),
            ));
            if let Some(texture) = texture {
                let uv = texture.uv();
                let feather = (1.0 / cx.painter.pixels_per_point()).min(picture.size().min_elem());
                let mut outline = PaintPath::default();
                outline.add_line_loop(&[
                    picture.left_top(),
                    picture.right_top(),
                    picture.right_bottom(),
                    picture.left_bottom(),
                ]);
                let mut mesh = Mesh::with_texture(texture.id);
                // A regular textured quad has a hard edge after rotation. Feather the outline
                // by one physical pixel and keep its UVs inside this thumbnail's atlas region.
                outline.fill_with_uv(
                    feather,
                    Color32::WHITE,
                    texture.id,
                    |at| {
                        let xy = (at - picture.min) / picture.size();
                        uv.lerp_inside(
                            xy.clamp(craft::egui::Vec2::ZERO, craft::egui::Vec2::splat(1.0)),
                        )
                    },
                    &mut mesh,
                );
                mesh.rotate(rotation, center);
                cx.data_shapes([Shape::mesh(mesh)]);
            } else {
                let icon = Rect::from_center_size(
                    center,
                    size.min(craft::egui::Vec2::splat(style.tokens.size.icon.md)),
                );
                cx.with_draw(|draw| draw.icon(icon, &IconSpec::svg(IconId::new("file"))));
            }
        }
        if count > 1 {
            paint_stack_count(cx, rect, badge_anchor, count);
        }
    }

    fn state(
        &self,
        cx: &ContentPainterCx<'_>,
        rect: Rect,
        path: &Path,
        surface: PreviewSurface,
    ) -> TextureState {
        let ctx = cx.painter.ctx();
        let request =
            ThumbRequest::for_cell(rect.width().max(rect.height()), ctx.pixels_per_point());
        // Independent resolutions keep the preview surfaces from invalidating each other's cached thumbnails.
        let variant = match surface {
            PreviewSurface::Grid => 0,
            PreviewSurface::Inspector => 1,
            PreviewSurface::Drag => 2,
        };
        let key = FileThumbKey {
            path: path.to_path_buf(),
            variant,
        };
        self.textures
            .state(ctx, &self.files, &key, &request, Priority::VISIBLE)
    }
}

fn paint_stack_count(cx: &ContentPainterCx<'_>, room: Rect, anchor: Pos2, count: usize) {
    let style = cx.style_cx();
    let tokens = &style.tokens;
    let count = count.to_string();
    cx.with_draw(|draw| {
        let run = TextRun::new(&count)
            .role(TypeRole::Caption)
            .color(style.color(tokens.color.text.on_strong));
        let label = draw.layout(&run, f32::INFINITY);
        let height = label.size().y + 2.0 * tokens.space.pad.chip;
        let width = (label.size().x + 2.0 * tokens.space.pad.control).max(height);
        let size = vec2(width, height);
        if width > room.width() || height > room.height() {
            return;
        }
        let center = room.shrink2(size / 2.0).clamp(anchor);
        let badge = Rect::from_center_size(center, size);
        draw.fill(
            &PaintShape::Rect(badge, UiTokens::corner(tokens.shape.radius.pill)),
            style.color(tokens.color.accent.strong),
        );
        draw.galley(badge.center() - label.size() / 2.0, &label);
    });
}
