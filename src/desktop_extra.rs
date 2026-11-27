use std::io::Read;

use assetcraft::Format;
use craft::behavior::PixelSize;
use craft::browse::thumbs::ThumbError;
use craft::host::queue::Cancel;
use craft::image_codecs::{Image, Limits};
use craft::svg::tree::draw::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};
use serde_json::Value as Json;

use crate::desktop_documents::{StillPreview, embedded, failed, rgba, straight_rgba, zip_preview};

const INPUT_BYTES: usize = 64 << 20;

pub fn render(
    format: Format,
    bytes: &[u8],
    limits: Limits,
    edge: u32,
    cancel: &Cancel,
) -> StillPreview {
    let edge = edge.clamp(32, 768);
    let result = match format {
        Format::Xcf => xcf(bytes, edge, cancel),
        Format::Gpl => palette(bytes, edge),
        Format::MaterialX => material(bytes, edge),
        Format::Lottie | Format::DotLottie => lottie(bytes, format, edge, cancel),
        Format::Sketch => sketch(bytes, limits),
        Format::Aep | Format::Aepx => after_effects(bytes),
        Format::Ktx | Format::Ktx2 | Format::Basis => {
            crate::desktop_textures::render(bytes, format)
        }
        _ => Err(ThumbError::Unsupported),
    };
    if cancel.is_cancelled() {
        return empty(ThumbError::Cancelled);
    }
    result.unwrap_or_else(|error| {
        let mut preview = empty(error.clone());
        if let ThumbError::Failed(reason) = error {
            preview.property("Preview", reason);
        }
        preview
    })
}

fn empty(error: ThumbError) -> StillPreview {
    StillPreview::new(Err(error))
}

fn metadata_only(reason: &'static str) -> StillPreview {
    let mut preview = empty(ThumbError::Unsupported);
    preview.property("Preview", reason);
    preview
}

fn swatches(colors: &[[u8; 3]], edge: u32) -> Result<Image, ThumbError> {
    if colors.is_empty() {
        return Err(ThumbError::Unsupported);
    }
    let columns = (colors.len() as f64).sqrt().ceil() as usize;
    let rows = colors.len().div_ceil(columns);
    let width = edge as usize;
    let height = (width * rows).div_ceil(columns);
    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let i = (y * rows / height) * columns + x * columns / width;
            let [r, g, b] = colors.get(i).copied().unwrap_or([242, 246, 250]);
            data.extend([r, g, b, 255]);
        }
    }
    rgba(edge, height as u32, data)
}

fn palette(bytes: &[u8], edge: u32) -> Result<StillPreview, ThumbError> {
    let text = std::str::from_utf8(bytes).map_err(failed)?;
    let palette = craft::palette::parse_gpl(text).map_err(failed)?;
    let colors: Vec<_> = palette.colors.iter().map(|color| color.rgb).collect();
    let mut preview = StillPreview::new(swatches(&colors, edge));
    preview.property("Palette", palette.name);
    preview.property("Colors", colors.len().to_string());
    Ok(preview)
}

fn material(bytes: &[u8], edge: u32) -> Result<StillPreview, ThumbError> {
    use craft::materialx::{Document, Value};
    let document = Document::parse(bytes).map_err(failed)?;
    let mut preview = metadata_only("Shader graph rendering is unavailable");
    preview.property("Node graphs", document.nodegraphs().count().to_string());
    if let Ok(material) = document.read_material(None) {
        preview.property("Material", material.name);
        preview.property("Shader", format!("{:?}", material.shader));
        preview.property("Resolved inputs", material.inputs.len().to_string());
        let color = material
            .inputs
            .iter()
            .find_map(|(name, value)| match value {
                Value::Constant { values, colorspace }
                    if name == "base_color" && values.len() >= 3 =>
                {
                    let rgb = [values[0], values[1], values[2]];
                    Some(if colorspace.as_deref() == Some("srgb_texture") {
                        rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                    } else {
                        rgb.map(craft::color::linear_to_srgb_u8)
                    })
                }
                _ => None,
            });
        if let Some(color) = color {
            preview.image = swatches(&[color], edge);
            preview.properties.retain(|(key, _)| *key != "Preview");
            preview.property("Preview", "Base-color swatch");
        } else if let Some(path) = material
            .inputs
            .iter()
            .find_map(|(name, value)| match value {
                Value::Image(path) if name == "base_color" => Some(path),
                _ => None,
            })
        {
            preview.properties.retain(|(key, _)| *key != "Preview");
            preview.property(
                "Preview",
                format!("Base color references the texture {path}"),
            );
        }
    }
    Ok(preview)
}

