<p align="center">
  <img src="assets/icon.svg" width="112" height="112" alt="AssetCraft icon">
</p>

<h1 align="center">AssetCraft</h1>

<p align="center">
  <strong>A local library for creative references.</strong><br>
  Browse and organize PNG and JPEG files where they already live.<br>
  An independent open-source app built with Rust and the Craft Framework.
</p>

<p align="center">
  <a href="https://github.com/Carbocode/assetcraft/actions/workflows/ci.yml"><img src="https://github.com/Carbocode/assetcraft/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-3a3a3a?style=flat-square" alt="Apache-2.0"></a>
  <img src="https://img.shields.io/badge/status-Linux%20preview-236b87?style=flat-square" alt="Linux preview">
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#get-started">Get started</a> ·
  <a href="#cli">CLI</a> ·
  <a href="#under-the-hood">Under the hood</a> ·
  <a href="#contributing">Contributing</a>
</p>

## Features

- **Browse visually.** Switch between grid and justified layouts and inspect thumbnails, dimensions and file properties.
- **Organize references.** Put an asset in multiple nested folders, assign several tags at once, and add ratings, favorites and notes.
- **Find what you need.** Search by name or tag and narrow results with filters. Move entries to Trash and restore them from the library.
- **Keep originals in place.** Import registers references in a local SQLite catalog. Browse or edit catalog metadata without moving the source files.

## Get started

The current source preview targets Linux. Install Rust 1.95 or newer, a C compiler and the native windowing libraries listed in the [Linux CI workflow](.github/workflows/ci.yml).

```sh
cargo run --locked --features desktop --bin assetcraft-desktop
```

Create or open a library, then use **Add files** or drop files into the workspace. Select a folder first to add imported assets to that folder. Use the left toolbar to manage the library's tags and the right inspector to assign them.

## CLI

The desktop and CLI share the same catalog.

```sh
cargo build --locked
target/debug/assetcraft-cli create ./catalog.sqlite "My library"
target/debug/assetcraft-cli add ./catalog.sqlite /path/to/logo.png /path/to/photo.jpg
target/debug/assetcraft-cli find ./catalog.sqlite logo
```

Create a catalog at an unused destination. The CLI identifies PNG and JPEG by file signature. A batch reports rejected files individually and retains successful entries. Search matches a literal, case-sensitive part of the filename. An empty query includes entries whose originals are unavailable.

## Under the hood

The Rust CLI and desktop share the SQLx catalog. The desktop uses the Craft Framework for the application shell, theme, dialogs, and shared browsing components. The public catalog format starts at version 6. Catalogs from earlier development builds need to be recreated.

## Contributing

Read the [repository guidance](AGENTS.md) for project conventions and [security policy](SECURITY.md) for reporting vulnerabilities. The CodeRabbit GitHub App reviews pull requests automatically, including drafts.

```sh
cargo fmt --all -- --check
cargo clippy --locked --features desktop --all-targets -- -D warnings
cargo test --locked --features desktop
```

### Release previews

Push a version tag such as `v0.1.0` or run **Release preview** from GitHub Actions. The workflow builds the Linux desktop and CLI in release mode and uploads a tarball and SHA-256 checksums as an Actions artifact. Preview archives can be tested before publishing a GitHub release.

## License

AssetCraft is licensed under [Apache-2.0](LICENSE). See [third-party attribution](ATTRIBUTION.md).
