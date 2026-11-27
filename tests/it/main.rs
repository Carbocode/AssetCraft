// Generated 1×1 grayscale fixtures with a constant pixel value.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3a, 0x7e, 0x9b,
    0x55, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x70, 0x03, 0x00, 0x00,
    0x48, 0x00, 0x47, 0xe6, 0xdf, 0xdc, 0x2f, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];
const JPEG: &[u8] = &[
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08,
    0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0a, 0x0c, 0x14, 0x0d, 0x0c, 0x0b, 0x0b, 0x0c, 0x19, 0x12,
    0x13, 0x0f, 0x14, 0x1d, 0x1a, 0x1f, 0x1e, 0x1d, 0x1a, 0x1c, 0x1c, 0x20, 0x24, 0x2e, 0x27, 0x20,
    0x22, 0x2c, 0x23, 0x1c, 0x1c, 0x28, 0x37, 0x29, 0x2c, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1f, 0x27,
    0x39, 0x3d, 0x38, 0x32, 0x3c, 0x2e, 0x33, 0x34, 0x32, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x01,
    0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0xff, 0xc4, 0x00, 0x14,
    0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f, 0x00, 0x0a, 0xff, 0xd9,
];

use assetcraft::{AssetView, Catalog, Format};
use craft::image_codecs::{self, ChannelLayout, EncodeOptions, Image};
use futures_executor::block_on;
use sha2::{Digest, Sha256};
use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use std::ffi::{OsStr, OsString};
use std::fs;
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[cfg(feature = "desktop")]
#[path = "../../src/desktop_audio.rs"]
mod desktop_audio;
#[cfg(feature = "desktop")]
#[path = "../../src/desktop_documents.rs"]
mod desktop_documents;
#[cfg(feature = "desktop")]
#[path = "../../src/desktop_extra.rs"]
mod desktop_extra;
#[cfg(feature = "desktop")]
#[path = "../../src/desktop_textures.rs"]
mod desktop_textures;

#[cfg(feature = "desktop")]
#[path = "../../src/desktop_models.rs"]
mod desktop_models;

#[cfg(feature = "desktop")]
#[path = "../../src/desktop_video.rs"]
mod desktop_video;

const CATALOG: &str = "catalog.sqlite";