fn lottie(
    bytes: &[u8],
    format: Format,
    edge: u32,
    cancel: &Cancel,
) -> Result<StillPreview, ThumbError> {
    use craft::lottie::parse_document;
    if format == Format::Lottie {
        return lottie_document(bytes, 1, edge, cancel);
    }
    let mut archive = craft::zip::Archive::open(
        bytes,
        craft::zip::Limits {
            max_entries: 4096,
            max_total_bytes: INPUT_BYTES as u64,
        },
    )
    .map_err(failed)?;
    if archive.has_duplicate_names() {
        return Err(failed("The animation package contains duplicate entries"));
    }
    let manifest = archive
        .read("manifest.json", 1 << 20)
        .map_err(failed)?
        .ok_or_else(|| failed("Missing dotLottie manifest"))?;
    let manifest = parse_document(&manifest).map_err(failed)?;
    let entries = manifest
        .get("animations")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| failed("Missing dotLottie animation list"))?;
    let id = entries
        .first()
        .and_then(|entry| entry.get("id"))
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty() && !id.contains(['/', '\\']) && !matches!(*id, "." | ".."))
        .ok_or_else(|| failed("Invalid dotLottie animation ID"))?;
    for directory in ["a", "animations"] {
        if let Some(animation) = archive
            .read(&format!("{directory}/{id}.json"), 8 << 20)
            .map_err(failed)?
        {
            return lottie_document(&animation, entries.len(), edge, cancel);
        }
    }
    // Keep the manifest's useful metadata even when an animation entry is missing.
    let mut preview = metadata_only("The first animation is missing from this package");
    preview.property("Animations", entries.len().to_string());
    Ok(preview)
}

fn lottie_document(
    bytes: &[u8],
    count: usize,
    edge: u32,
    cancel: &Cancel,
) -> Result<StillPreview, ThumbError> {
    if bytes.len() > 8 << 20 {
        return Err(failed("Animation JSON exceeds the preview limit"));
    }
    let document = craft::lottie::parse_document(bytes).map_err(failed)?;
    let header = craft::lottie::Header::read(&document).map_err(failed)?;
    let mut preview = metadata_only("Lottie rendering is unavailable");
    if header.width >= 1.0
        && header.height >= 1.0
        && header.width <= u32::MAX as f64
        && header.height <= u32::MAX as f64
    {
        preview.dimensions = Some(PixelSize::new(header.width as u32, header.height as u32));
    }
    preview.property("Animations", count.to_string());
    preview.property("Frame rate", format!("{:.3} fps", header.frame_rate));
    preview.property(
        "Duration",
        format!(
            "{:.3} s",
            (header.out_point - header.in_point) / header.frame_rate
        ),
    );
    if let Some(name) = header.name {
        preview.property("Animation", name);
    }
    if let Some(layers) = document.get("layers").and_then(serde_json::Value::as_array) {
        preview.property("Layers", layers.len().to_string());
    }
    if let Some(image) = lottie_poster(&document, &header, edge, cancel)? {
        preview.image = Ok(image);
        preview.properties.retain(|(key, _)| *key != "Preview");
        preview.property("Preview", "First frame · supported vector shapes");
    }
    Ok(preview)
}

fn lottie_poster(
    document: &Json,
    header: &craft::lottie::Header<'_>,
    edge: u32,
    cancel: &Cancel,
) -> Result<Option<Image>, ThumbError> {
    let Some(layers) = document["layers"].as_array() else {
        return Ok(None);
    };
    let scale = f64::from(edge) / header.width.max(header.height);
    let width = (header.width * scale).round().max(1.0) as u32;
    let height = (header.height * scale).round().max(1.0) as u32;
    let mut pixmap =
        Pixmap::new(width, height).ok_or_else(|| failed("Invalid animation canvas"))?;
    let frame = header.in_point;
    let mut budget = 2048usize;
    for layer in layers.iter().rev().take(2048) {
        if cancel.is_cancelled() {
            return Err(ThumbError::Cancelled);
        }
        if layer["hd"] == true
            || layer["ddd"].as_i64().is_some_and(|value| value != 0)
            || layer.get("parent").is_some()
            || layer["hasMask"] == true
            || layer["tt"].as_i64().is_some_and(|value| value != 0)
            || layer["ip"].as_f64().is_some_and(|start| frame < start)
            || layer["op"].as_f64().is_some_and(|end| frame >= end)
        {
            continue;
        }
        if let Some(shapes) = layer["shapes"].as_array() {
            let (pose, opacity) = lottie_pose(&layer["ks"], frame);
            let transform = pose.post_scale(scale as f32, scale as f32);
            lottie_shapes(
                &mut pixmap,
                shapes,
                frame,
                transform,
                opacity,
                0,
                &mut budget,
            );
        }
    }
    if !pixmap.data().chunks_exact(4).any(|pixel| pixel[3] != 0) {
        return Ok(None);
    }
    straight_rgba(width, height, pixmap.take()).map(Some)
}

