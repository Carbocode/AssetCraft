use craft::image_codecs::{self, Format as Codec};
use std::io::{Cursor, Read, Seek};
use std::path::Path;

macro_rules! asset_formats {
    (raster { $($variant:ident => $id:literal),+ $(,)? }
     document { $($doc:ident => ($doc_id:literal, $label:literal)),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Format {
            $($variant,)+
            $($doc),+
        }

        impl Format {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+ $(Self::$doc),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $id,)+
                    $(Self::$doc => $doc_id),+
                }
            }

            pub fn codec(self) -> Option<Codec> {
                match self {
                    $(Self::$variant => Some(Codec::$variant),)+
                    $(Self::$doc)|+ => None,
                }
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => Codec::$variant.name(),)+
                    $(Self::$doc => $label),+
                }
            }

            pub(crate) fn from_id(id: &str) -> Option<Self> {
                match id {
                    $($id => Some(Self::$variant),)+
                    $($doc_id => Some(Self::$doc),)+
                    _ => None,
                }
            }

            fn from_codec(codec: Codec) -> Self {
                match codec {
                    $(Codec::$variant => Self::$variant,)+
                }
            }
        }
    };
}

asset_formats! {
    raster {
        Png => "png",
        Jpeg => "jpeg",
        WebP => "webp",
        Tiff => "tiff",
        Bmp => "bmp",
        Gif => "gif",
        Ico => "ico",
        Tga => "tga",
        Qoi => "qoi",
        Dds => "dds",
        OpenExr => "exr",
        Hdr => "hdr",
        Pnm => "pnm",
        Pcx => "pcx",
        Xpm => "xpm",
        Jp2 => "jp2",
        JpegLs => "jpegls",
        Iff => "iff",
        Fits => "fits",
        Dpx => "dpx",
        Cineon => "cineon",
        Dicom => "dicom",
        Avif => "avif",
        Heif => "heif",
        Jxl => "jxl",
        Sgi => "sgi",
        SunRaster => "sunraster",
    }
    document {
        Svg => ("svg", "SVG"),
        Svgz => ("svgz", "SVGZ"),
        Pdf => ("pdf", "PDF"),
        Eps => ("eps", "EPS"),
        Raw => ("raw", "Camera RAW"),
        Ttf => ("ttf", "TrueType"),
        Otf => ("otf", "OpenType"),
        Ttc => ("ttc", "Font collection"),
        Woff => ("woff", "WOFF"),
        Woff2 => ("woff2", "WOFF2"),
        Psd => ("psd", "PSD"),
        Psb => ("psb", "PSB"),
        Ora => ("ora", "OpenRaster"),
        Kra => ("kra", "Krita"),
        Pcraft => ("pcraft", "PhotoCraft"),
        VectorCraft => ("vectorcraft", "VectorCraft"),
        DesignCraft => ("designcraft", "DesignCraft"),
        Mp4 => ("mp4", "MP4"),
        Mov => ("mov", "QuickTime"),
        WebM => ("webm", "WebM"),
        Mkv => ("mkv", "Matroska"),
        Glb => ("glb", "GLB"),
        Gltf => ("gltf", "glTF"),
        Obj => ("obj", "Wavefront OBJ"),
        Stl => ("stl", "STL"),
        Ply => ("ply", "PLY"),
        ThreeMf => ("3mf", "3MF"),
        Fbx => ("fbx", "FBX"),
        Dae => ("dae", "COLLADA"),
        Abc => ("abc", "Alembic"),
        Dxf => ("dxf", "DXF"),
        Dwg => ("dwg", "DWG"),
        Wav => ("wav", "WAV"),
        Aiff => ("aiff", "AIFF"),
        Flac => ("flac", "FLAC"),
        Mp3 => ("mp3", "MP3"),
        M4a => ("m4a", "M4A"),
        Ogg => ("ogg", "Ogg audio"),
        Xcf => ("xcf", "GIMP XCF"),
        Gpl => ("gpl", "GIMP palette"),
        Usd => ("usd", "OpenUSD"),
        MaterialX => ("mtlx", "MaterialX"),
        Avi => ("avi", "AVI"),
        Mxf => ("mxf", "MXF"),
        MpegTs => ("mpegts", "MPEG transport stream"),
        Lottie => ("lottie", "Lottie"),
        DotLottie => ("dotlottie", "dotLottie"),
        Sketch => ("sketch", "Sketch"),
        Aep => ("aep", "After Effects"),
        Aepx => ("aepx", "After Effects XML"),
        Ktx => ("ktx", "KTX"),
        Ktx2 => ("ktx2", "KTX2"),
        Basis => ("basis", "Basis Universal"),
    }
}