#[cfg(feature = "desktop")]
#[test]
fn extended_assets_keep_previews_after_migrating_a_version_twelve_catalog() {
    use craft::host::queue::Cancel;

    let fixture = Fixture::new();
    fixture.create();
    fixture.write("existing.png", PNG);
    fixture.ok(&["add", CATALOG, "existing.png"]);
    block_on(fixture.execute_sql("PRAGMA user_version = 12"));
    let palette = b"GIMP Palette\nName: Earth\n25 76 125 Ocean\n83 121 51 Land\n";
    let source = b"#usda 1.0\n( upAxis = \"Y\" )\ndef Mesh \"Panel\" {\n point3f[] points = [(0, 0, 0), (2, 0, 0), (0, 2, 0)]\n int[] faceVertexCounts = [3]\n int[] faceVertexIndices = [0, 1, 2]\n}\n";
    let animation = br#"{"v":"5.7.0","w":96,"h":64,"fr":24,"ip":0,"op":48,"layers":[]}"#;
    let mut ktx = b"\xabKTX 11\xbb\r\n\x1a\n".to_vec();
    for word in [
        0x04030201u32,
        0x1401,
        1,
        0x1907,
        0x8051,
        0x1907,
        2,
        1,
        0,
        0,
        1,
        1,
        0,
        8,
    ] {
        ktx.extend(word.to_le_bytes());
    }
    ktx.extend([240, 20, 18, 22, 230, 15, 0, 0]);
    let mut wav = Vec::new();
    let samples: Vec<_> = (0..8000)
        .flat_map(|index| (if index % 8 < 4 { 16000i16 } else { -16000i16 }).to_le_bytes())
        .collect();
    wav.extend(b"RIFF");
    wav.extend((samples.len() as u32 + 36).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    for field in [16u32, 0x0001_0001, 8000, 16000, 0x0010_0002] {
        wav.extend(field.to_le_bytes());
    }
    wav.extend(b"data");
    wav.extend((samples.len() as u32).to_le_bytes());
    wav.extend(samples);
    for (name, format, content) in [
        ("palette.data", Format::Gpl, palette.as_slice()),
        ("model.usda", Format::Usd, source),
        ("clip.json", Format::Lottie, animation),
        ("texture.ktx", Format::Ktx, &ktx),
        ("recording.wav", Format::Wav, &wav),
    ] {
        fixture.write(name, content);
        fixture.ok(&["add", CATALOG, name]);
        assert!(
            fixture
                .ok(&["find", CATALOG, name])
                .contains(&format!("\t{}\tavailable\t", format.as_str()))
        );
    }
    let cancel = Cancel::new();
    let palette = desktop_extra::render(Format::Gpl, palette, Default::default(), 96, &cancel);
    assert_eq!(&palette.image.unwrap().to_rgba8()[..4], &[25, 76, 125, 255]);
    let usd = desktop_models::render(
        &fixture.0.join("model.usda"),
        Format::Usd,
        source,
        96,
        &cancel,
    );
    assert!(usd.image.is_ok(), "{:?}", usd.properties);
    let animation =
        desktop_extra::render(Format::Lottie, animation, Default::default(), 96, &cancel);
    assert!(
        animation
            .properties
            .contains(&("Duration", "2.000 s".into()))
    );
    let texture = desktop_textures::render(&ktx, Format::Ktx).unwrap();
    assert_eq!(
        texture.image.unwrap().to_rgba8(),
        [240, 20, 18, 255, 22, 230, 15, 255]
    );
    let wav = desktop_audio::render(
        fs::File::open(fixture.0.join("recording.wav")).unwrap(),
        Format::Wav,
        96,
        &cancel,
    );
    assert!(wav.properties.contains(&("Duration", "1.000 s".into())));
    let waveform = wav.image.unwrap().to_rgba8();
    assert!(
        waveform
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel != &waveform[..4])
    );
    assert!(
        fixture
            .ok(&["find", CATALOG, "existing.png"])
            .contains("\tpng\tavailable\t")
    );
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("assetcraft-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_assetcraft-cli"));
        command.current_dir(&self.0);
        command
    }

    fn run<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command().args(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        stdout(self.run(args))
    }

    fn create(&self) -> String {
        self.ok(&["create", CATALOG, "Fixture library"])
    }

    fn write(&self, path: impl AsRef<Path>, bytes: &[u8]) {
        fs::write(self.0.join(path), bytes).unwrap();
    }

    async fn execute_sql(&self, sql: &'static str) {
        let options = SqliteConnectOptions::new().filename(self.0.join(CATALOG));
        let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
        sqlx::raw_sql(sql).execute(&mut connection).await.unwrap();
        connection.close().await.unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn library_and_referenced_images_survive_separate_cli_invocations() {
    let fixture = Fixture::new();
    let created = fixture.create();
    fixture.write("icon%.png", PNG);
    fixture.write("iconX.png", JPEG);
    let added = fixture.ok(&["add", CATALOG, "icon%.png", "iconX.png"]);
    let ids: Vec<_> = added
        .lines()
        .map(|line| line.split('\t').nth(1).unwrap())
        .collect();

    let literal = fixture.ok(&["find", CATALOG, "icon%"]);
    assert!(literal.starts_with(ids[0]));
    assert_eq!(literal.lines().count(), 1);
    assert!(fixture.ok(&["find", CATALOG, "Icon%"]).is_empty());
    assert!(
        fixture
            .ok(&["find", CATALOG, ""])
            .contains("\tjpeg\tavailable\t")
    );
    assert_eq!(fs::read(fixture.0.join("icon%.png")).unwrap(), PNG);
    assert_eq!(fs::read(fixture.0.join("iconX.png")).unwrap(), JPEG);

    block_on(async {
        let mut catalog = Catalog::open(&fixture.0.join(CATALOG)).await.unwrap();
        assert!(created.contains(&catalog.library().id.to_string()));
        assert_eq!(catalog.library().name, "Fixture library");
        let assets = catalog.find("icon").await.unwrap();
        assert_eq!(assets[0].id.to_string(), ids[0]);
        assert_eq!(assets[1].id.to_string(), ids[1]);
        assert_eq!(assets[0].path, fixture.0.join("icon%.png"));
        assert_eq!(assets[1].path, fixture.0.join("iconX.png"));
        assert_eq!(
            assets[0].sha256_at_import,
            <[u8; 32]>::from(Sha256::digest(PNG))
        );
        assert_eq!(
            assets[1].sha256_at_import,
            <[u8; 32]>::from(Sha256::digest(JPEG))
        );
        catalog.close().await.unwrap();
    });
}

#[test]
fn new_raster_formats_are_recognized_persisted_and_reopen_after_import() {
    let fixture = Fixture::new();
    fixture.create();
    let image = Image::from_u8(16, 8, ChannelLayout::Rgb, [30, 150, 220].repeat(16 * 8)).unwrap();
    let formats = [
        (Format::WebP, "webp.png"),
        (Format::Tiff, "tiff.data"),
        (Format::Bmp, "bmp.data"),
        (Format::Gif, "gif.data"),
        (Format::Ico, "ico.data"),
        (Format::Tga, "tga.TGA"),
        (Format::Qoi, "qoi.data"),
        (Format::Dds, "dds.data"),
        (Format::Pnm, "netpbm.data"),
        (Format::Sgi, "sgi.data"),
        (Format::SunRaster, "sun-raster.data"),
        (Format::Jxl, "jpeg-xl.data"),
    ];
    let mut arguments = vec!["add", CATALOG];
    for (format, name) in formats {
        let bytes =
            image_codecs::encode(&image, format.codec().unwrap(), &EncodeOptions::default())
                .unwrap();
        fixture.write(name, &bytes);
        arguments.push(name);
    }
    fixture.ok(&arguments);
    block_on(async {
        let mut catalog = Catalog::open(&fixture.0.join(CATALOG)).await.unwrap();
        let assets = catalog.find("").await.unwrap();
        assert_eq!(assets.len(), formats.len());
        for (format, name) in formats {
            let entry = assets
                .iter()
                .find(|asset| asset.display_name == name)
                .unwrap();
            assert_eq!(entry.format, format);
            let bytes = fs::read(&entry.path).unwrap();
            assert_eq!(
                entry.sha256_at_import,
                <[u8; 32]>::from(Sha256::digest(&bytes))
            );
            #[cfg(feature = "desktop")]
            assert_eq!(
                image_codecs::decode_as(format.codec().unwrap(), &bytes)
                    .unwrap()
                    .dimensions(),
                image.dimensions()
            );
        }
        catalog.close().await.unwrap();
    });
}

#[test]
fn document_assets_reopen_with_their_detected_formats_and_original_hashes() {
    use craft::zip::{Compression, Writer};
    let fixture = Fixture::new();
    fixture.create();
    fixture.write("previous.png", PNG);
    fixture.ok(&["add", CATALOG, "previous.png"]);
    // Version 9 uses the same table and allowed only raster formats in the application.
    block_on(fixture.execute_sql("PRAGMA user_version = 9"));
    let mut ora = Writer::new();
    ora.add("mimetype", b"image/openraster", Compression::Stored)
        .unwrap();
    ora.add("mergedimage.png", PNG, Compression::Stored)
        .unwrap();
    let ora = ora.finish().unwrap();
    let mut photo = Writer::new();
    photo
        .add(
            "manifest.json",
            br#"{"format_version":1,"document":{}}"#,
            Compression::Deflated,
        )
        .unwrap();
    // Leave the central directory outside the import prefix to exercise seekable detection.
    photo.add("thumb.png", PNG, Compression::Stored).unwrap();
    photo
        .add("blobs/test.zst", &[17; 8192], Compression::Stored)
        .unwrap();
    let photo = photo.finish().unwrap();
    let entries: &[(&str, Format, &[u8])] = &[
        ("vector.png", Format::Svg, br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10"/></svg>"#),
        ("page.dat", Format::Pdf, b"%PDF-1.7\n%%EOF\n"),
        ("print.dat", Format::Eps, b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 2 1\n%%BeginPreview: 2 1 1 1\n% 80\n%%EndPreview\n%%EOF\n"),
        ("camera.cr2", Format::Raw, b"II*\0\x10\0\0\0CR\x02\0\0\0\0\0"),
        ("art.ora", Format::Ora, &ora),
        ("art.pcraft", Format::Pcraft, &photo),
        ("art.drawcraft", Format::VectorCraft, br#"{"format":"drawcraft","version":2}"#),
    ];
    let mut args = vec!["add", CATALOG];
    for &(name, _, bytes) in entries {
        fixture.write(name, bytes);
        args.push(name);
    }
    fixture.ok(&args);
    block_on(async {
        let mut catalog = Catalog::open(&fixture.0.join(CATALOG)).await.unwrap();
        let assets = catalog.find("").await.unwrap();
        assert_eq!(assets.len(), entries.len() + 1);
        assert!(
            assets
                .iter()
                .any(|asset| asset.display_name == "previous.png" && asset.format == Format::Png)
        );
        for &(name, format, bytes) in entries {
            let entry = assets
                .iter()
                .find(|asset| asset.display_name == name)
                .unwrap();
            assert_eq!(entry.format, format);
            assert_eq!(
                entry.sha256_at_import,
                <[u8; 32]>::from(Sha256::digest(bytes))
            );
        }
        catalog.close().await.unwrap();
    });
    fixture.write("renamed.pcraft", b"This is not a bundle");
    assert!(
        !fixture
            .run(["add", CATALOG, "renamed.pcraft"])
            .status
            .success()
    );
}

#[cfg(feature = "desktop")]
#[test]
fn vector_previews_render_the_document_size_and_first_pdf_page() {
    use craft::browse::thumbs::ThumbError;
    use craft::host::queue::Cancel;
    use std::io::Write;

    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="192" height="128"><rect width="192" height="128" fill="red"/></svg>"#;
    let vector = desktop_documents::render(
        Format::Svg,
        svg.to_vec(),
        Default::default(),
        desktop_documents::VIEWER_EDGE,
        &Cancel::new(),
    );
    assert!(
        vector
            .properties
            .iter()
            .any(|(label, value)| *label == "Document size" && value.contains("192"))
    );
    let vector = vector.image.unwrap();
    let (width, height) = vector.dimensions();
    assert_eq!(width * 2, height * 3);
    assert!(
        vector
            .to_rgba8()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 192 128] /Resources << >> /Contents 5 0 R >>"
            .to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 192] /Resources << >> >>".to_owned(),
        "<< /Length 25 >>\nstream\n1 0 0 rg 0 0 192 128 re f\nendstream".to_owned(),
    ];
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(pdf, "{} 0 obj\n{}\nendobj", index + 1, object).unwrap();
    }
    let xref = pdf.len();
    writeln!(pdf, "xref\n0 6\n0000000000 65535 f ").unwrap();
    for offset in offsets {
        writeln!(pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(
        pdf,
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF"
    )
    .unwrap();
    let page = desktop_documents::render(
        Format::Pdf,
        pdf,
        Default::default(),
        desktop_documents::VIEWER_EDGE,
        &Cancel::new(),
    );
    assert!(
        page.properties
            .iter()
            .any(|(label, value)| *label == "Pages" && value == "2")
    );
    let page = page.image.unwrap();
    let (width, height) = page.dimensions();
    assert!(width > height);
    let center = (width as usize * (height / 2) as usize + (width / 2) as usize) * 4;
    assert_eq!(&page.to_rgba8()[center..center + 4], &[255, 0, 0, 255]);

    let eps = desktop_documents::render(
        Format::Eps,
        b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 200 100\n".to_vec(),
        Default::default(),
        desktop_documents::VIEWER_EDGE,
        &Cancel::new(),
    );
    assert!(matches!(eps.image, Err(ThumbError::Unsupported)));
    assert!(
        eps.properties
            .iter()
            .any(|(label, _)| *label == "Bounding box")
    );
}

#[cfg(feature = "desktop")]
#[test]
fn video_preview_seeks_past_large_media_and_catalogues_the_container() {
    use craft::host::queue::Cancel;
    use craft::media::isobmff::{
        self, Brand, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions,
    };
    use craft::video::prores::{ChromaFormat, Encoder, Frame, Profile};
    use std::fs::File;
    use std::io::{Cursor, Seek, SeekFrom, Write};

    let fixture = Fixture::new();
    fixture.create();
    block_on(fixture.execute_sql("PRAGMA user_version = 10"));
    let mut frame = Frame::new(64, 32, ChromaFormat::Yuv422, 10, false).unwrap();
    frame.y.fill(320);
    frame.cb.fill(428);
    frame.cr.fill(820);
    let packet = Encoder::new(Profile::Standard, 64, 32)
        .encode(&frame)
        .unwrap();
    let mut writer =
        Mp4Writer::new(Cursor::new(Vec::new()), WriterOptions::new(Brand::Mov)).unwrap();
    let track = writer
        .add_track(TrackConfig::new(
            SampleEntry::prores(isobmff::FourCc(*b"apcn"), 64, 32),
            1000,
        ))
        .unwrap();
    writer
        .write_sample(
            track,
            WriteSample {
                data: &packet,
                duration: 1000,
                composition_offset: 0,
                is_sync: true,
            },
        )
        .unwrap();
    let small = writer.finish().unwrap().into_inner();
    let movie = isobmff::open(small.as_slice()).unwrap();
    let media_end = movie.moov_offset as usize;
    let source = fixture.0.join("large-movie.data");
    let mut file = File::create(&source).unwrap();
    file.write_all(&small[..media_end]).unwrap();
    let padding = 72 * 1024 * 1024u32;
    file.write_all(&padding.to_be_bytes()).unwrap();
    file.write_all(b"free").unwrap();
    file.seek(SeekFrom::Current(i64::from(padding) - 8))
        .unwrap();
    file.write_all(&small[media_end..]).unwrap();
    drop(file);
    assert!(fs::metadata(&source).unwrap().len() > 64 << 20);
    fixture.ok(&["add", CATALOG, "large-movie.data"]);
    assert!(
        fixture
            .ok(&["find", CATALOG, "large-movie"])
            .contains("\tmov\tavailable\t")
    );

    let preview =
        desktop_video::render(File::open(source).unwrap(), Format::Mov, 32, &Cancel::new());
    assert!(
        preview
            .properties
            .iter()
            .any(|(key, value)| *key == "Video codec" && value == "ProRes")
    );
    assert!(
        preview
            .properties
            .iter()
            .any(|(key, value)| *key == "Duration" && value == "0:00:01.000")
    );
    let image = preview.image.unwrap();
    assert_eq!(image.dimensions(), (32, 16));
    let pixels = image.to_rgba8();
    assert!(pixels[0] > pixels[1] && pixels[0] > pixels[2]);
}

#[cfg(feature = "desktop")]
#[test]
fn model_and_drawing_previews_survive_a_version_eleven_catalog_upgrade() {
    use craft::host::queue::Cancel;
    use craft::mesh::{self, Limits};

    let fixture = Fixture::new();
    fixture.create();
    fixture.write("original.png", PNG);
    fixture.ok(&["add", CATALOG, "original.png"]);
    block_on(fixture.execute_sql("PRAGMA user_version = 11"));

    let obj = b"v 0 0 0\nv 2 0 0\nv 0 3 0\nf 1 2 3\n";
    let limits = Limits::default();
    let scene = mesh::obj::read(obj, &limits, &mut mesh::no_external).unwrap();
    let glb = mesh::gltf::write_glb(&scene, &limits).unwrap();
    let stl = mesh::stl::write_binary(&scene, "Original triangle", &limits).unwrap();
    let ply = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nend_header\n0 0 0\n1 0 0\n0 1 0\n";
    let dxf = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n0\n11\n20\n21\n15\n0\nENDSEC\n0\nEOF\n";
    craft::dwg::worker::use_worker(
        PathBuf::from(env!("CARGO_BIN_EXE_assetcraft-desktop")),
        vec![craft::dwg::WORKER_ARG.into()],
    );
    let dwg = craft::dwg::encode_as(dxf, "AC1032").unwrap();
    for (name, format, contents, property) in [
        ("wave.obj", Format::Obj, obj.as_slice(), ("Triangles", "1")),
        ("scene.data", Format::Glb, &glb, ("Triangles", "1")),
        ("print.stl", Format::Stl, &stl, ("Triangles", "1")),
        ("points.data", Format::Ply, ply, ("Vertices", "3")),
        ("plan.data", Format::Dxf, dxf, ("Drawable paths", "1")),
        ("plan.dwg", Format::Dwg, &dwg, ("DWG version", "AC1032")),
    ] {
        fixture.write(name, contents);
        fixture.ok(&["add", CATALOG, name]);
        let listed = fixture.ok(&["find", CATALOG, name]);
        assert!(listed.contains(&format!("\t{}\tavailable\t", format.as_str())));
        let preview =
            desktop_models::render(&fixture.0.join(name), format, contents, 112, &Cancel::new());
        assert!(
            preview
                .properties
                .iter()
                .any(|(key, value)| (*key, value.as_str()) == property),
            "{name}: {:?}",
            preview.properties
        );
        let image = preview
            .image
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let pixels = image.to_rgba8();
        if format.is_model() {
            assert_eq!(&pixels[..4], &[0; 4], "{name}");
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] == 255),
                "{name}"
            );
        }
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel != &pixels[..4]),
            "{name}"
        );
    }
    assert!(
        fixture
            .ok(&["find", CATALOG, "original.png"])
            .contains("\tpng\tavailable\t")
    );
}