fn lottie_value(value: &Json, frame: f64) -> Option<&Json> {
    match craft::lottie::read_prop(value).value {
        craft::lottie::PropValue::Static(value) => Some(value),
        craft::lottie::PropValue::Keys(keys) => keys
            .iter()
            .rev()
            .find(|key| key.t <= frame)
            .or_else(|| keys.first())
            .and_then(|key| key.s),
        craft::lottie::PropValue::None => None,
    }
}

fn lottie_scalar(value: &Json, frame: f64, fallback: f32) -> f32 {
    lottie_value(value, frame)
        .and_then(|value| value.as_f64().or_else(|| value[0].as_f64()))
        .map_or(fallback, |value| value as f32)
}

fn lottie_pair(value: &Json, frame: f64, fallback: [f32; 2]) -> [f32; 2] {
    lottie_value(value, frame).map_or(fallback, |value| {
        [0, 1].map(|i| value[i].as_f64().map_or(fallback[i], |value| value as f32))
    })
}

fn lottie_pose(pose: &Json, frame: f64) -> (Transform, f32) {
    let [ax, ay] = lottie_pair(&pose["a"], frame, [0.0; 2]);
    let [px, py] = lottie_pair(&pose["p"], frame, [0.0; 2]);
    let [sx, sy] = lottie_pair(&pose["s"], frame, [100.0; 2]);
    let rotation = lottie_scalar(&pose["r"], frame, 0.0);
    let opacity = lottie_scalar(&pose["o"], frame, 100.0).clamp(0.0, 100.0) / 100.0;
    (
        Transform::from_translate(-ax, -ay)
            .post_scale(sx / 100.0, sy / 100.0)
            .post_rotate(rotation)
            .post_translate(px, py),
        opacity,
    )
}

fn lottie_shapes(
    pixmap: &mut Pixmap,
    items: &[Json],
    frame: f64,
    transform: Transform,
    opacity: f32,
    depth: usize,
    budget: &mut usize,
) {
    if depth >= 24 {
        return;
    }
    let mut paths = Vec::new();
    for item in items.iter().filter(|item| item["hd"] != true) {
        let Some(left) = budget.checked_sub(1) else {
            break;
        };
        *budget = left;
        match item["ty"].as_str() {
            Some(kind @ ("el" | "rc")) => {
                let [x, y] = lottie_pair(&item["p"], frame, [0.0; 2]);
                let [w, h] = lottie_pair(&item["s"], frame, [0.0; 2]);
                if let Some(rect) = Rect::from_xywh(x - w / 2.0, y - h / 2.0, w, h) {
                    let mut path = PathBuilder::new();
                    if kind == "el" {
                        path.push_oval(rect);
                    } else {
                        path.push_rect(rect);
                    }
                    paths.extend(path.finish());
                }
            }
            Some("fl") => {
                let Some(rgb) = lottie_value(&item["c"], frame) else {
                    continue;
                };
                let Some(left) = budget.checked_sub(paths.len()) else {
                    break;
                };
                *budget = left;
                let [r, g, b] = [0, 1, 2].map(|i| {
                    (rgb[i].as_f64().unwrap_or(0.0).clamp(0.0, 1.0) * 255.0).round() as u8
                });
                let alpha = lottie_scalar(&item["o"], frame, 100.0).clamp(0.0, 100.0) / 100.0;
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, (alpha * opacity * 255.0).round() as u8);
                let rule = if item["r"] == 2 {
                    FillRule::EvenOdd
                } else {
                    FillRule::Winding
                };
                for path in &paths {
                    pixmap.fill_path(path, &paint, rule, transform, None);
                }
            }
            Some("sh") => {
                if let Some(path) = lottie_value(&item["ks"], frame)
                    .and_then(|value| craft::lottie::Bezier::from_json(value, false))
                    && path.vertices.len() <= *budget
                    && let Some(&[x, y]) = path.vertices.first()
                {
                    *budget -= path.vertices.len();
                    let mut builder = PathBuilder::new();
                    builder.move_to(x as f32, y as f32);
                    let segments = path.vertices.len() - usize::from(!path.closed);
                    for i in 0..segments {
                        let next = (i + 1) % path.vertices.len();
                        let [ax, ay] = path.vertices[i];
                        let [bx, by] = path.vertices[next];
                        let [ox, oy] = path.out_tangents[i];
                        let [ix, iy] = path.in_tangents[next];
                        builder.cubic_to(
                            (ax + ox) as f32,
                            (ay + oy) as f32,
                            (bx + ix) as f32,
                            (by + iy) as f32,
                            bx as f32,
                            by as f32,
                        );
                    }
                    if path.closed {
                        builder.close();
                    }
                    paths.extend(builder.finish());
                }
            }
            Some("gr") => {
                if let Some(children) = item["it"].as_array() {
                    let (pose, alpha) = children
                        .iter()
                        .find(|child| child["ty"] == "tr")
                        .map_or((Transform::identity(), 1.0), |child| {
                            lottie_pose(child, frame)
                        });
                    lottie_shapes(
                        pixmap,
                        children,
                        frame,
                        pose.post_concat(transform),
                        opacity * alpha,
                        depth + 1,
                        budget,
                    );
                }
            }
            _ => {}
        }
    }
}

