use assetcraft::Format;
use basisu::{DecodeFlags, TargetFormat, Transcoder};
use craft::behavior::PixelSize;
use craft::browse::thumbs::ThumbError;
use craft::image_codecs::Image;

use crate::desktop_documents::{StillPreview, failed, rgba};

const PIXELS: u64 = 12_000_000;

pub fn render(bytes: &[u8], format: Format) -> Result<StillPreview, ThumbError> {
    match format {
        Format::Ktx => ktx(bytes),
        Format::Ktx2 => ktx2(bytes),
        Format::Basis => {
            let decoder = Transcoder::new(bytes).map_err(basis_error)?;
            let level = decoder.image_level_info(0).map_err(basis_error)?;
            let mut preview = texture_info(
                (level.width, level.height),
                decoder.level_count(),
                decoder.layer_count(),
                decoder.face_count(),
            );
            preview.property("Texture encoding", format!("{:?}", decoder.source_format()));
            set_image(&mut preview, transcode(&decoder));
            Ok(preview)
        }
        _ => Err(ThumbError::Unsupported),
    }
}

fn basis_error(error: basisu::Error) -> ThumbError {
    failed(format!("Basis texture: {error:?}"))
}

fn set_image(preview: &mut StillPreview, image: Result<Image, ThumbError>) {
    if let Err(error) = &image {
        let reason = match error {
            ThumbError::Failed(reason) => reason.as_str(),
            _ => "No preview decoder for this texture encoding",
        };
        preview.property("Preview", reason);
    }
    preview.image = image;
}

fn texture_info((width, height): (u32, u32), levels: u32, layers: u32, faces: u32) -> StillPreview {
    let mut preview = StillPreview::new(Err(ThumbError::Unsupported));
    preview.dimensions = Some(PixelSize::new(width, height.max(1)));
    preview.property("Mip levels", levels.max(1).to_string());
    preview.property("Layers", layers.max(1).to_string());
    preview.property("Faces", faces.to_string());
    preview
}

fn checked_pixels(width: u32, height: u32) -> Result<(), ThumbError> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > PIXELS {
        return Err(failed("Texture exceeds the preview pixel limit"));
    }
    Ok(())
}

fn transcode(decoder: &Transcoder<'_>) -> Result<Image, ThumbError> {
    // Preview an existing smaller mip when the base level would exceed the allocation budget.
    for level in 0..decoder.level_count().min(32) {
        let size = decoder.image_level_info(level).map_err(basis_error)?;
        if checked_pixels(size.width, size.height).is_ok() {
            let decoded = decoder
                .transcode(level, TargetFormat::Rgba32, DecodeFlags::NONE)
                .map_err(basis_error)?;
            return rgba(size.width, size.height, decoded);
        }
    }
    Err(failed("Texture exceeds the preview pixel limit"))
}

// OpenGL KTX 1.0 header and the first image of the base level.
fn ktx(bytes: &[u8]) -> Result<StillPreview, ThumbError> {
    let header = bytes
        .get(..64)
        .ok_or_else(|| failed("Truncated KTX header"))?;
    let endian = match &header[12..16] {
        [1, 2, 3, 4] => u32::from_le_bytes,
        [4, 3, 2, 1] => u32::from_be_bytes,
        _ => return Err(failed("Invalid KTX byte order")),
    };
    let field = |offset| endian(std::array::from_fn(|i| header[offset + i]));
    let (width, height) = (field(36), field(40).max(1));
    let mut preview = texture_info((width, height), field(56), field(48), field(52));
    preview.property("OpenGL internal format", format!("0x{:04X}", field(28)));
    let metadata_len = field(60) as usize;
    let base = 64usize
        .checked_add(metadata_len)
        .filter(|&offset| offset <= bytes.len())
        .ok_or_else(|| failed("Invalid KTX metadata length"))?;
    let (flip_x, flip_y) = ktx_orientation(&bytes[64..base], endian);
    let channels = match (field(16), field(20), field(24)) {
        (0x1401, 1, 0x1903) => 1,
        (0x1401, 1, 0x1907) => 3,
        (0x1401, 1, 0x1908 | 0x80e1) => 4,
        _ => 0,
    };
    let image = (|| {
        let size = bytes
            .get(base..)
            .and_then(|tail| tail.get(..4))
            .and_then(|size| size.try_into().ok())
            .map(endian)
            .ok_or_else(|| failed("Truncated KTX level"))? as usize;
        let pixels = bytes[base + 4..]
            .get(..size)
            .ok_or_else(|| failed("Truncated KTX pixels"))?;
        if channels == 0 {
            return bc_image(width, height, pixels, gl_bc(field(28)), flip_x, flip_y);
        }
        raw_image(
            width,
            height,
            pixels,
            Samples {
                channels,
                align: 4,
                bgr: field(24) == 0x80e1,
                flip_x,
                flip_y,
            },
        )
    })();
    set_image(&mut preview, image);
    Ok(preview)
}