#[cfg(feature = "desktop")]
#[test]
fn gltf_preview_loads_only_local_geometry_buffers() {
    use craft::host::queue::Cancel;

    let fixture = Fixture::new();
    fixture.create();
    fs::create_dir(fixture.0.join("models")).unwrap();
    let vertices: Vec<_> = [0.0f32, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    fixture.write("models/geometry.bin", &vertices);
    fixture.write("outside.bin", &vertices);
    let gltf = r#"{"asset":{"version":"2.0"},"buffers":[{"uri":"geometry.bin","byteLength":36}],
        "bufferViews":[{"buffer":0,"byteLength":36}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
        "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;
    let path = "models/local.gltf";
    fixture.write(path, gltf.as_bytes());
    fixture.ok(&["add", CATALOG, path]);
    assert!(
        fixture
            .ok(&["find", CATALOG, "local.gltf"])
            .contains("\tgltf\tavailable\t")
    );
    let path = fixture.0.join(path);
    let render = |text: &str| {
        desktop_models::render(&path, Format::Gltf, text.as_bytes(), 96, &Cancel::new())
    };
    assert!(render(gltf).image.is_ok());
    let blocked = render(&gltf.replace("geometry.bin", "../outside.bin"));
    assert!(blocked.image.is_err());
    assert!(blocked.properties.iter().any(|(key, _)| *key == "Preview"));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("../outside.bin", fixture.0.join("models/link.bin")).unwrap();
        assert!(
            render(&gltf.replace("geometry.bin", "link.bin"))
                .image
                .is_err()
        );
    }
}

#[test]
fn duplicate_locations_and_partial_failures_have_distinct_outcomes() {
    let fixture = Fixture::new();
    fixture.create();
    fixture.write("same.png", PNG);
    fixture.write("forged.png", b"not an image");
    fs::create_dir(fixture.0.join("other")).unwrap();
    fixture.write("other/same.png", PNG);

    let output = fixture.run([
        "add",
        CATALOG,
        "same.png",
        "./same.png",
        "forged.png",
        "other/same.png",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("forged.png"));
    let added = String::from_utf8(output.stdout).unwrap();
    let mut lines = added.lines().map(|line| line.split('\t'));
    let first: Vec<_> = lines.next().unwrap().collect();
    let duplicate: Vec<_> = lines.next().unwrap().collect();
    let other: Vec<_> = lines.next().unwrap().collect();
    assert_eq!(
        (first[0], duplicate[0], other[0]),
        ("added", "existing", "added")
    );
    assert_eq!(first[1], duplicate[1]);
    assert_ne!(first[1], other[1]);
    assert_eq!(fixture.ok(&["find", CATALOG, ""]).lines().count(), 2);
    assert!(fixture.ok(&["find", CATALOG, "forged"]).is_empty());
    fixture.write("same.png", b"now contains something else");
    let repeated = fixture.ok(&["add", CATALOG, "same.png"]);
    assert_eq!(repeated.split('\t').nth(1), Some(first[1]));
}

#[test]
fn native_paths_round_trip_and_missing_originals_remain_searchable() {
    let fixture = Fixture::new();
    fixture.create();
    #[cfg(target_os = "linux")]
    let name = OsString::from_vec(b"unusual-\xff.png".to_vec());
    #[cfg(not(target_os = "linux"))]
    let name = OsString::from("unusual-素材-📷.png");
    fixture.write(&name, PNG);
    let added = stdout(fixture.run([OsStr::new("add"), OsStr::new(CATALOG), &name]));
    let id = added.split('\t').nth(1).unwrap();

    block_on(async {
        let mut catalog = Catalog::open(&fixture.0.join(CATALOG)).await.unwrap();
        let found = catalog.find("unusual").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, fixture.0.join(&name));
        catalog.close().await.unwrap();
    });
    fs::rename(fixture.0.join(&name), fixture.0.join("moved.png")).unwrap();
    let found = fixture.ok(&["find", CATALOG, "unusual"]);
    assert!(found.starts_with(id));
    assert!(found.contains("\tunavailable\t"));
}

#[test]
fn catalog_open_and_create_preserve_unrecognized_or_unsupported_files() {
    let fixture = Fixture::new();
    fixture.write(CATALOG, b"Unrelated data");
    let output = fixture.run(["find", CATALOG, ""]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not an AssetCraft catalog"));
    assert!(
        !fixture
            .run(["create", CATALOG, "Replace me"])
            .status
            .success()
    );
    assert_eq!(
        fs::read(fixture.0.join(CATALOG)).unwrap(),
        b"Unrelated data"
    );

    fs::rename(fixture.0.join(CATALOG), fixture.0.join("untouched")).unwrap();
    fixture.create();
    block_on(fixture.execute_sql("PRAGMA user_version = 14"));
    let expected = fs::read(fixture.0.join(CATALOG)).unwrap();
    let output = fixture.run(["add", CATALOG, "image.png"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("version 14 is unsupported"));
    assert_eq!(fs::read(fixture.0.join(CATALOG)).unwrap(), expected);

    block_on(fixture.execute_sql("PRAGMA user_version = 5"));
    let expected = fs::read(fixture.0.join(CATALOG)).unwrap();
    let output = fixture.run(["find", CATALOG, ""]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("version 5 is unsupported"));
    assert_eq!(fs::read(fixture.0.join(CATALOG)).unwrap(), expected);
}

#[test]
fn point_comments_persist_after_migrating_a_version_six_catalog() {
    let fixture = Fixture::new();
    fixture.write("original.png", PNG);
    let original = fixture.0.join("original.png");
    let baseline = fixture.0.join("baseline.sqlite");
    let asset = Uuid::new_v4();
    let library = Uuid::new_v4();
    block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(&baseline)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
        sqlx::raw_sql(include_str!("../../src/schema.sql"))
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO library (singleton, id, name, created_at_unix_ms) VALUES (1, ?, 'Previous library', 0)")
            .bind(library.as_bytes().as_slice()).execute(&mut connection).await.unwrap();
        sqlx::query("INSERT INTO asset VALUES (?, ?, 'original.png', 'png', ?, 0)")
            .bind(asset.as_bytes().as_slice())
            .bind(assetcraft::local_file::encode_path(&original))
            .bind(Sha256::digest(PNG).as_slice())
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO asset_details (asset_id, notes) VALUES (?, 'Existing note')")
            .bind(asset.as_bytes().as_slice())
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
    });
    let baseline_bytes = fs::read(&baseline).unwrap();
    fs::copy(&baseline, fixture.0.join(CATALOG)).unwrap();
    let asset_id = asset.to_string();
    assert!(fixture.ok(&["comments", CATALOG, &asset_id]).is_empty());
    let result = fixture.run(["comment-add", CATALOG, &asset_id, "NaN", "0.5", "Invalid"]);
    assert!(!result.status.success());
    let result = fixture.run(["comment-add", CATALOG, &asset_id, "0.5", "0.5", "   "]);
    assert!(!result.status.success());
    fixture.ok(&[
        "comment-add",
        CATALOG,
        &asset_id,
        "0.25",
        "0.75",
        "  Check this point  ",
    ]);
    assert!(
        fixture
            .ok(&["comments", CATALOG, &asset_id])
            .contains("0.25\t0.75\t\"Check this point\"")
    );
    assert_eq!(fs::read(&baseline).unwrap(), baseline_bytes);
    assert_eq!(fs::read(original).unwrap(), PNG);
    block_on(async {
        let mut catalog = Catalog::open(&fixture.0.join(CATALOG)).await.unwrap();
        assert_eq!(catalog.library().id, library);
        assert_eq!(catalog.find("").await.unwrap()[0].id, asset);
        assert_eq!(
            catalog.organization().await.unwrap().assets[&asset].notes,
            "Existing note"
        );
        assert_eq!(catalog.point_comments(asset).await.unwrap().len(), 1);
        catalog.close().await.unwrap();
    });
}

#[test]
fn point_comment_edits_keep_text_and_position_independent_and_require_the_owner() {
    let fixture = Fixture::new();
    fixture.create();
    fixture.write("first.png", PNG);
    fixture.write("second.jpg", JPEG);
    let first = fixture.ok(&["add", CATALOG, "first.png"]);
    let second = fixture.ok(&["add", CATALOG, "second.jpg"]);
    let first = first.split('\t').nth(1).unwrap();
    let second = second.split('\t').nth(1).unwrap();

    let added = fixture.ok(&["comment-add", CATALOG, first, "0.25", "0.75", "Original"]);
    let id = added.trim().split('\t').nth(1).unwrap();
    fixture.ok(&["comment-edit", CATALOG, first, id, "Edited"]);
    let read = fixture.ok(&["comments", CATALOG, first]);
    assert_eq!(read.trim(), format!("{id}\t0.25\t0.75\t\"Edited\""));

    fixture.ok(&["comment-move", CATALOG, first, id, "0.8", "0.2"]);
    let moved = fixture.ok(&["comments", CATALOG, first]);
    assert_eq!(moved.trim(), format!("{id}\t0.8\t0.2\t\"Edited\""));

    for command in [
        vec!["comment-edit", CATALOG, second, id, "Wrong asset"],
        vec!["comment-edit", CATALOG, first, id, "   "],
        vec!["comment-move", CATALOG, first, id, "1.5", "0.2"],
        vec!["comment-move", CATALOG, second, id, "0.4", "0.4"],
        vec!["comment-delete", CATALOG, second, id],
    ] {
        assert!(!fixture.run(command).status.success());
    }
    assert_eq!(fixture.ok(&["comments", CATALOG, first]), moved);
    assert!(fixture.ok(&["comments", CATALOG, second]).is_empty());

    fixture.ok(&["comment-delete", CATALOG, first, id]);
    assert!(fixture.ok(&["comments", CATALOG, first]).is_empty());
    assert!(
        !fixture
            .run(["comment-delete", CATALOG, first, id])
            .status
            .success()
    );
}

#[test]
fn migrating_version_eight_preserves_asset_associations_and_delete_cascades() {
    let fixture = Fixture::new();
    let baseline = fixture.0.join("version8.sqlite");
    let original = fixture.0.join("legacy.png");
    fixture.write("legacy.png", PNG);
    let asset = Uuid::new_v4();
    let folder = Uuid::new_v4();
    block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(&baseline)
            .create_if_missing(true);
        let mut db = SqliteConnection::connect_with(&options).await.unwrap();
        for schema in [
            include_str!("../../src/schema.sql"),
            include_str!("../../src/schema_7.sql"),
            include_str!("../../src/schema_8.sql"),
        ] {
            sqlx::raw_sql(schema).execute(&mut db).await.unwrap();
        }
        sqlx::query("INSERT INTO library VALUES (1, ?, 'Legacy catalog', 0, 'folder', NULL)")
            .bind(Uuid::new_v4().as_bytes().as_slice())
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO asset VALUES (?, ?, 'legacy.png', 'png', ?, 0)")
            .bind(asset.as_bytes().as_slice())
            .bind(assetcraft::local_file::encode_path(&original))
            .bind(Sha256::digest(PNG).as_slice())
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO folder (id, name) VALUES (?, 'Textures')")
            .bind(folder.as_bytes().as_slice())
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO folder_asset VALUES (?, ?)")
            .bind(folder.as_bytes().as_slice())
            .bind(asset.as_bytes().as_slice())
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO library_tag VALUES ('Approved')")
            .execute(&mut db)
            .await
            .unwrap();
        for query in [
            "INSERT INTO asset_state VALUES (?, 1, 1)",
            "INSERT INTO asset_details VALUES (?, 4, 'Keep this')",
            "INSERT INTO asset_tag VALUES (?, 'Approved')",
            "INSERT INTO asset_comment VALUES (42, ?, 0.25, 0.75, 'Existing comment')",
        ] {
            sqlx::query(query)
                .bind(asset.as_bytes().as_slice())
                .execute(&mut db)
                .await
                .unwrap();
        }
        db.close().await.unwrap();
    });
    let baseline_bytes = fs::read(&baseline).unwrap();
    let upgraded = fixture.0.join(CATALOG);
    fs::copy(&baseline, &upgraded).unwrap();
    block_on(async {
        let mut catalog = Catalog::open(&upgraded).await.unwrap();
        let organization = catalog.organization().await.unwrap();
        let details = &organization.assets[&asset];
        assert_eq!(details.folders, [folder]);
        assert_eq!(details.tags, ["Approved"]);
        assert_eq!(details.rating, 4);
        assert_eq!(details.notes, "Keep this");
        let trash = catalog
            .find_filtered(AssetView::Trash, "", None, false)
            .await
            .unwrap();
        assert_eq!(trash[0].0.id, asset);
        assert!(trash[0].1.favorite);
        assert_eq!(catalog.point_comments(asset).await.unwrap()[0].id, 42);
        catalog.close().await.unwrap();

        let mut db = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(upgraded)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let version: u32 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut db)
            .await
            .unwrap();
        assert_eq!(version, 13);
        sqlx::query("DELETE FROM asset WHERE id = ?")
            .bind(asset.as_bytes().as_slice())
            .execute(&mut db)
            .await
            .unwrap();
        let rows: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM folder_asset) + (SELECT count(*) FROM asset_state)
             + (SELECT count(*) FROM asset_details) + (SELECT count(*) FROM asset_tag)
             + (SELECT count(*) FROM asset_comment)",
        )
        .fetch_one(&mut db)
        .await
        .unwrap();
        assert_eq!(rows, 0);
        db.close().await.unwrap();
    });
    assert_eq!(fs::read(baseline).unwrap(), baseline_bytes);
    assert_eq!(fs::read(original).unwrap(), PNG);
}