fn sketch(bytes: &[u8], limits: Limits) -> Result<StillPreview, ThumbError> {
    let archive = craft::sketch::Archive::open(bytes).map_err(failed)?;
    let mut preview = metadata_only("No saved Sketch preview is embedded");
    preview.property("Pages", archive.pages.len().to_string());
    preview.property("Sketch format version", archive.version.to_string());
    if let Some(image) = zip_preview(
        bytes,
        &["previews/preview.png", "preview.png", "thumbnail.png"],
    )? {
        preview.image = embedded(&image, limits);
        if preview.image.is_ok() {
            preview.properties.retain(|(key, _)| *key != "Preview");
        }
    }
    Ok(preview)
}

fn after_effects(bytes: &[u8]) -> Result<StillPreview, ThumbError> {
    let file = craft::aep::parse_any(bytes).map_err(failed)?;
    let mut preview = metadata_only("After Effects composition rendering is unavailable");
    if let Some(project) = craft::aep::Project::new(&file) {
        let mut pending: Vec<_> = project.items().take(25_000).collect();
        let mut items = 0;
        let mut compositions = 0;
        while let Some(item) = pending.pop() {
            items += 1;
            if item.kind() == Some(craft::aep::ItemKind::Composition) {
                compositions += 1;
            }
            if items == 25_000 {
                preview.property("Project items", "At least 25000");
                return Ok(preview);
            }
            pending.extend(item.children().take(25_000 - items - pending.len()));
        }
        preview.property("Project items", items.to_string());
        preview.property("Compositions", compositions.to_string());
    }
    Ok(preview)
}

