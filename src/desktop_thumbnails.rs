use crate::desktop_documents::{self, StillPreview};
use crate::{desktop_models, desktop_video};
use assetcraft::{Format, local_file};
use craft::behavior::{IconId, PixelSize};
use craft::browse::thumbs::{
    DecodeRequest, DecodedThumb, FileStat, FileThumbKey, FileThumbSource, ThumbDecoder, ThumbError,
};
use craft::color::linear_to_srgb_u8;
use craft::egui::{
    Color32, Context, Pos2, Rect, Stroke,
    emath::Rot2,
    epaint::{Mesh, Shape, tessellator::Path as PaintPath},
    vec2,
};
use craft::host::queue::Cancel;
use craft::image_codecs::{
    AnimationFrame, ChannelLayout, CodecError, DecodeOptions, DecodeWarning, Image, Limits,
    decode_as_with, decode_gif_frames, decode_webp_frames, has_unretained_source_data,
    webp_is_animated,
};
use craft::theme::{Srgba, TypeRole};
use craft::ui::primitives::textures::{
    Priority, TextureState, ThumbFit, ThumbPaint, ThumbRequest, ThumbnailOptions, Thumbnails,
    fit_rect, thumb_image,
};
use craft::ui::primitives::{ContentPainterCx, IconSpec, PaintShape, TextRun};
use craft::ui::style::UiTokens;
use std::collections::HashMap;
use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
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
    properties: Vec<(&'static str, String)>,
}