impl Format {
    pub fn detect(path: &Path, bytes: &[u8]) -> Option<Self> {
        Self::document(path, bytes)
            .or_else(|| Self::additional(path, bytes))
            .or_else(|| Self::video(bytes))
            .or_else(|| Self::model(path, bytes, bytes.len() as u64))
            .or_else(|| Self::from_prefix(path, bytes, image_codecs::detect(bytes)?))
            .or_else(|| Self::bundle(path, bytes, Cursor::new(bytes)))
    }

    pub fn is_video(self) -> bool {
        matches!(
            self,
            Self::Mp4 | Self::Mov | Self::WebM | Self::Mkv | Self::Avi | Self::Mxf | Self::MpegTs
        )
    }

    pub fn is_audio(self) -> bool {
        matches!(
            self,
            Self::Wav | Self::Aiff | Self::Flac | Self::Mp3 | Self::M4a | Self::Ogg
        )
    }

    pub fn is_model(self) -> bool {
        matches!(
            self,
            Self::Glb
                | Self::Gltf
                | Self::Obj
                | Self::Stl
                | Self::Ply
                | Self::ThreeMf
                | Self::Fbx
                | Self::Dae
                | Self::Abc
                | Self::Usd
        )
    }

    pub fn is_cad(self) -> bool {
        matches!(self, Self::Dxf | Self::Dwg)
    }

    pub(crate) fn additional(path: &Path, head: &[u8]) -> Option<Self> {
        let head = head.get(..craft::sniff::HEAD).unwrap_or(head);
        let name = path.file_name()?.to_str().unwrap_or_default();
        let extension = name.rsplit_once('.').map(|(_, suffix)| suffix);
        let named = |suffix: &str| extension.is_some_and(|ext| ext.eq_ignore_ascii_case(suffix));
        let form = head.get(8..12);
        let riff = head.starts_with(b"RIFF") || head.starts_with(b"RF64");
        if riff && form == Some(b"WAVE") {
            return Some(Self::Wav);
        }
        if riff && form == Some(b"AVI ") {
            return Some(Self::Avi);
        }
        if head.starts_with(b"FORM") && matches!(form, Some(b"AIFF" | b"AIFC")) {
            return Some(Self::Aiff);
        }
        if head.starts_with(b"fLaC") {
            return Some(Self::Flac);
        }
        if head.starts_with(b"OggS")
            && (head.windows(8).any(|part| part == b"OpusHead")
                || head.windows(7).any(|part| part == b"\x01vorbis"))
        {
            return Some(Self::Ogg);
        }
        if named("mp3")
            && (head.starts_with(b"ID3")
                || head.first() == Some(&255)
                    && head
                        .get(1)
                        .is_some_and(|byte| byte & 0xe0 == 0xe0 && byte & 6 != 0))
        {
            return Some(Self::Mp3);
        }
        if head.get(4..8) == Some(b"ftyp") && (named("m4a") || head.get(8..12) == Some(b"M4A ")) {
            return Some(Self::M4a);
        }
        if head.starts_with(b"gimp xcf ")
            || ((name.to_ascii_lowercase().ends_with(".xcf.gz") || named("xcfgz"))
                && head.starts_with(b"\x1f\x8b"))
        {
            return Some(Self::Xcf);
        }
        if head.starts_with(b"PXR-USDC") {
            return Some(Self::Usd);
        }
        if head.starts_with(b"\xabKTX 11\xbb\r\n\x1a\n") {
            return Some(Self::Ktx);
        }
        if head.starts_with(b"\xabKTX 20\xbb\r\n\x1a\n") {
            return Some(Self::Ktx2);
        }
        if named("basis") && head.starts_with(b"sB") && head.len() >= 77 {
            return Some(Self::Basis);
        }
        if head.starts_with(b"RIFX") && form == Some(b"Egg!") {
            return Some(Self::Aep);
        }
        if head.starts_with(b"\x06\x0e\x2b\x34\x02\x05\x01\x01\x0d\x01\x02\x01\x01\x02") {
            return Some(Self::Mxf);
        }
        if [(0, 188), (4, 192), (0, 204)]
            .into_iter()
            .any(|(start, stride)| {
                (0..4).all(|index| head.get(start + index * stride) == Some(&0x47))
            })
        {
            return Some(Self::MpegTs);
        }
        let text = String::from_utf8_lossy(head);
        let text = text.trim_start_matches('\u{feff}').trim_start();
        if text.starts_with("GIMP Palette") {
            return Some(Self::Gpl);
        }
        if text.starts_with("#usda ") {
            return Some(Self::Usd);
        }
        if text.starts_with('<') {
            if text.contains("<materialx") {
                return Some(Self::MaterialX);
            }
            if text.contains("<AfterEffectsProject") {
                return Some(Self::Aepx);
            }
        }
        if text.starts_with('{')
            && (named("json") || named("lottie"))
            && ["\"layers\"", "\"fr\"", "\"ip\"", "\"op\"", "\"w\"", "\"h\""]
                .into_iter()
                .all(|key| text.contains(key))
        {
            return Some(Self::Lottie);
        }
        None
    }