#[test]
fn migrating_version_seven_preserves_comments_and_retires_deleted_ids() {
    let fixture = Fixture::new();
    fixture.create();
    fixture.write("first.png", PNG);
    let added = fixture.ok(&["add", CATALOG, "first.png"]);
    let asset = added.split('\t').nth(1).unwrap();

    // Rebuild only the comment table from the original version 7 definition.
    block_on(fixture.execute_sql("DROP TABLE asset_comment"));
    block_on(fixture.execute_sql(include_str!("../../src/schema_7.sql")));
    let path = fixture.0.join(CATALOG);
    block_on(async {
        let mut legacy =
            SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path))
                .await
                .unwrap();
        sqlx::query("INSERT INTO asset_comment VALUES (42, ?, 0.25, 0.75, 'Existing comment')")
            .bind(Uuid::parse_str(asset).unwrap().as_bytes().as_slice())
            .execute(&mut legacy)
            .await
            .unwrap();
        legacy.close().await.unwrap();
    });
    assert_eq!(
        fixture.ok(&["comments", CATALOG, asset]).trim(),
        "42\t0.25\t0.75\t\"Existing comment\""
    );
    fixture.ok(&["comment-delete", CATALOG, asset, "42"]);
    let added = fixture.ok(&["comment-add", CATALOG, asset, "0.5", "0.5", "Replacement"]);
    assert!(
        added
            .trim()
            .split('\t')
            .nth(1)
            .unwrap()
            .parse::<i64>()
            .unwrap()
            > 42
    );
    assert!(
        !fixture
            .run(["comment-delete", CATALOG, asset, "42"])
            .status
            .success()
    );
    assert!(
        fixture
            .ok(&["comments", CATALOG, asset])
            .contains("Replacement")
    );
}