fn xcf(bytes: &[u8], edge: u32, cancel: &Cancel) -> Result<StillPreview, ThumbError> {
    let mut inflated = Vec::new();
    let bytes = if bytes.starts_with(b"\x1f\x8b") {
        flate2::read::GzDecoder::new(bytes)
            .take(INPUT_BYTES as u64 + 1)
            .read_to_end(&mut inflated)
            .map_err(failed)?;
        if inflated.len() > INPUT_BYTES {
            return Err(failed("Expanded XCF exceeds the preview read limit"));
        }
        &inflated
    } else {
        bytes
    };
    let keep_going = || !cancel.is_cancelled();
    let xcf = craft::xcf::Xcf::parse_with(bytes, &keep_going).map_err(failed)?;
    let image = xcf.image();
    let mut preview = metadata_only("No visible XCF layers could be rendered");
    preview.property("Layers", image.layers.len().to_string());
    preview.property("XCF version", image.version.to_string());
    preview.dimensions = Some(PixelSize::new(image.width, image.height));
    let scale = (f64::from(edge) / f64::from(image.width.max(image.height))).min(1.0);
    let width = (f64::from(image.width) * scale).round().max(1.0) as u32;
    let height = (f64::from(image.height) * scale).round().max(1.0) as u32;
    let mut canvas = vec![[0.0; 4]; (width * height) as usize];
    let mut budget = 96 << 20;
    let hidden_groups: std::collections::HashSet<_> = image
        .layers
        .iter()
        .filter(|layer| layer.group && !layer.visible && !layer.item_path.is_empty())
        .map(|layer| layer.item_path.as_slice())
        .collect();
    let mut approximate = image.icc_profile.is_some();
    let mut drew = false;
    let mut work = 20_000_000usize;
    for (index, layer) in image.layers.iter().enumerate().rev() {
        if !keep_going() {
            return Err(ThumbError::Cancelled);
        }
        if !layer.visible
            || layer.floating_selection
            || (1..layer.item_path.len())
                .any(|depth| hidden_groups.contains(&layer.item_path[..depth]))
        {
            continue;
        }
        if layer.group {
            approximate |= layer.opacity != 1.0 || layer.has_mask();
            continue;
        }
        let Some(left) = work.checked_sub(canvas.len()) else {
            preview.property(
                "Preview detail",
                "Some layers exceeded the compositing budget",
            );
            break;
        };
        work = left;
        let pixels = match xcf.layer_pixels(index, &mut budget, &keep_going) {
            Ok(Some(pixels)) => pixels,
            Ok(None) => continue,
            Err(error) => {
                preview.property("Preview detail", error.to_string());
                break;
            }
        };
        let mask = if layer.apply_mask && layer.has_mask() {
            xcf.layer_mask(index, &mut budget, &keep_going)
                .map_err(failed)?
                .map(|(_, pixels)| pixels)
        } else {
            None
        };
        approximate |= !matches!(layer.mode, 0 | 28) || layer.clipped || layer.effects != 0;
        blend_xcf(
            &mut canvas,
            (width, height),
            scale,
            layer,
            &pixels,
            mask.as_ref(),
        );
        drew = true;
    }
    if drew {
        let mut data = Vec::with_capacity(canvas.len() * 4);
        for [r, g, b, alpha] in canvas {
            let rgb = if alpha > 0.0 {
                [r / alpha, g / alpha, b / alpha]
            } else {
                [0.0; 3]
            };
            if image.precision.linear {
                data.extend(rgb.map(craft::color::linear_to_srgb_u8));
            } else {
                data.extend(rgb.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8));
            }
            data.push((alpha * 255.0).round() as u8);
        }
        preview.image = rgba(width, height, data);
        preview.properties.retain(|(key, _)| *key != "Preview");
        if approximate {
            preview.property("Preview", "Approximate composite of visible layers");
        }
    }
    Ok(preview)
}

fn blend_xcf(
    canvas: &mut [[f32; 4]],
    (width, height): (u32, u32),
    scale: f64,
    layer: &craft::xcf::Layer,
    pixels: &craft::xcf::Pixels,
    mask: Option<&craft::xcf::Pixels>,
) {
    for y in 0..height {
        let sy = (f64::from(y) / scale) as i64 - i64::from(layer.y);
        if !(0..i64::from(pixels.height)).contains(&sy) {
            continue;
        }
        for x in 0..width {
            let sx = (f64::from(x) / scale) as i64 - i64::from(layer.x);
            if !(0..i64::from(pixels.width)).contains(&sx) {
                continue;
            }
            let index = (sy as usize * pixels.width as usize + sx as usize) * pixels.channels;
            let Some(sample) = pixels.data.get(index..index + pixels.channels) else {
                continue;
            };
            let (rgb, alpha) = match sample {
                [g] => ([*g; 3], 1.0),
                [g, a] => ([*g; 3], *a),
                [r, g, b] => ([*r, *g, *b], 1.0),
                [r, g, b, a] => ([*r, *g, *b], *a),
                _ => continue,
            };
            let mask = mask.map_or(1.0, |mask| {
                if sx < i64::from(mask.width) && sy < i64::from(mask.height) {
                    mask.data
                        .get(sy as usize * mask.width as usize + sx as usize)
                        .copied()
                        .unwrap_or(0.0)
                } else {
                    0.0
                }
            });
            let alpha = (alpha * layer.opacity * mask).clamp(0.0, 1.0);
            if !alpha.is_finite() {
                continue;
            }
            let dest = &mut canvas[(y * width + x) as usize];
            for i in 0..3 {
                let src = if rgb[i].is_finite() { rgb[i] } else { 0.0 };
                dest[i] = src * alpha + dest[i] * (1.0 - alpha);
            }
            dest[3] = alpha + dest[3] * (1.0 - alpha);
        }
    }
}