fn ktx_orientation(mut data: &[u8], endian: fn([u8; 4]) -> u32) -> (bool, bool) {
    while let Some((length, rest)) = data.split_first_chunk::<4>() {
        let size = endian(*length) as usize;
        let Some(value) = rest.get(..size) else { break };
        if let Some(raw) = value.strip_prefix(b"KTXorientation\0") {
            return (
                raw.windows(3).any(|part| part == b"S=l"),
                raw.windows(3).any(|part| part == b"T=u"),
            );
        }
        let Some(next) = size
            .checked_next_multiple_of(4)
            .and_then(|skip| rest.get(skip..))
        else {
            break;
        };
        data = next;
    }
    (false, false)
}

fn ktx2(bytes: &[u8]) -> Result<StillPreview, ThumbError> {
    use ktx2::Format as Vk;
    let reader = ktx2::Reader::new(bytes).map_err(|e| failed(format!("{e:?}")))?;
    let head = reader.header();
    let width = head.pixel_width;
    let height = head.pixel_height.max(1);
    let mut preview = texture_info(
        (width, height),
        head.level_count,
        head.layer_count,
        head.face_count,
    );
    preview.property("Vulkan format", format!("{:?}", head.format));
    let orientation = reader
        .key_value_data()
        .find_map(|(key, value)| (key == "KTXorientation").then_some(value));
    let flip_x = orientation.and_then(|v| v.first()) == Some(&b'l');
    let flip_y = orientation.and_then(|v| v.get(1)) == Some(&b'u');
    let image = (|| {
        if head.format.is_none() {
            // Reject levels that declare unusually large inflated regions before decoding.
            if reader
                .levels()
                .any(|level| level.uncompressed_byte_length > 96 << 20)
            {
                return Err(failed("KTX2 inflation exceeds the preview limit"));
            }
            let decoder = Transcoder::new(bytes).map_err(basis_error)?;
            preview.property("Texture encoding", format!("{:?}", decoder.source_format()));
            let image = transcode(&decoder)?;
            let (w, h) = image.dimensions();
            return orient_rgba(w, h, image.to_rgba8(), flip_x, flip_y);
        }
        let level = reader
            .levels()
            .next()
            .ok_or_else(|| failed("Missing KTX2 level"))?;
        if head.supercompression_scheme.is_some() {
            return Err(ThumbError::Unsupported);
        }
        let (channels, bgr) = match head.format {
            Some(Vk::R8_UNORM | Vk::R8_SRGB) => (1, false),
            Some(Vk::R8G8B8_UNORM | Vk::R8G8B8_SRGB) => (3, false),
            Some(Vk::B8G8R8_UNORM | Vk::B8G8R8_SRGB) => (3, true),
            Some(Vk::R8G8B8A8_UNORM | Vk::R8G8B8A8_SRGB) => (4, false),
            Some(Vk::B8G8R8A8_UNORM | Vk::B8G8R8A8_SRGB) => (4, true),
            _ => {
                return bc_image(
                    width,
                    height,
                    level.data,
                    head.format.and_then(vk_bc),
                    flip_x,
                    flip_y,
                );
            }
        };
        raw_image(
            width,
            height,
            level.data,
            Samples {
                channels,
                align: 1,
                bgr,
                flip_x,
                flip_y,
            },
        )
    })();
    set_image(&mut preview, image);
    Ok(preview)
}

struct Samples {
    channels: usize,
    align: usize,
    bgr: bool,
    flip_x: bool,
    flip_y: bool,
}