#[test]
fn comment_write_reports_a_blocked_commit() {
    let fixture = Fixture::new();
    fixture.create();
    fixture.write("first.png", PNG);
    let added = fixture.ok(&["add", CATALOG, "first.png"]);
    let asset = Uuid::parse_str(added.split('\t').nth(1).unwrap()).unwrap();

    block_on(async {
        let path = fixture.0.join(CATALOG);
        let mut writer = Catalog::open(&path).await.unwrap();
        let mut reader =
            SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path))
                .await
                .unwrap();
        let mut transaction = reader.begin().await.unwrap();
        sqlx::query("SELECT id FROM asset_comment")
            .fetch_all(&mut *transaction)
            .await
            .unwrap();

        let result = writer
            .add_point_comment(asset, [0.25, 0.75], "Blocked")
            .await;
        transaction.rollback().await.unwrap();
        assert!(
            matches!(
                result,
                Err(assetcraft::Error::Database(sqlx::Error::Database(ref error)))
                    if error.code().as_deref() == Some("5")
            ),
            "{result:?}"
        );
        assert!(writer.point_comments(asset).await.unwrap().is_empty());

        writer
            .add_point_comment(asset, [0.25, 0.75], "Retry")
            .await
            .unwrap();
        assert_eq!(writer.point_comments(asset).await.unwrap()[0].body, "Retry");
    });
}