    pub(crate) fn model(path: &Path, head: &[u8], len: u64) -> Option<Self> {
        let extension = path.extension().and_then(|value| value.to_str());
        let named = |ext: &str| extension.is_some_and(|name| name.eq_ignore_ascii_case(ext));
        if head.starts_with(b"glTF") {
            return Some(Self::Glb);
        }
        if head.starts_with(b"Kaydara FBX Binary  \x00\x1a\x00") {
            return Some(Self::Fbx);
        }
        if head.starts_with(b"Ogawa") || (named("abc") && head.starts_with(b"\x89HDF\r\n\x1a\n")) {
            return Some(Self::Abc);
        }
        if head.starts_with(b"AutoCAD Binary DXF\r\n\x1a\0") {
            return Some(Self::Dxf);
        }
        if head.len() > 6 && head.starts_with(b"AC10") && head[4..6].iter().all(u8::is_ascii_digit)
        {
            return Some(Self::Dwg);
        }
        if head.starts_with(b"ply\n") || head.starts_with(b"ply\r") {
            return Some(Self::Ply);
        }
        if named("stl")
            && head
                .get(80..84)
                .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                .is_some_and(|count| 84 + u64::from(u32::from_le_bytes(count)) * 50 <= len)
        {
            return Some(Self::Stl);
        }
        let text = String::from_utf8_lossy(head.get(..craft::sniff::HEAD).unwrap_or(head));
        let text = text.trim_start_matches('\u{feff}').trim_start();
        let mut lines = text.lines().map(str::trim);
        while lines.clone().next() == Some("999") {
            lines.next();
            lines.next();
        }
        let first = lines.next().unwrap_or_default();
        if first
            .get(..5)
            .is_some_and(|word| word.eq_ignore_ascii_case("solid"))
            && (text.contains("facet") || text.contains("endsolid"))
        {
            return Some(Self::Stl);
        }
        if first == "0" && lines.next() == Some("SECTION") && lines.next() == Some("2") {
            return Some(Self::Dxf);
        }
        if named("gltf") && text.starts_with('{') {
            return Some(Self::Gltf);
        }
        if named("obj")
            && text.lines().any(|line| {
                line.split_ascii_whitespace().next().is_some_and(|token| {
                    matches!(token, "v" | "vt" | "vn" | "f" | "o" | "g" | "mtllib")
                })
            })
        {
            return Some(Self::Obj);
        }
        if text.starts_with('<')
            && (text.contains("<COLLADA") || (named("dae") && text.contains(":COLLADA")))
        {
            return Some(Self::Dae);
        }
        if named("fbx") && (first.starts_with("; FBX") || text.contains("FBXHeaderExtension:")) {
            return Some(Self::Fbx);
        }
        None
    }

    pub(crate) fn video(bytes: &[u8]) -> Option<Self> {
        use craft::sniff::Kind;
        match craft::sniff::identify(bytes)? {
            Kind::Mp4 => Some(Self::Mp4),
            Kind::QuickTime => Some(Self::Mov),
            Kind::WebM => Some(Self::WebM),
            Kind::Matroska => Some(Self::Mkv),
            _ => None,
        }
    }

