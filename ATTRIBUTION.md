# Third-party attribution

`jpeg-encoder` 0.6.1 is included through Craft Image Codecs under `(MIT OR Apache-2.0) AND IJG`. Copyright (c) 2021 Volker Ströbel. This software is based in part on the work of the Independent JPEG Group. [Published crate source and licence files](https://docs.rs/crate/jpeg-encoder/0.6.1/source/)

`tempfile` 3.27.0 stages new catalogs for publication. AssetCraft uses it under Apache-2.0. [Published crate source and licence files](https://docs.rs/crate/tempfile/3.27.0/source/)

HEIF decoding uses `heic-rs` 0.1.4 and JPEG XL decoding uses `jxl-oxide` 0.12.6 and its companion crates, through Craft Image Codecs. These dependencies offer MIT or Apache-2.0. [heic-rs source and notice](https://docs.rs/crate/heic-rs/0.1.4/source/) · [jxl-oxide source and licences](https://docs.rs/crate/jxl-oxide/0.12.6/source/)

The raster integration tests generate their files from original pixel arrays with Craft Image Codecs. The migration tests generate their SQLite fixtures from AssetCraft's checked-in schemas. These fixtures are covered by the repository's Apache-2.0 licence.

The PDF rasterizer uses Craft’s patched Hayro and Vello at revision `90e14375a4a461fb42eba802846250b2473366f7`. The backend carries Foxit/PDFium standard fonts and Adobe CMaps under BSD-3-Clause and a Compact ICC profile by Clinton Ingram under CC0-1.0. [Upstream provenance and asset hashes](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/ATTRIBUTION.md#assets-retained-with-the-optional-pdf-reader)

The document classification integration test generates its own SVG and document signature inputs and assembles ZIP fixtures in code with Craft. These fixtures are covered by the repository’s Apache-2.0 licence.

The video preview integration test generates an original solid-colour ProRes frame and QuickTime container in code with the pinned Craft video and media modules. The synthetic fixture is covered by the repository’s Apache-2.0 licence.

DWG conversion uses the unmodified `acadrust` 0.6.6 dependency through Craft DWG. Its source is available under MPL-2.0. The dependency includes MIT-licensed code adapted from ACadSharp. The licence and upstream third-party notices accompany releases in `NOTICE`. [Exact upstream source](https://crates.io/api/v1/crates/acadrust/0.6.6/download)

Audio preview uses Craft Audio and its unmodified Symphonia 0.6.1 fallback for formats including FLAC and Vorbis. Symphonia and its component crates are covered by MPL-2.0. [Symphonia source repository](https://github.com/pdeljanov/Symphonia) · [Exact published root crate](https://crates.io/api/v1/crates/symphonia/0.6.1/download)

Texture preview uses the Apache-2.0 `ktx2` 0.5.0 and `basisu` 0.1.0 crates. Basis transcoding uses `ruzstd` 0.7.3 under MIT. USD decoding uses `lz4_flex` 0.11.7 under MIT. The notices are in `NOTICE`. [ktx2](https://crates.io/crates/ktx2/0.5.0) · [basisu](https://crates.io/crates/basisu/0.1.0) · [ruzstd](https://crates.io/crates/ruzstd/0.7.3) · [lz4_flex](https://crates.io/crates/lz4_flex/0.11.7)

The extended-format integration test generates original palette, audio, animation, geometry and texture inputs. The fixtures are covered by this repository's Apache-2.0 licence.

The interactive-viewer integration tests generate original ProRes luminance frames, an OBJ tetrahedron and an HDR light map. The fixtures are covered by this repository's Apache-2.0 licence.

WOFF2 preview uses the MIT-licensed `wuff` 0.2.10 decoder that Craft also uses. Its licence and copyright notices are in `NOTICE`. [Exact upstream source](https://github.com/nicoburns/wuff/tree/0bb77cb3448100b8f560abf796014c8b85d75e23)

Model companion files use capability-relative opens from `cap-primitives` 4.0.3. AssetCraft uses the crate under MIT. [Published crate and licence files](https://docs.rs/crate/cap-primitives/4.0.3/source/)

## Preview gallery

The images under `docs/screenshots/` use window captures of AssetCraft v0.2.0 on Linux. The four overview images place those captures in original layouts. The captured assets are credited below. The layouts and original demo artwork are covered by the repository's Apache-2.0 licence.

| Gallery images | Source, author and licence |
| --- | --- |
| `assetcraft-020.webp`, `creative-library.webp`, `model-viewer.webp` | [Flight Helmet glTF sample](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/main/Models/FlightHelmet), © 2018 Public, CC0-1.0. Gary Hsu converted the model from Maya. The captures show studio lighting, an HDR backdrop and wireframe rendering. |
| `raster.webp`, `document.webp`, `layered.webp`, `hdr.webp`, `palette.webp`, `cad.webp`, `video.webp`, `gif.webp` and the featured overviews | [Blue Marble Next Generation, base topography and bathymetry](https://science.nasa.gov/earth/earth-observatory/blue-marble-next-generation/base-topography-bathymetry/), NASA Earth Observatory, public domain. The PNG, PDF, PSD, HDR, DDS and palette are test-library derivatives. The DXF is original graticule linework. The GIF and ProRes clip are an original globe animation using the NASA map. |
| `raw.webp` and the library overviews | [raw.pixls.us](https://raw.pixls.us/) test specimens released under CC0-1.0 by their contributors. The standalone landscape is Sony ILCE-6000 sample 1971. The grid also shows Canon EOS 7D sample 1183, Fujifilm X-T4 sample 3913, Nikon D7000 sample 958 and Panasonic DMC-LX10 sample 953. |
| `font.webp`, `glyphs.webp` and the featured overviews | [Roboto](https://github.com/google/fonts/tree/main/ofl/roboto), The Roboto Project Authors, OFL-1.1. The variable TTF used for the specimen and overview text has SHA-256 `d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134`. The overview grid also shows [Noto Sans CJK](https://github.com/notofonts/noto-cjk/tree/f8d157532fbfaeda587e826d4cd5b21a49186f7c) by the Noto Project Authors and [Source Sans 3](https://github.com/adobe-fonts/source-sans/releases/tag/3.052R) by Adobe, both OFL-1.1. |
| `creative-library.webp`, `assetcraft-020.webp` | [OpenEXR Images](https://github.com/AcademySoftwareFoundation/openexr-images), Blobbies and ColorCodedLevels, BSD-3-Clause. The notice is in `NOTICE`. The colored FBX triangles come from the [Craft mesh test fixtures at `90e1437`](https://github.com/storytold/craft-libs/tree/90e14375a4a461fb42eba802846250b2473366f7/crates/mesh/src/tests), ArtCraft Team and contributors, MIT or Apache-2.0. |
| `material.webp` and the library overviews | [MaterialX gold sample at `4d5a884`](https://github.com/KhronosGroup/MaterialX/blob/4d5a884441f39ea6fca85b5c9c01a04d61dafa96/resources/Materials/Examples/StandardSurface/standard_surface_gold.mtlx), MaterialX contributors, Apache-2.0. AssetCraft renders its constant base color. |
| `vector.webp`, `texture.webp`, `lottie.webp`, `audio.webp` and the featured overviews | Orbital SVG illustration, Woven copper KTX, Orbit swatches Lottie and Orbital signal WAV are original generated demo material, Apache-2.0. |