#[test]
fn catalog_creation_preserves_existing_sidecars() {
    let fixture = Fixture::new();
    for suffix in ["journal", "wal", "shm"] {
        let sidecar = format!("{CATALOG}-{suffix}");
        fixture.write(&sidecar, b"existing file");
        let output = fixture.run(["create", CATALOG, "Library"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("SQLite sidecar already exists"));
        let sidecar = fixture.0.join(sidecar);
        assert_eq!(fs::read(&sidecar).unwrap(), b"existing file");
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
        fs::remove_file(sidecar).unwrap();
    }
    fixture.create();
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
}

#[test]
fn concurrent_catalog_creation_keeps_the_winner() {
    let fixture = Fixture::new();
    let path = fixture.0.join(CATALOG);
    let barrier = std::sync::Barrier::new(2);
    let results = thread::scope(|scope| {
        ["First library", "Second library"]
            .map(|name| {
                let path = &path;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    block_on(async {
                        let catalog = Catalog::create(path, name).await?;
                        catalog.close().await?;
                        Ok::<_, assetcraft::Error>(name)
                    })
                })
            })
            .map(|thread| thread.join().unwrap())
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let winner = results.into_iter().find_map(Result::ok).unwrap();
    block_on(async {
        let catalog = Catalog::open(&path).await.unwrap();
        assert_eq!(catalog.library().name, winner);
        catalog.close().await.unwrap();
    });
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
}

#[test]
fn killing_a_cli_batch_during_a_write_preserves_committed_assets() {
    let fixture = Fixture::new();
    fixture.create();
    fixture.write("kept.png", PNG);
    fixture.write("first.png", PNG);
    fixture.write("slow.jpg", JPEG);
    fixture.ok(&["add", CATALOG, "kept.png"]);
    let previous = fixture.ok(&["find", CATALOG, ""]);
    // Keep the transaction active long enough to observe its rollback journal.
    block_on(fixture.execute_sql(
        "CREATE TRIGGER delay_batch BEFORE INSERT ON asset
         WHEN NEW.display_name = 'slow.jpg'
         BEGIN
            SELECT sum(n) FROM (
                WITH RECURSIVE ticks(n) AS (VALUES(0) UNION ALL
                    SELECT n + 1 FROM ticks WHERE n < 500000000)
                SELECT n FROM ticks
            );
         END;",
    ));

    let mut child = fixture
        .command()
        .args(["add", CATALOG, "first.png", "slow.jpg"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let journal = fixture.0.join("catalog.sqlite-journal");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !journal.is_file() && Instant::now() < deadline && child.try_wait().unwrap().is_none() {
        thread::sleep(Duration::from_millis(10));
    }
    let reached_write = journal.is_file();
    let _ = child.kill();
    let output = child.wait_with_output().unwrap();
    assert!(reached_write, "{}", String::from_utf8_lossy(&output.stderr));
    #[cfg(unix)]
    assert_eq!(output.status.signal(), Some(9));
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fixture.ok(&["find", CATALOG, ""]), previous);
}

#[test]
fn a_failed_second_insert_rolls_back_a_completed_first_insert() {
    let fixture = Fixture::new();
    fixture.create();
    fixture.write("first.png", PNG);
    fixture.write("second.jpg", JPEG);
    block_on(fixture.execute_sql(
        "CREATE TRIGGER reject_second BEFORE INSERT ON asset
         WHEN NEW.display_name = 'second.jpg'
           AND EXISTS (SELECT 1 FROM asset WHERE display_name = 'first.png')
         BEGIN
            SELECT RAISE(ABORT, 'second insert rejected after first');
         END;",
    ));

    let output = fixture.run(["add", CATALOG, "first.png", "second.jpg"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("second insert rejected after first"));
    assert!(fixture.ok(&["find", CATALOG, ""]).is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn an_invalid_catalog_directory_leaves_no_destination_file() {
    let fixture = Fixture::new();
    let directory = fixture.0.join(OsString::from_vec(b"catalog-\xff".to_vec()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join(CATALOG);
    assert!(block_on(Catalog::create(&path, "Library")).is_err());
    assert!(fs::read_dir(directory).unwrap().next().is_none());
}

#[test]
fn catalog_preserves_assets_and_persists_nested_folders() {
    use assetcraft::{Added, AssetView, Error};

    let fixture = Fixture::new();
    fixture.create();
    fixture.write("existing.png", PNG);
    fixture.write("new.png", PNG);
    let original = fixture.0.join("existing.png");
    let added = fixture.ok(&["add", CATALOG, "existing.png"]);
    let old_id = added.split('\t').nth(1).unwrap();

    block_on(async {
        let db = fixture.0.join(CATALOG);
        let mut catalog = Catalog::open(&db).await.unwrap();
        let assets = catalog.find("").await.unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].id.to_string(), old_id);
        let id = assets[0].id;

        let root = catalog.create_folder(None, "Concepts").await.unwrap();
        let child = catalog
            .create_folder(Some(root), "References")
            .await
            .unwrap();
        assert!(matches!(
            catalog.create_folder(Some(root), " References ").await,
            Err(Error::FolderNameExists)
        ));
        assert!(matches!(
            catalog.create_folder(None, "Concepts").await,
            Err(Error::FolderNameExists)
        ));
        assert!(
            catalog
                .create_folder(Some(Uuid::new_v4()), "Orphan")
                .await
                .is_err()
        );
        let new = fixture.0.join("new.png");
        let outcomes = catalog
            .add_to_folder(&[original.clone(), new], Some(child))
            .await
            .unwrap();
        assert!(matches!(&outcomes[0], Added::Existing(asset) if asset.id == id));
        assert!(matches!(&outcomes[1], Added::Imported(_)));
        catalog.set_favorite(id, true).await.unwrap();
        catalog.set_trashed(id, true).await.unwrap();
        catalog.close().await.unwrap();

        let mut reopened = Catalog::open(&db).await.unwrap();
        let folders = reopened.folders().await.unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(
            folders
                .iter()
                .find(|folder| folder.id == child)
                .unwrap()
                .parent,
            Some(root)
        );
        assert_eq!(
            reopened
                .find_in(AssetView::Folder(child), "")
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            reopened
                .find_in(AssetView::Favorites, "")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            reopened
                .find_in(AssetView::Trash, "existing")
                .await
                .unwrap()[0]
                .0
                .id,
            id
        );
        reopened.set_trashed(id, false).await.unwrap();
        assert_eq!(
            reopened.find_in(AssetView::Favorites, "").await.unwrap()[0]
                .0
                .id,
            id
        );
        assert_eq!(
            reopened.find_in(AssetView::Recent, "").await.unwrap().len(),
            2
        );
        assert_eq!(
            reopened
                .find_in(AssetView::Untagged, "")
                .await
                .unwrap()
                .len(),
            2
        );
        let linked = reopened
            .find_in(AssetView::Folder(child), "existing")
            .await
            .unwrap();
        assert_eq!(linked[0].0.path, original);
        assert_eq!(fs::read(&original).unwrap(), PNG);
        reopened.close().await.unwrap();
    });
}

#[test]
fn folder_appearance_survives_reopen() {
    use assetcraft::FolderAppearance;

    let fixture = Fixture::new();
    fixture.create();
    let db = fixture.0.join(CATALOG);
    block_on(async {
        let mut catalog = Catalog::open(&db).await.unwrap();
        let parent = catalog
            .create_folder(None, "Existing folder")
            .await
            .unwrap();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&db).await.unwrap();
        let initial = catalog.folders().await.unwrap();
        assert_eq!(initial[0].id, parent);
        assert_eq!(initial[0].appearance, FolderAppearance::default());

        let blue_star = FolderAppearance {
            icon: "star".to_owned(),
            color: Some("blue".to_owned()),
        };
        let child = catalog
            .create_folder_with_appearance(Some(parent), "Child", &blue_star)
            .await
            .unwrap();
        let image = FolderAppearance {
            icon: "image".to_owned(),
            color: None,
        };
        catalog.set_folder_appearance(parent, &image).await.unwrap();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&db).await.unwrap();
        let folders = catalog.folders().await.unwrap();
        let child = folders.iter().find(|folder| folder.id == child).unwrap();
        assert_eq!(child.parent, Some(parent));
        assert_eq!(child.appearance, blue_star);
        assert_eq!(
            folders
                .iter()
                .find(|folder| folder.id == parent)
                .unwrap()
                .appearance,
            image
        );
        catalog.close().await.unwrap();
    });
}

#[test]
fn library_appearance_survives_reopen() {
    use assetcraft::Appearance;

    let fixture = Fixture::new();
    fixture.create();
    let db = fixture.0.join(CATALOG);
    block_on(async {
        let catalog = Catalog::open(&db).await.unwrap();
        let original = catalog.library().clone();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&db).await.unwrap();
        assert_eq!(catalog.library(), &original);
        let look = Appearance {
            icon: "star".to_owned(),
            color: Some("blue".to_owned()),
        };
        catalog.set_library_appearance(look.clone()).await.unwrap();
        assert_eq!(catalog.library().appearance, look);
        catalog.close().await.unwrap();

        let reopened = Catalog::open(&db).await.unwrap();
        assert_eq!(reopened.library().id, original.id);
        assert_eq!(reopened.library().appearance, look);
        reopened.close().await.unwrap();

        let new_path = fixture.0.join("custom.sqlite");
        Catalog::create_with_appearance(&new_path, "Custom", &look)
            .await
            .unwrap()
            .close()
            .await
            .unwrap();
        let created = Catalog::open(&new_path).await.unwrap();
        assert_eq!(created.library().appearance, look);
        created.close().await.unwrap();
    });
}

#[test]
fn organization_survives_reopen_and_folder_deletion_preserves_assets() {
    use assetcraft::{AssetView, MetadataEdit};

    let fixture = Fixture::new();
    fixture.create();
    fixture.write("one.png", PNG);
    fixture.write("two.png", PNG);
    let path = fixture.0.join(CATALOG);
    block_on(async {
        let mut catalog = Catalog::open(&path).await.unwrap();
        let first = catalog.create_folder(None, "References").await.unwrap();
        let child = catalog
            .create_folder(Some(first), "Materials")
            .await
            .unwrap();
        let second = catalog.create_folder(None, "Collection").await.unwrap();
        catalog
            .add_to_folder(
                &[fixture.0.join("one.png"), fixture.0.join("two.png")],
                Some(child),
            )
            .await
            .unwrap();
        let assets = catalog.find("").await.unwrap();
        let ids: Vec<_> = assets.iter().map(|asset| asset.id).collect();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&path).await.unwrap();
        catalog.create_tag("Color study").await.unwrap();
        catalog.create_tag("Unused").await.unwrap();
        assert!(catalog.create_tag(" Color study ").await.is_err());
        assert!(
            catalog
                .edit_metadata(
                    &ids,
                    &MetadataEdit::Tags {
                        add: vec!["Uncreated tag".to_owned()],
                        remove: Vec::new()
                    }
                )
                .await
                .is_err()
        );
        catalog
            .edit_metadata(
                &ids,
                &MetadataEdit::Folder {
                    id: second,
                    linked: true,
                },
            )
            .await
            .unwrap();
        catalog
            .edit_metadata(
                &ids,
                &MetadataEdit::Tags {
                    add: vec![" Color study ".to_owned(), "Color study".to_owned()],
                    remove: vec![],
                },
            )
            .await
            .unwrap();
        catalog
            .edit_metadata(
                &ids[..1],
                &MetadataEdit::Notes("A note\non two lines".to_owned()),
            )
            .await
            .unwrap();
        catalog
            .edit_metadata(&ids, &MetadataEdit::Rating(4))
            .await
            .unwrap();
        assert!(
            catalog
                .edit_metadata(&ids, &MetadataEdit::Rating(6))
                .await
                .is_err()
        );
        catalog.set_trashed(ids[1], true).await.unwrap();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&path).await.unwrap();
        let organization = catalog.organization().await.unwrap();
        assert_eq!(organization.counts[&AssetView::All], 1);
        assert_eq!(organization.counts[&AssetView::Trash], 1);
        assert_eq!(organization.counts[&AssetView::Untagged], 0);
        assert_eq!(organization.counts[&AssetView::Folder(child)], 1);
        assert_eq!(organization.counts[&AssetView::Folder(second)], 1);
        assert_eq!(organization.tags["Color study"], 1);
        assert_eq!(organization.tags["Unused"], 0);
        assert!(!organization.tags.contains_key("Uncreated tag"));
        assert_eq!(
            catalog
                .find_filtered(AssetView::Folder(first), "", Some("Color study"), true)
                .await
                .unwrap()
                .iter()
                .map(|(asset, _)| asset.id)
                .collect::<Vec<_>>(),
            ids[..1]
        );
        assert!(
            catalog
                .find_filtered(AssetView::Folder(first), "", Some("Color study"), false)
                .await
                .unwrap()
                .is_empty()
        );
        let first_asset = &organization.assets[&ids[0]];
        assert_eq!(first_asset.folders.len(), 2);
        assert_eq!(first_asset.rating, 4);
        assert_eq!(first_asset.notes, "A note\non two lines");
        assert_eq!(catalog.find("study").await.unwrap(), assets[..1]);
        assert_eq!(
            catalog
                .find_in(AssetView::Tag("Color study".to_owned()), "")
                .await
                .unwrap()
                .len(),
            1
        );

        catalog.delete_folder(first).await.unwrap();
        catalog.set_trashed(ids[1], false).await.unwrap();
        assert_eq!(
            catalog
                .folders()
                .await
                .unwrap()
                .iter()
                .map(|folder| folder.id)
                .collect::<Vec<_>>(),
            [second]
        );
        assert_eq!(
            catalog
                .find_in(AssetView::Folder(second), "")
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(catalog.find("").await.unwrap(), assets);
        assert_eq!(fs::read(fixture.0.join("one.png")).unwrap(), PNG);
        catalog
            .edit_metadata(
                &ids[..1],
                &MetadataEdit::Tags {
                    add: vec![],
                    remove: vec!["Color study".to_owned()],
                },
            )
            .await
            .unwrap();
        assert_eq!(
            catalog.organization().await.unwrap().counts[&AssetView::Untagged],
            1
        );
        catalog.close().await.unwrap();
    });
}

#[test]
fn adding_multiple_tags_preserves_existing_assignments_after_reopening() {
    use assetcraft::MetadataEdit;

    let fixture = Fixture::new();
    fixture.create();
    fixture.write("photo.png", PNG);
    fixture.ok(&["add", CATALOG, "photo.png"]);
    let path = fixture.0.join(CATALOG);
    block_on(async {
        let mut catalog = Catalog::open(&path).await.unwrap();
        let id = catalog.find("").await.unwrap()[0].id;
        for tag in ["Existing", "Editorial", "Palette"] {
            catalog.create_tag(tag).await.unwrap();
        }
        for tags in [
            vec!["Existing".to_owned()],
            vec!["Editorial".to_owned(), "Palette".to_owned()],
        ] {
            catalog
                .edit_metadata(
                    &[id],
                    &MetadataEdit::Tags {
                        add: tags,
                        remove: vec![],
                    },
                )
                .await
                .unwrap();
        }
        catalog.close().await.unwrap();

        let mut reopened = Catalog::open(&path).await.unwrap();
        let organization = reopened.organization().await.unwrap();
        assert_eq!(
            organization.assets[&id].tags,
            ["Editorial", "Existing", "Palette"]
        );
        reopened.close().await.unwrap();
    });
}

#[cfg(feature = "desktop")]
#[test]
fn video_playback_keeps_presentation_order_after_seeking_backwards() {
    use craft::host::queue::Cancel;
    use craft::media::isobmff::{
        Brand, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions,
    };
    use craft::video::prores::{ChromaFormat, Encoder, Frame, Profile};
    use desktop_video::stream::{FrameUpdate, Quality};
    use std::io::Cursor;

    let fixture = Fixture::new();
    let width = 1600;
    let mut encoder = Encoder::new(Profile::Standard, width, 32);
    let mut frame = Frame::new(width, 32, ChromaFormat::Yuv422, 10, false).unwrap();
    frame.cb.fill(512);
    frame.cr.fill(512);
    let mut movie =
        Mp4Writer::new(Cursor::new(Vec::new()), WriterOptions::new(Brand::Mov)).unwrap();
    let track = movie
        .add_track(TrackConfig::new(
            SampleEntry::prores(craft::media::isobmff::FourCc(*b"apcn"), width as u16, 32),
            1000,
        ))
        .unwrap();
    for (luminance, duration) in [(200, 40), (460, 80), (800, 30)] {
        frame.y.fill(luminance);
        movie
            .write_sample(
                track,
                WriteSample {
                    data: &encoder.encode(&frame).unwrap(),
                    duration,
                    composition_offset: 0,
                    is_sync: true,
                },
            )
            .unwrap();
    }
    fixture.write("timed.mov", &movie.finish().unwrap().into_inner());
    let cancel = Cancel::new();
    let mut stream =
        desktop_video::stream::Video::open(&fixture.0.join("timed.mov"), Format::Mov, &cancel)
            .unwrap();
    let marks = stream.timeline();
    assert_eq!(&*marks, &[0, 40_000_000, 120_000_000]);
    assert!(stream.duration() > marks[2]);
    assert_eq!(stream.audio_offset(), None);
    let ready = |update| match update {
        FrameUpdate::Ready(Some(image)) => image,
        _ => panic!("expected a newly decoded frame"),
    };
    let opening = ready(stream.frame(0, true, Quality::Still, &cancel).unwrap());
    let middle = ready(
        stream
            .frame(42_000_000, false, Quality::Playback, &cancel)
            .unwrap(),
    );
    let last = ready(
        stream
            .frame(121_000_000, false, Quality::Playback, &cancel)
            .unwrap(),
    );
    assert!(opening.dimensions().0 > last.dimensions().0);
    let paused = ready(
        stream
            .frame(121_000_000, false, Quality::Still, &cancel)
            .unwrap(),
    );
    assert_eq!(paused.dimensions(), opening.dimensions());
    assert!(opening.to_rgba8()[0] < middle.to_rgba8()[0]);
    assert!(middle.to_rgba8()[0] < last.to_rgba8()[0]);
    let returned = ready(
        stream
            .frame(30_000_000, true, Quality::Still, &cancel)
            .unwrap(),
    );
    assert_eq!(returned.to_rgba8(), opening.to_rgba8());
}

#[cfg(feature = "desktop")]
#[path = "../../src/desktop_scene_render.rs"]
mod desktop_scene_render;

#[cfg(feature = "desktop")]
#[test]
fn cineon_preview_accepts_tight_eight_bit_rgb() {
    use craft::image_codecs::Limits;

    let mut bytes = vec![0; 1024];
    bytes[..4].copy_from_slice(&0x802a_5fd7u32.to_be_bytes());
    bytes[4..8].copy_from_slice(&1024u32.to_be_bytes());
    bytes[193] = 3;
    bytes[681] = 5;
    for channel in 0..3 {
        let at = channel * 28;
        bytes[198 + at] = 8;
        bytes[200 + at..204 + at].copy_from_slice(&2u32.to_be_bytes());
        bytes[204 + at..208 + at].copy_from_slice(&1u32.to_be_bytes());
    }
    bytes.extend([255, 0, 0, 0, 128, 64]);
    let image = desktop_documents::cineon_8bit(&bytes, Limits::default())
        .unwrap()
        .unwrap();
    assert_eq!(image.dimensions(), (2, 1));
    assert_eq!(image.to_rgba8(), [255, 0, 0, 255, 0, 128, 64, 255]);
    bytes.pop();
    assert!(desktop_documents::cineon_8bit(&bytes, Limits::default()).is_none());
}

#[cfg(feature = "desktop")]
#[test]
fn lottie_poster_renders_vector_shapes_on_the_animation_canvas() {
    use craft::host::queue::Cancel;
    use craft::image_codecs::Limits;

    let bytes = br#"{
        "v":"5.7.4","w":80,"h":40,"fr":30,"ip":0,"op":60,
        "layers":[{"ty":4,"ip":0,"op":60,"ks":{"p":{"a":1,"k":[{"t":0,"s":[40,20,0]}]}},
            "shapes":[{"ty":"el","s":{"a":0,"k":[20,20]}},
                      {"ty":"fl","c":{"a":0,"k":[0,1,0,1]}}]}]
    }"#;
    let preview =
        desktop_extra::render(Format::Lottie, bytes, Limits::default(), 80, &Cancel::new());
    let image = preview.image.unwrap();
    assert_eq!(image.dimensions(), (80, 40));
    let pixels = image.to_rgba8();
    assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
    let center = (20 * 80 + 40) * 4;
    assert_eq!(&pixels[center..center + 4], &[0, 255, 0, 255]);
}

