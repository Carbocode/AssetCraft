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