    pub(crate) fn document(path: &Path, bytes: &[u8]) -> Option<Self> {
        use craft::eps::Kind;
        let extension = path.extension().and_then(|value| value.to_str());
        let named = |ext: &str| extension.is_some_and(|name| name.eq_ignore_ascii_case(ext));
        if let Some(signature) = craft::eps::sniff(bytes) {
            return match signature.kind {
                Kind::Pdf => Some(Self::Pdf),
                Kind::Eps => Some(Self::Eps),
                Kind::PostScript => None,
            };
        }
        if let Some(mime) = craft::zip::stored_mimetype(bytes) {
            return match mime {
                b"image/openraster" => Some(Self::Ora),
                b"application/x-krita" => Some(Self::Kra),
                b"application/vnd.designcraft+zip" => Some(Self::DesignCraft),
                _ => None,
            };
        }
        if craft::vectorcraft_format::sniff(bytes) {
            return Some(Self::VectorCraft);
        }
        if craft::svg::sniff::is_svg(bytes) {
            return Some(Self::Svg);
        }
        if named("svgz") && craft::svg::sniff::is_svgz(bytes) {
            return Some(Self::Svgz);
        }
        if craft::raw::probe(bytes).is_some()
            || (image_codecs::detect(bytes) == Some(Codec::Tiff)
                && extension.is_some_and(is_tiff_raw_extension))
        {
            return Some(Self::Raw);
        }
        match bytes {
            [b'8', b'B', b'P', b'S', 0, 1, ..] => Some(Self::Psd),
            [b'8', b'B', b'P', b'S', 0, 2, ..] => Some(Self::Psb),
            bytes if bytes.len() >= 12 => match &bytes[..4] {
                [0, 1, 0, 0] | b"true" => Some(Self::Ttf),
                b"OTTO" => Some(Self::Otf),
                b"ttcf" => Some(Self::Ttc),
                b"wOFF" => Some(Self::Woff),
                b"wOF2" => Some(Self::Woff2),
                _ => None,
            },
            _ => None,
        }
    }

    pub(crate) fn bundle(path: &Path, prefix: &[u8], reader: impl Read + Seek) -> Option<Self> {
        if !prefix.starts_with(b"PK\x03\x04") {
            return None;
        }
        let extension = path.extension()?.to_str()?;
        let (format, entry) = if extension.eq_ignore_ascii_case("pcraft") {
            (Self::Pcraft, "manifest.json")
        } else if extension.eq_ignore_ascii_case("designcraft") {
            (Self::DesignCraft, "document.json")
        } else if extension.eq_ignore_ascii_case("3mf") {
            (Self::ThreeMf, "[Content_Types].xml")
        } else if extension.eq_ignore_ascii_case("sketch") {
            (Self::Sketch, "document.json")
        } else if extension.eq_ignore_ascii_case("lottie") {
            (Self::DotLottie, "manifest.json")
        } else {
            return None;
        };
        let archive = craft::zip::SeekableArchive::open(
            reader,
            craft::zip::SeekableLimits {
                max_entries: 100_000,
                max_metadata_bytes: 16 << 20,
                max_total_bytes: 0,
            },
        )
        .ok()?;
        (!archive.has_duplicate_names()
            && archive.contains(entry)
            && match format {
                Self::ThreeMf => archive.contains("_rels/.rels"),
                Self::Sketch => archive.contains("meta.json"),
                Self::DotLottie => archive.names().any(|name| {
                    (name.starts_with("a/") || name.starts_with("animations/"))
                        && name.ends_with(".json")
                }),
                _ => true,
            })
        .then_some(format)
    }

    pub(crate) fn from_prefix(path: &Path, bytes: &[u8], codec: Codec) -> Option<Self> {
        if codec == Codec::Tga && !bytes.ends_with(b"TRUEVISION-XFILE.\0") {
            // Legacy TGA has no magic. Require the filename to corroborate its plausible header.
            let hinted = path.extension().and_then(|value| value.to_str());
            if hinted.and_then(image_codecs::from_extension) != Some(Codec::Tga) {
                return None;
            }
        }
        Some(Self::from_codec(codec))
    }
}

fn is_tiff_raw_extension(extension: &str) -> bool {
    [
        "dng", "nef", "nrw", "arw", "sr2", "srf", "pef", "srw", "3fr", "fff", "erf", "mef", "iiq",
        "kdc", "dcr", "mos",
    ]
    .iter()
    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}