#[cfg(feature = "desktop")]
#[test]
fn lottie_parent_fill_preserves_child_group_opacity() {
    use craft::host::queue::Cancel;
    use craft::image_codecs::Limits;

    let bytes = br#"{
        "v":"5.7.4","w":80,"h":40,"fr":30,"ip":0,"op":60,
        "layers":[{"ty":4,"ip":0,"op":60,"shapes":[
            {"ty":"rc","p":{"k":[10,20]},"s":{"k":[8,8]}},
            {"ty":"gr","it":[
                {"ty":"rc","p":{"k":[30,20]},"s":{"k":[8,8]}},
                {"ty":"tr","o":{"k":0}}
            ]},
            {"ty":"gr","it":[
                {"ty":"rc","p":{"k":[50,20]},"s":{"k":[8,8]}},
                {"ty":"tr","o":{"k":50}}
            ]},
            {"ty":"fl","c":{"k":[0,1,0,1]}}
        ]}]
    }"#;
    let preview =
        desktop_extra::render(Format::Lottie, bytes, Limits::default(), 80, &Cancel::new());
    let pixels = preview.image.unwrap().to_rgba8();
    assert_eq!(pixels[(20 * 80 + 10) * 4 + 3], 255);
    assert_eq!(pixels[(20 * 80 + 30) * 4 + 3], 0);
    assert_eq!(pixels[(20 * 80 + 50) * 4 + 3], 128);
}

#[cfg(feature = "desktop")]
#[test]
fn epsi_unicode_whitespace_obeys_the_pixel_limit() {
    use craft::host::queue::Cancel;
    use craft::image_codecs::Limits;

    let bytes = "%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 8 2\n%%BeginPreview: 8\u{a0}2 1 2\n% ff\n% 00\n%%EndPreview\n%%EOF";
    let decode = |max_pixels| {
        desktop_documents::render(
            Format::Eps,
            bytes.as_bytes().to_vec(),
            Limits {
                max_pixels,
                ..Limits::default()
            },
            64,
            &Cancel::new(),
        )
        .image
    };
    assert!(matches!(
        decode(15),
        Err(craft::browse::thumbs::ThumbError::Failed(message)) if message.contains("pixel limit")
    ));
    assert_eq!(decode(16).unwrap().dimensions(), (8, 2));
}

#[cfg(feature = "desktop")]
#[test]
fn eps_preview_reads_image_magick_rgb_without_interpreting_postscript() {
    use craft::host::queue::Cancel;
    use craft::image_codecs::Limits;

    let bytes = b"%!PS-Adobe-3.0 EPSF-3.0\n%%Creator: (ImageMagick)\n%%BoundingBox: 0 0 2 1\nuserdict begin\nDisplayImage\n0 0\n2 1\n12\n2 1\n0\n0\nFF0000008000\n\nend\n%%EOF";
    let preview = desktop_documents::render(
        Format::Eps,
        bytes.to_vec(),
        Limits::default(),
        64,
        &Cancel::new(),
    );
    let image = preview.image.unwrap();
    assert_eq!(image.dimensions(), (2, 1));
    assert_eq!(image.to_rgba8(), [255, 0, 0, 255, 0, 128, 0, 255]);
}

#[cfg(feature = "desktop")]
#[test]
fn orbit_reprojects_geometry_and_environment_lights_the_scene() {
    use craft::host::queue::Cancel;
    use desktop_scene_render::{Environment, Orbit, Quality, Scene, Studio};

    let fixture = Fixture::new();
    fixture.write(
        "model.obj",
        b"v 0 0 0\nv 2 0 0\nv 0 3 0\nv 0 0 1\nf 1 2 3\nf 1 4 2\nf 2 4 3\nf 1 3 4\n",
    );
    let cancel = Cancel::new();
    let scene = Scene::open(&fixture.0.join("model.obj"), Format::Obj, &cancel).unwrap();
    let view = Orbit {
        grid: false,
        show_environment: false,
        ..Default::default()
    };
    let first = scene
        .render(view, None, 96, Quality::Settled, &cancel)
        .unwrap()
        .to_rgba8();
    let orbit = scene
        .render(
            Orbit {
                azimuth: 135.0,
                ..view
            },
            None,
            96,
            Quality::Settled,
            &cancel,
        )
        .unwrap()
        .to_rgba8();
    assert_ne!(first, orbit);
    let larger = scene
        .render(view, None, 1024, Quality::Settled, &cancel)
        .unwrap();
    assert!(larger.dimensions().0 > 640);
    assert!(
        larger
            .to_rgba8()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| *pixel != [242, 246, 250, 255])
    );

    let data: Vec<_> = (0..32 * 16)
        .flat_map(|i| {
            if i % 32 < 12 {
                [8.0f32, 0.3, 0.15]
            } else {
                [0.08, 0.3, 1.5]
            }
        })
        .collect();
    let environment = Image::from_normalized(
        32,
        16,
        ChannelLayout::Rgb,
        image_codecs::SampleType::F32,
        &data,
    )
    .unwrap();
    fixture.write(
        "environment.hdr",
        &image_codecs::encode(&environment, image_codecs::Format::Hdr, &Default::default())
            .unwrap(),
    );
    let environment = Environment::open(&fixture.0.join("environment.hdr"), &cancel).unwrap();
    let lit = scene
        .render(view, Some(&environment), 96, Quality::Settled, &cancel)
        .unwrap()
        .to_rgba8();
    let rotated = scene
        .render(
            Orbit {
                environment_rotation: 90.0,
                ..view
            },
            Some(&environment),
            96,
            Quality::Settled,
            &cancel,
        )
        .unwrap()
        .to_rgba8();
    assert_ne!(lit, first);
    assert_ne!(lit, rotated);

    let dark = Orbit {
        grid: true,
        studio: Studio {
            background: [22, 24, 28],
            grid: [53, 56, 61],
            axis: [105, 110, 119],
        },
        ..view
    };
    let image = scene
        .render(dark, Some(&environment), 96, Quality::Settled, &cancel)
        .unwrap()
        .to_rgba8();
    assert_eq!(&image[..4], &[22, 24, 28, 255]);
    assert!(image.as_chunks::<4>().0.contains(&[53, 56, 61, 255]));
    let panorama = scene
        .render(
            Orbit {
                show_environment: true,
                ..dark
            },
            Some(&environment),
            96,
            Quality::Settled,
            &cancel,
        )
        .unwrap()
        .to_rgba8();
    assert_ne!(&panorama[..4], &image[..4]);
}