fn raw_image(
    width: u32,
    height: u32,
    pixels: &[u8],
    samples: Samples,
) -> Result<Image, ThumbError> {
    checked_pixels(width, height)?;
    let row = width as usize * samples.channels;
    let stride = row.next_multiple_of(samples.align);
    let length = stride * height as usize;
    let pixels = pixels
        .get(..length)
        .ok_or_else(|| failed("Truncated texture image"))?;
    let mut data = Vec::with_capacity(width as usize * height as usize * 4);
    for stored in pixels.chunks_exact(stride) {
        for pixel in stored[..row].chunks_exact(samples.channels) {
            let (mut rgb, alpha) = match pixel {
                [r] => ([*r; 3], 255),
                [r, g, b] => ([*r, *g, *b], 255),
                [r, g, b, a] => ([*r, *g, *b], *a),
                _ => return Err(ThumbError::Unsupported),
            };
            if samples.bgr {
                rgb.swap(0, 2);
            }
            data.extend(rgb);
            data.push(alpha);
        }
    }
    orient_rgba(width, height, data, samples.flip_x, samples.flip_y)
}

fn orient_rgba(
    width: u32,
    height: u32,
    mut data: Vec<u8>,
    x: bool,
    y: bool,
) -> Result<Image, ThumbError> {
    let row = width as usize * 4;
    if y {
        let (top, bottom) = data.split_at_mut(height as usize / 2 * row);
        for (a, b) in top.chunks_exact_mut(row).zip(bottom.rchunks_exact_mut(row)) {
            a.swap_with_slice(b);
        }
    }
    if x {
        for scanline in data.chunks_exact_mut(row) {
            let (left, right) = scanline.split_at_mut(width as usize / 2 * 4);
            let (left, _) = left.as_chunks_mut::<4>();
            let (right, _) = right.as_chunks_mut::<4>();
            for (a, b) in left.iter_mut().zip(right.iter_mut().rev()) {
                std::mem::swap(a, b);
            }
        }
    }
    rgba(width, height, data)
}

fn gl_bc(format: u32) -> Option<(u32, usize)> {
    Some(match format {
        0x83f0 | 0x83f1 | 0x8c4c | 0x8c4d => (71, 8),
        0x83f2 | 0x8c4e => (74, 16),
        0x83f3 | 0x8c4f => (77, 16),
        0x8dbb => (80, 8),
        0x8dbd => (83, 16),
        0x8e8c | 0x8e8d => (98, 16),
        _ => return None,
    })
}

fn vk_bc(format: ktx2::Format) -> Option<(u32, usize)> {
    use ktx2::Format as Vk;
    Some(match format {
        Vk::BC1_RGB_UNORM_BLOCK
        | Vk::BC1_RGB_SRGB_BLOCK
        | Vk::BC1_RGBA_UNORM_BLOCK
        | Vk::BC1_RGBA_SRGB_BLOCK => (71, 8),
        Vk::BC2_UNORM_BLOCK | Vk::BC2_SRGB_BLOCK => (74, 16),
        Vk::BC3_UNORM_BLOCK | Vk::BC3_SRGB_BLOCK => (77, 16),
        Vk::BC4_UNORM_BLOCK => (80, 8),
        Vk::BC5_UNORM_BLOCK => (83, 16),
        Vk::BC7_UNORM_BLOCK | Vk::BC7_SRGB_BLOCK => (98, 16),
        _ => return None,
    })
}

fn bc_image(
    width: u32,
    height: u32,
    pixels: &[u8],
    encoding: Option<(u32, usize)>,
    x: bool,
    y: bool,
) -> Result<Image, ThumbError> {
    checked_pixels(width, height)?;
    let (dxgi, block_bytes) = encoding.ok_or(ThumbError::Unsupported)?;
    let size = width.div_ceil(4) as usize * height.div_ceil(4) as usize * block_bytes;
    let data = pixels
        .get(..size)
        .ok_or_else(|| failed("Truncated compressed texture"))?;
    // Craft's BC decoders are exposed through DDS. Describe this single face and mip in a DX10 header.
    let mut dds = vec![0; 148];
    dds[..4].copy_from_slice(b"DDS ");
    dds[84..88].copy_from_slice(b"DX10");
    for (offset, value) in [
        (4, 124),
        (8, 0x81007),
        (12, height),
        (16, width),
        (20, size as u32),
        (76, 32),
        (80, 4),
        (108, 0x1000),
        (128, dxgi),
        (132, 3),
        (140, 1),
    ] {
        dds[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    dds.extend_from_slice(data);
    let image = craft::image_codecs::decode_as_with(
        craft::image_codecs::Format::Dds,
        &dds,
        &Default::default(),
    )
    .map_err(failed)?;
    orient_rgba(width, height, image.to_rgba8(), x, y)
}
