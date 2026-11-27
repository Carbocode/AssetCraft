use assetcraft::Format;
use craft::behavior::{
    PixelSize,
    kurbo::{PathEl, Shape},
};
use craft::browse::thumbs::ThumbError;
use craft::font_catalog::{FontOutline, sfnt};
use craft::host::queue::Cancel;
use craft::image_codecs::{self, ChannelLayout, DecodeOptions, Image, Limits};
use craft::svg::tree::{self, draw::tiny_skia, usvg};
use std::{
    io::{Cursor, Read},
    sync::{Arc, OnceLock},
};

#[path = "desktop_font_woff.rs"]
pub(crate) mod woff;

pub const VIEWER_EDGE: u32 = 2048;
const PREVIEW_BYTES: usize = 64 << 20;
const RENDER_BUDGET: u64 = 128 << 20;

pub struct StillPreview {
    pub image: Result<Image, ThumbError>,
    pub dimensions: Option<PixelSize>,
    pub properties: Vec<(&'static str, String)>,
}

impl StillPreview {
    pub fn new(image: Result<Image, ThumbError>) -> Self {
        Self {
            image,
            dimensions: None,
            properties: Vec::new(),
        }
    }

    pub(crate) fn property(&mut self, name: &'static str, value: impl Into<String>) {
        self.properties.push((name, value.into()));
    }
}

pub fn render(
    format: Format,
    bytes: Vec<u8>,
    limits: Limits,
    edge: u32,
    cancel: &Cancel,
) -> StillPreview {
    match format {
        Format::Svg | Format::Svgz => svg(&bytes, edge, cancel).unwrap_or_else(failed_preview),
        Format::Pdf => pdf(bytes, edge, cancel).unwrap_or_else(failed_preview),
        Format::Eps => eps(&bytes, limits).unwrap_or_else(failed_preview),
        Format::Raw => raw(&bytes, limits, edge),
        Format::Ttf | Format::Otf | Format::Ttc => font(&bytes),
        Format::Woff | Format::Woff2 => match woff::decode(&bytes) {
            Some(sfnt) => font(&sfnt),
            None => failed_preview(failed("Invalid or unsupported web font")),
        },
        Format::Psd | Format::Psb => psd(bytes, limits, edge).unwrap_or_else(failed_preview),
        Format::Ora | Format::Kra | Format::Pcraft | Format::DesignCraft | Format::VectorCraft => {
            StillPreview::new(saved_image(format, &bytes, limits))
        }
        Format::Xcf
        | Format::Gpl
        | Format::MaterialX
        | Format::Sketch
        | Format::Lottie
        | Format::DotLottie
        | Format::Aep
        | Format::Aepx
        | Format::Ktx
        | Format::Ktx2
        | Format::Basis => crate::desktop_extra::render(format, &bytes, limits, edge, cancel),
        _ => StillPreview::new(Err(ThumbError::Unsupported)),
    }
}

pub(crate) fn failed(error: impl std::fmt::Display) -> ThumbError {
    ThumbError::Failed(error.to_string())
}

fn failed_preview(error: ThumbError) -> StillPreview {
    StillPreview::new(Err(error))
}

pub(crate) fn embedded(bytes: &[u8], limits: Limits) -> Result<Image, ThumbError> {
    image_codecs::decode_with(
        bytes,
        &DecodeOptions {
            limits,
            ..Default::default()
        },
    )
    .map_err(failed)
}

pub(crate) fn rgba(width: u32, height: u32, data: Vec<u8>) -> Result<Image, ThumbError> {
    Image::from_u8(width, height, ChannelLayout::Rgba, data).map_err(failed)
}

pub(crate) fn cineon_8bit(
    bytes: &[u8],
    limits: Limits,
) -> Option<Result<Image, image_codecs::CodecError>> {
    let word = |offset| {
        Some(u32::from_be_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    if word(0)? != 0x802a_5fd7 || bytes.get(198) != Some(&8) {
        return None;
    }
    let &[interleave, packing, sign, sense] = bytes.get(680..684)? else {
        return None;
    };
    if interleave != 0 || sign != 0 || sense != 0 {
        return None;
    }
    let channels = *bytes.get(193)?;
    let layout = match channels {
        1 => ChannelLayout::Gray,
        3 => ChannelLayout::Rgb,
        _ => return None,
    };
    let (width, height) = (word(200)?, word(204)?);
    for channel in 1..usize::from(channels) {
        let relative = channel * 28;
        if *bytes.get(198 + relative)? != 8
            || word(200 + relative)? != width
            || word(204 + relative)? != height
        {
            return None;
        }
    }
    if let Err(error) = limits.check(width, height, layout, image_codecs::SampleType::U8) {
        return Some(Err(error));
    }
    let offset = word(4)? as usize;
    if offset < 692 {
        return None;
    }
    let row = width as usize * usize::from(channels);
    let stride = row.checked_add(word(684)? as usize)?;
    let data_end = offset.checked_add(stride.checked_mul(height as usize)?)?;
    // Some 8-bit writers retain packing 5 in the header but write tight RGB payloads.
    let tail = word(688)? as usize;
    let packed_bytes =
        matches!(packing, 0..=2) || packing == 5 && data_end.checked_add(tail)? == bytes.len();
    if !packed_bytes {
        return None;
    }
    let data = bytes.get(offset..data_end)?;
    let pixels = data
        .chunks_exact(stride)
        .flat_map(|line| line[..row].iter().copied())
        .collect();
    let orientation = *[1, 2, 4, 3, 5, 6, 8, 7].get(usize::from(bytes[192]))?;
    Some(
        Image::from_u8(width, height, layout, pixels).and_then(|image| image.oriented(orientation)),
    )
}

pub(crate) fn straight_rgba(
    width: u32,
    height: u32,
    mut data: Vec<u8>,
) -> Result<Image, ThumbError> {
    for pixel in data.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if (1..255).contains(&alpha) {
            for color in &mut pixel[..3] {
                *color = ((u32::from(*color) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    rgba(width, height, data)
}

fn svg(bytes: &[u8], edge: u32, cancel: &Cancel) -> Result<StillPreview, ThumbError> {
    static SYSTEM_FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let limits = tree::Limits {
        bytes: 16 << 20,
        depth: 64,
        tree_depth: 64,
        allow_dtd: false,
        xml_nodes: 250_000,
        embedded_bytes: 8 << 20,
        embedded_raster_pixels: 8_000_000,
        ..Default::default()
    };
    let source = tree::text(bytes, &limits).map_err(failed)?;
    let checked = tree::preflight(&source, &limits).map_err(failed)?;
    let images = tree::ValidatedImageResolver::new(limits);
    let fonts = SYSTEM_FONTS.get_or_init(|| {
        let mut database = usvg::fontdb::Database::new();
        database.load_system_fonts();
        Arc::new(database)
    });
    let options = usvg::Options {
        fontdb: Arc::clone(fonts),
        ..Default::default()
    };
    let drawing = images.parse_preflight(checked, options).map_err(failed)?;
    if cancel.is_cancelled() {
        return Err(ThumbError::Cancelled);
    }
    let size = drawing.size();
    let scale = (edge as f32 / size.width().max(size.height())).min(2.0);
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let pixmap = tree::draw::render(
        &drawing,
        tiny_skia::Transform::from_scale(scale, scale),
        width,
        height,
        0,
        RENDER_BUDGET,
    )
    .map_err(|error| failed(format!("SVG render limit: {error:?}")))?;
    let mut preview = StillPreview::new(straight_rgba(width, height, pixmap.take()));
    preview.property(
        "Document size",
        format!("{:.2} × {:.2} px", size.width(), size.height()),
    );
    let usage = images.usage();
    if usage.invalid_raster_images
        + usage.raster_limit_images
        + usage.invalid_svg_images
        + usage.svg_limit_images
        > 0
    {
        preview.property("Preview", "Some embedded images were omitted");
    }
    Ok(preview)
}

fn pdf(bytes: Vec<u8>, edge: u32, cancel: &Cancel) -> Result<StillPreview, ThumbError> {
    use craft::pdf::{self, interpret::InterpreterSettings, raster, syntax};
    let options = pdf::OpenOptions {
        max_input_bytes: PREVIEW_BYTES,
        decode_limits: syntax::DecodeLimits::default()
            .with_max_decoded_stream(32 << 20)
            .with_max_decoded_image(64 << 20)
            .with_max_image_pixels(16_000_000),
    };
    let flag = cancel.clone();
    let cancellation = syntax::Cancellation::new(move || flag.is_cancelled());
    let document =
        pdf::open_with_options_and_cancel(Arc::new(bytes), None, &options, &cancellation).map_err(
            |error| {
                if cancel.is_cancelled() {
                    ThumbError::Cancelled
                } else {
                    failed(error)
                }
            },
        )?;
    let pages = document.pages();
    let mut preview = StillPreview::new(Err(ThumbError::Unsupported));
    preview.property("Pages", pages.len().to_string());
    if let Some(page) = pages.first() {
        let (width, height) = page.render_dimensions();
        preview.property("First page size", format!("{width:.2} × {height:.2} pt"));
        let scale = (edge as f32 / width.max(height)).min(2.0);
        let options = raster::Options {
            scale: [scale; 2],
            background: [255; 4],
            max_coverage: Some(32 << 20),
            ..Default::default()
        };
        let settings = InterpreterSettings {
            preview: true,
            ..Default::default()
        };
        preview.image = raster::render(page, &raster::RenderCache::new(), &settings, &options)
            .map_err(failed)
            .and_then(|rendered| {
                if rendered.diagnostics.content_truncated
                    || rendered.diagnostics.image_decode_failed
                {
                    preview.property("Preview", "Some PDF content was omitted");
                }
                straight_rgba(rendered.width, rendered.height, rendered.bytes().to_vec())
            });
    }
    Ok(preview)
}

fn eps(bytes: &[u8], limits: Limits) -> Result<StillPreview, ThumbError> {
    let sections = craft::eps::sections(bytes).map_err(failed)?;
    let image = if let Some(tiff) = sections.tiff {
        embedded(tiff, limits)
    } else if let Some(bitmap) = craft::eps::epsi(sections.postscript) {
        Image::from_u8(
            bitmap.width,
            bitmap.height,
            ChannelLayout::Gray,
            bitmap.gray,
        )
        .map_err(failed)
    } else {
        eps_rgb_image(sections.postscript, limits).ok_or(ThumbError::Unsupported)
    };
    let mut preview = StillPreview::new(image);
    let comments = craft::eps::Comments::read(sections.postscript);
    if matches!(preview.image, Err(ThumbError::Unsupported)) {
        preview.property(
            "Preview",
            "This EPS has no supported embedded preview or raster payload",
        );
    }
    if let Some([x0, y0, x1, y1]) = comments.hires_bounding_box.or(comments.bounding_box) {
        preview.property(
            "Bounding box",
            format!("{:.2} × {:.2} pt", x1 - x0, y1 - y0),
        );
    }
    Ok(preview)
}

// Extract the uncompressed DirectClass RGB payload written by ImageMagick.
// PostScript operators are never evaluated.
fn eps_rgb_image(postscript: &[u8], limits: Limits) -> Option<Image> {
    const IMAGE: &[u8] = b"\nDisplayImage\n";
    let preamble = &postscript[..postscript.len().min(128 << 10)];
    if !preamble
        .split(|&byte| byte == b'\n')
        .take(24)
        .any(|line| line.trim_ascii() == b"%%Creator: (ImageMagick)")
    {
        return None;
    }
    let start = preamble
        .windows(IMAGE.len())
        .position(|part| part == IMAGE)?
        + IMAGE.len();
    let mut lines = postscript[start..].split(|&byte| byte == b'\n');
    // Translation, scale and text size precede the raster dimensions.
    let mut size = std::str::from_utf8(lines.nth(3)?)
        .ok()?
        .split_ascii_whitespace();
    let width = size.next()?.parse::<u32>().ok()?;
    let height = size.next()?.parse::<u32>().ok()?;
    if size.next().is_some()
        || lines.next()?.trim_ascii() != b"0"
        || lines.next()?.trim_ascii() != b"0"
    {
        return None;
    }
    limits
        .check(
            width,
            height,
            ChannelLayout::Rgb,
            image_codecs::SampleType::U8,
        )
        .ok()?;
    let count = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(3)?;
    if count > (postscript.len() - start) / 2 {
        return None;
    }
    let mut encoded = lines
        .take_while(|line| line.trim_ascii() != b"end")
        .flatten()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace());
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(count).ok()?;
    for _ in 0..count {
        let high = char::from(encoded.next()?).to_digit(16)?;
        let low = char::from(encoded.next()?).to_digit(16)?;
        pixels.push((high * 16 + low) as u8);
    }
    if encoded.next().is_some() {
        return None;
    }
    Image::from_u8(width, height, ChannelLayout::Rgb, pixels).ok()
}

fn raw(bytes: &[u8], limits: Limits, edge: u32) -> StillPreview {
    let image = craft::raw::embedded_preview_covering(bytes, edge)
        .ok_or(ThumbError::Unsupported)
        .and_then(|data| embedded(&data, limits));
    let mut preview = StillPreview::new(image);
    if let Ok(info) = craft::raw::probe_info(bytes) {
        preview.property(
            "Sensor size",
            format!("{} × {} px", info.width, info.height),
        );
        preview.property("Sensor bit depth", info.bits.to_string());
        let model = [info.camera.make, info.camera.model]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        if !model.is_empty() {
            preview.property("Camera", model);
        }
    }
    if let Some(format) = craft::raw::probe(bytes) {
        preview.property("RAW format", format!("{format:?}"));
    }
    preview
}

fn font(bytes: &[u8]) -> StillPreview {
    let faces = sfnt::read_faces_bytes(bytes);
    let Some(face) = faces.first() else {
        return StillPreview::new(Err(ThumbError::Unsupported));
    };
    let mut preview = StillPreview::new(specimen(bytes, face.index));
    preview.property("Font family", face.family.clone());
    preview.property("Font style", face.style.clone());
    preview.property("Font faces", faces.len().to_string());
    preview
}

fn specimen(bytes: &[u8], index: u32) -> Result<Image, ThumbError> {
    let face = FontOutline::from_bytes(bytes, index, &[]).map_err(failed)?;
    let em = f32::from(face.metrics().units_per_em).max(1.0);
    let scale = 90.0 / em;
    let mut pixmap =
        tiny_skia::Pixmap::new(780, 164).ok_or_else(|| failed("Font preview allocation failed"))?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(25, 29, 33, 255);
    let mut left = 28.0;
    let mut drawn = false;
    for character in "AaBb0123漢あ가ЖΩأक".chars() {
        let Some(glyph) = face.glyph_for(character) else {
            continue;
        };
        let Ok(outline) = face.outline(glyph) else {
            continue;
        };
        let bounds = outline.bounding_box();
        let width = (bounds.width() as f32 * scale).max(1.0);
        if left + width >= 752.0 {
            break;
        }
        let mut path = tiny_skia::PathBuilder::new();
        for element in outline.elements() {
            match *element {
                PathEl::MoveTo(p) => path.move_to(p.x as f32, p.y as f32),
                PathEl::LineTo(p) => path.line_to(p.x as f32, p.y as f32),
                PathEl::QuadTo(a, b) => {
                    path.quad_to(a.x as f32, a.y as f32, b.x as f32, b.y as f32)
                }
                PathEl::CurveTo(a, b, c) => path.cubic_to(
                    a.x as f32, a.y as f32, b.x as f32, b.y as f32, c.x as f32, c.y as f32,
                ),
                PathEl::ClosePath => path.close(),
            }
        }
        if let Some(path) = path.finish() {
            let transform = tiny_skia::Transform::from_row(
                scale,
                0.0,
                0.0,
                -scale,
                left - bounds.x0 as f32 * scale,
                119.0,
            );
            pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, None);
            left += width + 15.0;
            drawn = true;
        }
    }
    if !drawn {
        return Err(ThumbError::Unsupported);
    }
    rgba(pixmap.width(), pixmap.height(), pixmap.take())
}

fn psd(bytes: Vec<u8>, limits: Limits, edge: u32) -> Result<StillPreview, ThumbError> {
    let document =
        craft::psd::PsdFile::from_source(craft::psd::source::Source::new(bytes)).map_err(failed)?;
    let width = document.header.width;
    let height = document.header.height;
    let image = if document.has_real_merged_data() == Some(false) {
        Err(ThumbError::Unsupported)
    } else if let Some(preview) = document.merged_preview(edge).map_err(failed)? {
        rgba(preview.width, preview.height, preview.rgba)
    } else if u64::from(width) * u64::from(height) <= limits.max_pixels
        && u64::from(width)
            * u64::from(height)
            * u64::from(document.header.channels)
            * u64::from(document.header.depth.div_ceil(8))
            <= limits.max_alloc
    {
        let preview = document.composite_rgba8().map_err(failed)?;
        rgba(preview.width, preview.height, preview.data)
    } else {
        Err(failed("The merged PSD image exceeds the preview limit"))
    };
    let mut preview = StillPreview::new(image);
    preview.dimensions = Some(PixelSize::new(width, height));
    Ok(preview)
}

fn saved_image(format: Format, bytes: &[u8], limits: Limits) -> Result<Image, ThumbError> {
    let data = match format {
        Format::Ora => zip_preview(bytes, &["mergedimage.png", "Thumbnails/thumbnail.png"])?,
        Format::Kra => zip_preview(bytes, &["mergedimage.png", "preview.png"])?,
        Format::Pcraft => {
            use craft::pcraft_format::{COMPOSITE, FULL_COMPOSITE, THUMB};
            zip_preview(bytes, &[COMPOSITE, FULL_COMPOSITE, THUMB])?
        }
        Format::DesignCraft => craft::designcraft_format::read_preview(Cursor::new(bytes)),
        Format::VectorCraft => vectorcraft(bytes)?,
        _ => None,
    };
    embedded(&data.ok_or(ThumbError::Unsupported)?, limits)
}

pub(crate) fn zip_preview(bytes: &[u8], names: &[&str]) -> Result<Option<Vec<u8>>, ThumbError> {
    let mut archive = craft::zip::Archive::open(
        bytes,
        craft::zip::Limits {
            max_entries: 100_000,
            max_total_bytes: PREVIEW_BYTES as u64,
        },
    )
    .map_err(failed)?;
    if archive.has_duplicate_names() {
        return Err(failed("The document contains duplicate ZIP entries"));
    }
    for name in names {
        if let Some(data) = archive.read(name, PREVIEW_BYTES as u64).map_err(failed)? {
            return Ok(Some(data));
        }
    }
    Ok(None)
}

fn vectorcraft(bytes: &[u8]) -> Result<Option<Vec<u8>>, ThumbError> {
    use craft::vectorcraft_format::{Head, is_compressed, split_tail};
    let mut expanded = Vec::new();
    let bytes = if is_compressed(bytes) {
        flate2::read::GzDecoder::new(bytes)
            .take(PREVIEW_BYTES as u64 + 1)
            .read_to_end(&mut expanded)
            .map_err(failed)?;
        if expanded.len() > PREVIEW_BYTES {
            return Err(failed("VectorCraft document exceeds the preview limit"));
        }
        expanded.as_slice()
    } else {
        bytes
    };
    let (json, tail) = split_tail(bytes);
    let head: Head = serde_json::from_slice(json).map_err(failed)?;
    Ok(head
        .preview
        .and_then(|image| image.bytes_at_most(tail, PREVIEW_BYTES as u64)))
}