impl ThumbDecoder for ImageDecoder {
    fn decode(&self, request: &DecodeRequest, cancel: &Cancel) -> Result<DecodedThumb, ThumbError> {
        let path = &request.key.path;
        let (source, opened_stat) = read_asset_source(path, cancel)?;
        let edge = request.side.clamp(1, desktop_documents::VIEWER_EDGE);
        let stat = FileStat {
            len: request.len,
            modified: request.modified,
        };
        if opened_stat != stat {
            return Err(ThumbError::Cancelled);
        }
        let preview = match source {
            AssetSource::Media(file, format) => decode_media(file, format, edge, cancel),
            AssetSource::Image(bytes, format) => decode_still(path, bytes, format, edge, cancel),
        };
        if cancel.is_cancelled() {
            return Err(ThumbError::Cancelled);
        }
        if let Ok(mut originals) = self.originals.lock()
            && let Some(original) = originals.get_mut(path)
            && original.stat == Some(stat)
        {
            original.dimensions = preview.dimensions;
            original.properties = preview.properties;
            if matches!(preview.image, Err(ThumbError::Unsupported))
                && !original
                    .properties
                    .iter()
                    .any(|(label, _)| *label == "Preview")
            {
                original
                    .properties
                    .push(("Preview", "Unavailable for this file".to_owned()));
            }
        }
        let image = preview.image?;
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

pub struct ImagePreview {
    pub image: Image,
    pub animation: Result<Vec<AnimationFrame>, CodecError>,
}

fn decode_still(
    path: &Path,
    bytes: Vec<u8>,
    format: Format,
    edge: u32,
    cancel: &Cancel,
) -> StillPreview {
    if format.is_model() || format.is_cad() {
        return desktop_models::render(path, format, &bytes, edge, cancel);
    }
    let Some(codec) = format.codec() else {
        return desktop_documents::render(
            format,
            bytes,
            LIMITS,
            edge.clamp(1, desktop_documents::VIEWER_EDGE),
            cancel,
        );
    };
    let image = decode_image(&bytes, codec, cancel);
    let dimensions = image.as_ref().ok().map(|image| {
        let (width, height) = image.dimensions();
        PixelSize::new(width, height)
    });
    let mut preview = StillPreview::new(image);
    preview.dimensions = dimensions;
    preview
}

fn decode_media(file: File, format: Format, edge: u32, cancel: &Cancel) -> StillPreview {
    if format.is_audio() {
        crate::desktop_audio::render(file, format, edge, cancel)
    } else {
        desktop_video::render(file, format, edge, cancel)
    }
}

pub fn decode_preview(path: &Path, cancel: &Cancel) -> Result<ImagePreview, ThumbError> {
    let (bytes, format) = match read_asset_source(path, cancel)?.0 {
        AssetSource::Media(file, format) => {
            let preview = decode_media(file, format, desktop_documents::VIEWER_EDGE, cancel);
            return Ok(ImagePreview {
                image: preview.image?,
                animation: Ok(Vec::new()),
            });
        }
        AssetSource::Image(bytes, format) => (bytes, format),
    };
    let Some(codec) = format.codec() else {
        let preview = decode_still(path, bytes, format, desktop_documents::VIEWER_EDGE, cancel);
        if cancel.is_cancelled() {
            return Err(ThumbError::Cancelled);
        }
        let image = preview.image.map_err(|error| {
            if matches!(error, ThumbError::Unsupported)
                && let Some((_, reason)) = preview
                    .properties
                    .into_iter()
                    .find(|(name, _)| *name == "Preview")
            {
                ThumbError::Failed(reason)
            } else {
                error
            }
        })?;
        return Ok(ImagePreview {
            image,
            animation: Ok(Vec::new()),
        });
    };
    let image = decode_image(&bytes, codec, cancel)?;
    let options = DecodeOptions {
        limits: Limits {
            max_alloc: 32 * 1024 * 1024,
            ..LIMITS
        },
        ..DecodeOptions::default()
    };
    let animation = match format {
        Format::Gif => has_unretained_source_data(&bytes).and_then(|animated| {
            if animated {
                decode_gif_frames(&bytes, &options)
            } else {
                Ok(Vec::new())
            }
        }),
        Format::WebP if webp_is_animated(&bytes) => {
            let orientation =
                craft::image_codecs::webp_probe::header(&bytes).map_or(1, |(_, _, value)| value);
            decode_webp_frames(&bytes, &options).and_then(|frames| {
                frames
                    .into_iter()
                    .map(|mut frame| {
                        frame.image = frame.image.oriented(orientation)?;
                        Ok(frame)
                    })
                    .collect()
            })
        }
        _ => Ok(Vec::new()),
    };
    if cancel.is_cancelled() {
        return Err(ThumbError::Cancelled);
    }
    let animation = animation.and_then(|frames| {
        if frames.first().is_some_and(|frame| {
            frame
                .image
                .warnings
                .iter()
                .any(|warning| matches!(warning, DecodeWarning::Truncated { .. }))
        }) {
            return Err(CodecError::Malformed {
                format: codec,
                message: "The animation is incomplete".into(),
            });
        }
        Ok(frames)
    });
    Ok(ImagePreview { image, animation })
}

fn decode_image(
    bytes: &[u8],
    format: craft::image_codecs::Format,
    cancel: &Cancel,
) -> Result<Image, ThumbError> {
    let image = decode_as_with(
        format,
        bytes,
        &DecodeOptions {
            limits: LIMITS,
            ..DecodeOptions::default()
        },
    );
    let image = if format == craft::image_codecs::Format::Cineon
        && matches!(image, Err(CodecError::Unsupported { .. }))
        && let Some(image) = desktop_documents::cineon_8bit(bytes, LIMITS)
    {
        image
    } else {
        image
    }
    .map_err(|error| match error {
        CodecError::Unsupported { .. }
            if matches!(
                format,
                craft::image_codecs::Format::Cineon | craft::image_codecs::Format::JpegLs
            ) =>
        {
            ThumbError::Failed(error.to_string())
        }
        CodecError::UnknownFormat | CodecError::Unsupported { .. } => ThumbError::Unsupported,
        error => ThumbError::Failed(error.to_string()),
    })?;
    let image = if matches!(
        format,
        craft::image_codecs::Format::OpenExr | craft::image_codecs::Format::Hdr
    ) {
        hdr_for_display(&image, cancel)?
    } else {
        image
    };
    if cancel.is_cancelled() {
        return Err(ThumbError::Cancelled);
    }
    Ok(image)
}

fn hdr_for_display(image: &Image, cancel: &Cancel) -> Result<Image, ThumbError> {
    let (width, height) = image.dimensions();
    let layout = image.layout();
    let channels = layout.channels();
    let mut rgba = Vec::with_capacity(image.pixel_count() * 4);
    for row in 0..height as usize {
        if cancel.is_cancelled() {
            return Err(ThumbError::Cancelled);
        }
        for column in 0..width as usize {
            let offset = (row * width as usize + column) * channels;
            let sample = |channel| {
                let value = image.sample_normalized(offset + channel);
                if value.is_finite() {
                    value.max(0.0)
                } else {
                    0.0
                }
            };
            let red = sample(0);
            let rgb = if matches!(layout, ChannelLayout::Gray | ChannelLayout::GrayA) {
                [red; 3]
            } else {
                [red, sample(1), sample(2)]
            };
            // Compress highlights into the SDR preview while preserving the RGB ratios.
            let peak = rgb.into_iter().fold(0.0, f32::max);
            rgba.extend(rgb.map(|value| linear_to_srgb_u8(value / (1.0 + peak))));
            let alpha = if layout.has_alpha() {
                sample(channels - 1)
            } else {
                1.0
            };
            rgba.push((alpha.min(1.0) * 255.0).round() as u8);
        }
    }
    Image::from_u8(width, height, ChannelLayout::Rgba, rgba)
        .map_err(|error| ThumbError::Failed(error.to_string()))
}

enum AssetSource {
    Media(File, Format),
    Image(Vec<u8>, Format),
}

fn read_asset_source(path: &Path, cancel: &Cancel) -> Result<(AssetSource, FileStat), ThumbError> {
    let file_error = |error: std::io::Error| {
        if error.kind() == ErrorKind::NotFound {
            ThumbError::Missing
        } else {
            ThumbError::Failed(error.to_string())
        }
    };
    let mut file = local_file::open_read(path).map_err(file_error)?;
    let metadata = file.metadata().map_err(file_error)?;
    if !metadata.is_file() {
        return Err(ThumbError::Missing);
    }
    let stat = FileStat {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    };
    let mut head = Vec::with_capacity(craft::sniff::HEAD);
    (&mut file)
        .take(craft::sniff::HEAD as u64)
        .read_to_end(&mut head)
        .map_err(file_error)?;
    if cancel.is_cancelled() {
        return Err(ThumbError::Cancelled);
    }
    if let Some(format) =
        Format::detect(path, &head).filter(|format| format.is_video() || format.is_audio())
    {
        file.seek(SeekFrom::Start(0)).map_err(file_error)?;
        return Ok((AssetSource::Media(file, format), stat));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(ThumbError::Failed(
            "Original exceeds the preview read limit".into(),
        ));
    }
    let left = MAX_FILE_BYTES + 1 - head.len() as u64;
    file.take(left).read_to_end(&mut head).map_err(file_error)?;
    if head.len() as u64 > MAX_FILE_BYTES {
        return Err(ThumbError::Failed(
            "Original exceeds the preview read limit".into(),
        ));
    }
    if cancel.is_cancelled() {
        return Err(ThumbError::Cancelled);
    }
    let format = Format::detect(path, &head).ok_or(ThumbError::Unsupported)?;
    Ok((AssetSource::Image(head, format), stat))
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

    pub fn properties(&self, path: &Path) -> Vec<(&'static str, String)> {
        self.decoder
            .originals
            .lock()
            .ok()
            .and_then(|originals| {
                originals
                    .get(path)
                    .map(|original| original.properties.clone())
            })
            .unwrap_or_default()
    }

    pub fn observe(&self, path: &Path, stat: Option<FileStat>) {
        let Ok(mut originals) = self.decoder.originals.lock() else {
            return;
        };
        let original = originals.entry(path.to_path_buf()).or_insert(Original {
            stat,
            dimensions: None,
            properties: Vec::new(),
        });
        if original.stat != stat {
            *original = Original {
                stat,
                dimensions: None,
                properties: Vec::new(),
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
