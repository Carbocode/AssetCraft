<img src="assets/icon.svg" width="64" height="64" alt="">

# AssetCraft

AssetCraft is an independent, local-first asset library built with the Craft Framework. The current preview runs on Linux and catalogs PNG and JPEG files by reference in SQLite. Your originals stay in their existing locations.

## Build and run on Linux

Install Rust 1.95 or newer and a C compiler. The desktop build also needs the native windowing libraries listed in the [Linux CI workflow](.github/workflows/ci.yml).

```sh
cargo run --locked --features desktop --bin assetcraft-desktop
```

Create or open a library from the start page. Add files with **Add files** or drag them into the workspace. Search by name or tag, browse the grid or justified view, and organize assets into nested folders. The inspector shows file properties and lets you assign folders and tags, set a rating or favorite, and edit notes. Move assets to **Trash** to remove them from active views and restore them when needed.

The first public catalog format is version 6. Catalogs from earlier development builds need to be recreated.

## CLI

The desktop and CLI share the same catalog. Build the CLI and try the workflow below.

```sh
cargo build --locked
target/debug/assetcraft-cli create ./catalog.sqlite "My library"
target/debug/assetcraft-cli add ./catalog.sqlite /path/to/logo.png /path/to/photo.jpg
target/debug/assetcraft-cli find ./catalog.sqlite logo
```

Choose an unused destination for the catalog. The CLI reads originals in place and detects PNG and JPEG by file signature. A batch reports failures per file and returns a nonzero status if any file was rejected. Valid entries remain committed. Search matches a literal, case-sensitive part of the filename. An empty query lists all registered assets, including ones whose originals are unavailable.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --locked --features desktop --all-targets -- -D warnings
cargo test --locked --features desktop
```

See [repository guidance](AGENTS.md) and [third-party attribution](ATTRIBUTION.md).

## Release previews

Push a version tag such as `v0.1.0` or run **Release preview** from GitHub Actions. The workflow builds the Linux desktop and CLI in release mode and uploads a tarball and SHA-256 checksums as an Actions artifact. Preview archives can be tested before publishing a GitHub release.

The CodeRabbit GitHub App reviews pull requests automatically, including drafts.

## License

AssetCraft is licensed under [Apache-2.0](LICENSE).