#[cfg(feature = "desktop")]
#[test]
fn perspective_scales_geometry_by_depth() {
    use craft::host::queue::Cancel;
    use desktop_scene_render::{Orbit, Projection, Quality, Scene};

    let fixture = Fixture::new();
    fixture.write(
        "depth.obj",
        b"v -2 -1 2\nv -1 -1 2\nv -1.5 1 2\nv 1 -1 -2\nv 2 -1 -2\nv 1.5 1 -2\nf 1 2 3\nf 4 5 6\n",
    );
    let cancel = Cancel::new();
    let scene = Scene::open(&fixture.0.join("depth.obj"), Format::Obj, &cancel).unwrap();
    let orthographic = Orbit {
        projection: Projection::Orthographic,
        azimuth: 0.0,
        elevation: 0.0,
        grid: false,
        ..Orbit::default()
    };
    let perspective = Orbit {
        azimuth: orthographic.azimuth,
        elevation: orthographic.elevation,
        grid: false,
        ..Orbit::default()
    };
    const EDGE: u32 = 256;
    let projected_areas = |view| {
        let rgba = scene
            .render(view, None, EDGE, Quality::Settled, &cancel)
            .unwrap()
            .to_rgba8();
        let mut areas = [0usize; 2];
        for (i, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
            if *pixel != [242, 246, 250, 255] {
                let x = i % EDGE as usize;
                areas[usize::from(x >= EDGE as usize / 2)] += 1;
            }
        }
        areas
    };
    let [near, far] = projected_areas(orthographic);
    assert!(far > 0 && near.abs_diff(far) <= 1, "{near} vs {far}");
    let [near, far] = projected_areas(perspective);
    assert!(far > 0 && near * 2 > far * 3, "{near} vs {far}");
    let quick = scene
        .render(perspective, None, 96, Quality::Interactive, &cancel)
        .unwrap()
        .to_rgba8();
    let settled = scene
        .render(perspective, None, 96, Quality::Settled, &cancel)
        .unwrap()
        .to_rgba8();
    let colors: std::collections::HashSet<_> = quick.as_chunks::<4>().0.iter().collect();
    assert!(
        settled
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| !colors.contains(pixel))
    );
}

#[cfg(feature = "desktop")]
#[path = "../../src/desktop_movie_audio.rs"]
mod desktop_movie_audio;

#[cfg(feature = "desktop")]
#[test]
fn large_movie_keeps_its_audio_edit_and_can_seek_to_pcm() {
    use craft::host::queue::Cancel;
    use craft::media::isobmff::{
        Brand, Edit, FourCc, Mp4Writer, PcmConfig, SampleEntry, TrackConfig, WriteSample,
        WriterOptions,
    };
    use craft::video::prores::{ChromaFormat, Encoder, Frame, Profile};

    let fixture = Fixture::new();
    let path = fixture.0.join("delayed.mov");
    let mut movie = Mp4Writer::new(
        std::io::Cursor::new(Vec::new()),
        WriterOptions::new(Brand::Mov),
    )
    .unwrap();
    let video = movie
        .add_track(TrackConfig::new(
            SampleEntry::prores(FourCc(*b"apcn"), 32, 32),
            1000,
        ))
        .unwrap();
    let mut sound = TrackConfig::new(
        SampleEntry::pcm(PcmConfig {
            bits: 16,
            float: false,
            big_endian: false,
            signed: true,
            channels: 1,
            sample_rate: 8000.0,
        }),
        8000,
    );
    sound.edits = vec![
        Edit {
            segment_duration: 2000,
            media_time: -1,
            media_rate: 0x0001_0000,
        },
        Edit {
            segment_duration: 2000,
            media_time: 0,
            media_rate: 0x0001_0000,
        },
    ];
    let audio = movie.add_track(sound).unwrap();
    let frame = Frame::new(32, 32, ChromaFormat::Yuv422, 10, false).unwrap();
    let mut encoder = Encoder::new(Profile::Standard, 32, 32);
    movie
        .write_sample(
            video,
            WriteSample {
                data: &encoder.encode(&frame).unwrap(),
                duration: 4000,
                composition_offset: 0,
                is_sync: true,
            },
        )
        .unwrap();
    for sample in [8192i16, 16384i16] {
        let pcm = sample.to_le_bytes().repeat(8000);
        movie
            .write_sample(
                audio,
                WriteSample {
                    data: &pcm,
                    duration: 8000,
                    composition_offset: 0,
                    is_sync: true,
                },
            )
            .unwrap();
    }
    let mut bytes = movie.finish().unwrap().into_inner();
    let padding = 130u32 << 20;
    bytes.extend_from_slice(&padding.to_be_bytes());
    bytes.extend_from_slice(b"free");
    fixture.write("delayed.mov", &bytes);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.set_len(bytes.len() as u64 + u64::from(padding) - 8)
        .unwrap();
    drop(file);

    let cancel = Cancel::new();
    let video = desktop_video::stream::Video::open(&path, Format::Mov, &cancel).unwrap();
    assert_eq!(video.audio_offset(), Some(2_000_000_000));
    let file = assetcraft::local_file::open_read(&path).unwrap();
    let mut audio = desktop_movie_audio::MovieAudio::open(file, &cancel).unwrap();
    assert_eq!(audio.info(), (8000, 1, Some(16000)));
    audio.seek(8000);
    let mut values = Vec::new();
    assert!(audio.read(&mut |sample| values.push(sample)).unwrap());
    assert!(!values.is_empty());
    assert!(values.iter().all(|sample| (*sample - 0.5).abs() < 0.001));
}

#[cfg(feature = "desktop")]
#[test]
fn video_seek_resumes_after_exhausting_its_packet_budget() {
    use craft::host::queue::Cancel;
    use craft::media::isobmff::{
        Brand, FourCc, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions,
    };
    use craft::video::prores::{ChromaFormat, Encoder, Frame, Profile};
    use desktop_video::stream::{FrameUpdate, Quality, Video};

    let fixture = Fixture::new();
    let mut movie = Mp4Writer::new(
        std::io::Cursor::new(Vec::new()),
        WriterOptions::new(Brand::Mov),
    )
    .unwrap();
    let track = movie
        .add_track(TrackConfig::new(
            SampleEntry::prores(FourCc(*b"apcn"), 32, 32),
            1000,
        ))
        .unwrap();
    let mut frame = Frame::new(32, 32, ChromaFormat::Yuv422, 10, false).unwrap();
    frame.cb.fill(512);
    frame.cr.fill(512);
    let mut encoder = Encoder::new(Profile::Standard, 32, 32);
    frame.y.fill(200);
    let dark = encoder.encode(&frame).unwrap();
    frame.y.fill(800);
    let light = encoder.encode(&frame).unwrap();
    for i in 0..405 {
        movie
            .write_sample(
                track,
                WriteSample {
                    data: if i < 400 { &dark } else { &light },
                    duration: 20,
                    composition_offset: 0,
                    is_sync: i == 0,
                },
            )
            .unwrap();
    }
    fixture.write("long-gop.mov", &movie.finish().unwrap().into_inner());
    let cancel = Cancel::new();
    let mut video = Video::open(&fixture.0.join("long-gop.mov"), Format::Mov, &cancel).unwrap();
    let target = 8_000_000_000;
    assert!(matches!(
        video.frame(target, true, Quality::Still, &cancel).unwrap(),
        FrameUpdate::Pending
    ));
    let FrameUpdate::Ready(Some(image)) =
        video.frame(target, false, Quality::Still, &cancel).unwrap()
    else {
        panic!("seek did not resume at the previous decoder cursor");
    };
    assert!(image.to_rgba8()[0] > 150);
}
