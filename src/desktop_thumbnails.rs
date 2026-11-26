use assetcraft::local_file;
use craft::behavior::{IconId, PixelSize};
use craft::browse::thumbs::{
    DecodeRequest, DecodedThumb, FileThumbKey, FileThumbSource, ThumbDecoder, ThumbError,
};
use craft::egui::{
    Color32, Context, Rect, Stroke,
    emath::Rot2,
    epaint::{Mesh, Shape},
    vec2,
};
use craft::host::queue::Cancel;
use craft::image_codecs::{DecodeOptions, Limits, decode_as_with};
use craft::sniff::{Kind, identify};
use craft::theme::Srgba;
use craft::ui::primitives::textures::{
    Priority, TextureState, ThumbFit, ThumbPaint, ThumbRequest, ThumbnailOptions, Thumbnails,
    fit_rect, thumb_image,
};
use craft::ui::primitives::{ContentPainterCx, IconSpec};
use std::collections::HashMap;
use std::io::Read;
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
    dimensions: Mutex<HashMap<PathBuf, PixelSize>>,
}

impl ThumbDecoder for ImageDecoder {
    fn decode(&self, request: &DecodeRequest, cancel: &Cancel) -> Result<DecodedThumb, ThumbError> {
        let path = &request.key.path;
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
        if let Ok(mut dimensions) = self.dimensions.lock() {
            dimensions.insert(path.clone(), size);
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

fn read_original(path: &Path) -> Result<Vec<u8>, ThumbError> {
    let file =
        local_file::open_read(path).map_err(|error| ThumbError::Failed(error.to_string()))?;
    let metadata = file
        .metadata()
        .map_err(|error| ThumbError::Failed(error.to_string()))?;
    if !metadata.is_file() {
        return Err(ThumbError::Unsupported);
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(ThumbError::Failed(
            "Original exceeds the thumbnail read limit".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ThumbError::Failed(error.to_string()))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ThumbError::Failed(
            "Original exceeds the thumbnail read limit".into(),
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
        self.decoder.dimensions.lock().ok()?.get(path).copied()
    }

    pub fn clear(&self) {
        self.files.clear();
        if let Ok(mut dimensions) = self.decoder.dimensions.lock() {
            dimensions.clear();
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
        let gap = style.tokens.space.gap.tight;
        let paper = Color32::from(Srgba::from(style.tokens.color.surface.raised));
        let edge = Color32::from(Srgba::from(style.tokens.color.line.frame));
        let size = rect.size() * if count > 1 { 0.70 } else { 0.84 };
        let offset = rect.size().min_elem() * 0.09;
        let placement: &[(f32, f32, f32)] = match count {
            1 => &[(0.0, 0.0, 0.0)],
            2 => &[(1.0, 0.55, 0.12), (-1.0, -0.55, -0.15)],
            _ => &[(0.9, 0.85, 0.09), (-1.2, 0.0, -0.15), (0.55, -1.25, 0.16)],
        };
        for (path, &(x, y, angle)) in paths.into_iter().zip(placement).rev() {
            let center = rect.center() + vec2(x, y) * offset;
            let card = Rect::from_center_size(center, size);
            let rotation = Rot2::from_angle(angle);
            let corners = [
                card.left_top(),
                card.right_top(),
                card.right_bottom(),
                card.left_bottom(),
            ];
            cx.painter.add(Shape::convex_polygon(
                corners
                    .into_iter()
                    .map(|corner| center + rotation * (corner - center))
                    .collect(),
                paper,
                Stroke::new(style.tokens.stroke.thin, edge),
            ));
            let inner = card.shrink(gap.min(card.size().min_elem() / 4.0));
            let texture = path.and_then(|path| self.state(cx, inner, path, surface).shown());
            if let Some(texture) = texture {
                let mut mesh = Mesh::with_texture(texture.id);
                mesh.add_rect_with_uv(
                    fit_rect(inner, texture.size_vec2()),
                    texture.uv(),
                    Color32::WHITE,
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
