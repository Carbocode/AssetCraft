# Craft Framework alignment

## Upstream evidence

On 25 November 2026, Craft Libraries merged the official `next` branch into `main` at `90e14375a4a461fb42eba802846250b2473366f7`. Research compared the paths and hashes of 9,069 regular files and found identical contents at that merge commit and `next` at `93c2e08256da5511a1aafb294222ba8a25fbe639`. No Craft Libraries release tags were found during discovery. The merge PR displayed 70 of 85 checks passed at observation time. That summary leaves the outcome of the remaining checks unresolved. [Merge](https://github.com/storytold/craft-libs/pull/3536) · [Commit comparison](https://github.com/storytold/craft-libs/compare/90e14375a4a461fb42eba802846250b2473366f7...93c2e08256da5511a1aafb294222ba8a25fbe639)

The workspace declares edition 2024 and MSRV 1.95. It locks egui and eframe 0.37.0 at Storytold revision `b9770908e0f304896f9658d239a7b29cf8af1564`, winit 0.30.13 at `a517b3846ee8457d84ee4975110bfda6797c44dc`, and wgpu 30.0.1. A consuming workspace would apply the relevant Git patches in its root manifest and use one Craft revision. [Manifest](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/Cargo.toml) · [Lockfile](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/Cargo.lock)

The guide describes `ShellApp` and `ui-app` for a new application. PDFCraft and VectorCraft embed the framework in an existing eframe application. PhotoCraft integrated its framework port, and LightCraft has a separate `next` manifest that uses the facade. These ports provide integration evidence for the shared runtime. AssetCraft's runner and configuration still need local validation. [Framework guide](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/docs/craft-framework.md) · [PDFCraft port](https://github.com/storytold/pdfcraft/pull/8160) · [VectorCraft entry point](https://github.com/storytold/vectorcraft/blob/694f66a7bdbc6a23d652b39f81bb40993699aa54/apps/vectorcraft/src/main.rs#L762) · [PhotoCraft port](https://github.com/storytold/photocraft/pull/15449) · [LightCraft next](https://github.com/storytold/lightcraft/blob/4af60b593e8d2e2fdbf4e2db65846d7d0777ac82/Cargo.toml)

## Proposed application boundary

The proposed boundary uses the official shell, browser, theme and host services and keeps library identity, catalog mutations, search, recovery and export policy in AssetCraft. The approved evaluation revision is recorded in [D01](../decisions/REGISTRY.md), and the Linux prototype is recorded under D03 below.

VaultCraft is an open app request with Eagle as an explicit reference. LightCraft already provides photo cataloging, ratings, search, trash and virtualized browsing, and documents an append-only catalog. Scope and reuse coordination belong in the D05 dossier. Formal suite inclusion follows upstream governance and requires maintainer action. [VaultCraft request](https://github.com/storytold/artcraft/issues/2135) · [LightCraft architecture](https://github.com/storytold/lightcraft/blob/ea033fbe1c6dacdc7e1e99ee5a3e9fa0ce35d78c/docs/architecture.md#L17-L36) · [Craft app standards](https://github.com/storytold/craft-contrib/blob/e0bf5c9a60fc25b61fd7c1f4f4efb8cea33b3c8b/standards/app-agents.md) · [Decision process](https://github.com/storytold/craft-contrib/blob/e0bf5c9a60fc25b61fd7c1f4f4efb8cea33b3c8b/rfcs/0000-decision-process.md)

## D03 Linux prototype results

The temporary consumer lived outside the repository, under `a temporary local cache directory`. It used the approved Git revision with `ui-app`, `browse` and `image-codecs` and applied the Storytold egui and winit patches at its workspace root. On Linux x86_64 with Rust 1.96.0, `cargo build --locked --offline -j 5` and `cargo fmt --check` passed. The consumer's own lockfile resolved wgpu 30.0.2. Cargo reported five unused egui patches inherited from the upstream patch table.

Two consecutive native runs passed on the desktop X11 display. The prototype opened a `ShellApp` window, decoded two generated PNGs on a worker thread through `FileThumbSource` and painted both ready textures in `TileGrid`. Native Tab and Right events delivered by XTEST moved selection from the first tile to the second. The host queue test dropped a waiting job, signalled cancellation to the running job and verified that neither result was published. An earlier keyboard attempt ended when the prototype lost focus between events, so the driver checked focus before each event again on the successful runs. Native Wayland, other platforms, accessibility and catalog-scale performance were outside this verification.

The Linux graph contains 431 reachable build and runtime dependencies, including 53 Craft crates from a single revision. `craft-svg` brings in `vectorcraft-geom` and `vectorcraft-pathops` transitively. Cargo license metadata is available for every resolved dependency. It identifies conditional alternatives outside the published allowlist (`GPL-2.0-only` for `self_cell` and `MIT-0` in two expressions) and an MPL-2.0 dependency (`option-ext`) covered by separate upstream conditions. License choices, attribution and any required maintainer clarification remain open before adopting a project baseline. [Craft licensing standard](https://github.com/storytold/craft-contrib/blob/e0bf5c9a60fc25b61fd7c1f4f4efb8cea33b3c8b/standards/licences.md)

## D09 first desktop screen

| Window state | Layout and actions |
| --- | --- |
| No library open | Show **Open library** and **Create library**. Creating asks for a library name and a destination. Both commands open the resulting catalog in the window. |
| Library open | Put the library name, **Open library** and **Add files** in the top bar. Place **Search by name** above a list of matching files. Each row shows the filename and format, with an explicit status if the original is unavailable. |
| Work or errors | Keep the existing list visible during a search or import. Show whether work is in progress. At completion show the number added, already present or rejected, with per-file reasons. A cancelled picker leaves the screen as it was. |

### Reuse and implementation

- Add an opt-in Linux desktop binary using the D01 `craft` revision with `ui-app` and `dialogs`, alongside the feature set the CLI already uses. Apply the relevant Storytold egui and winit patches at the root. Resolve the exact Linux feature graph before enabling it in the project, including the dependencies enabled by the native picker. The D03 prototype used a different feature set and identified licence questions. Compare the new graph with the pinned licence standard and record the disposition of any unresolved dependency before adopting it. [Facade manifest](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/crates/craft/Cargo.toml) · [Craft licensing standard](https://github.com/storytold/craft-contrib/blob/e0bf5c9a60fc25b61fd7c1f4f4efb8cea33b3c8b/standards/licences.md)
- Use `ShellApp` and the framework theme, `TextField` for the library name and search, shared list rows and notices, and `FileService` with `PickRequest::open`, `save` and `open_many`. Expose labels and status changes to the accessibility runtime. Where a picker asks about overwriting, catalog creation still enforces the CLI's unused-destination rule. [Shell guide](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/docs/craft-framework.md) · [Native file service](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/crates/dialogs/src/service.rs)
- Run catalog actions through the shared D08 library on a Craft host job queue with one lane. Open, operate and close the same SQLite file used by the CLI. Keep picker presentation on the UI thread and database and original-file I/O off it. Associate search results with the library and query that requested them. Report a result only after its database operation returns, and refresh the list after a committed import. Submit search with Enter in the first slice.
- Defer thumbnail decoding and image previews to the browser slice. D09 uses a file list and preserves the CLI schema and behavior.

### Acceptance

1. Open an existing CLI catalog in the real Linux window and confirm the same records appear.
2. Create a second library in the window, add generated PNG and JPEG fixtures plus an unsupported file, and check the reported outcomes and original hashes. Query the resulting catalog with the CLI after closing the window.
3. Relaunch, reopen and search with the keyboard. Confirm focus returns after closing the picker and unavailable originals remain visible. Capture the accessibility result for the controls and status.
4. Verify the desktop build and rerun the existing headless integration tests. Record the exact dependency and licence inventory and any platform or backend limitations.

### D09b desktop dependency review

On 26 November 2026, an isolated consumer resolved the existing CLI dependency set together with Craft features `sniff`, `ui-app` and `dialogs` at the D01 revision. It applied the official egui and winit patches from that revision. The Linux x86_64 normal and build graph contained 336 distinct dependencies. The licence review identified two expressions that needed inspection. [Craft manifest](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/Cargo.toml) · [Craft licensing standard](https://github.com/storytold/craft-contrib/blob/e0bf5c9a60fc25b61fd7c1f4f4efb8cea33b3c8b/standards/licences.md)

- `option-ext 0.2.0` enters through `craft-settings`, `dirs` and `dirs-sys`. The standard's MPL-2.0 section allows unmodified third-party dependencies under that licence. Source availability and notices apply when shipping.
- `self_cell 1.3.0` enters through `epaint` and declares `Apache-2.0 OR GPL-2.0-only`. Craft's current standard says every identifier in the expression must be allowed. Craft Anything's own policy instead accepts permissive alternatives in dual-licensed crates and explicitly names `self_cell`. This provides a precedent for asking for clarification. The review found no suite-wide clarification that settles the discrepancy. [egui fork dependency](https://github.com/storytold/egui/blob/b9770908e0f304896f9658d239a7b29cf8af1564/Cargo.toml) · [Published crate metadata](https://crates.io/api/v1/crates/self_cell/1.3.0) · [Craft Anything policy](https://github.com/storytold/craft-anything/blob/0a4e66ba75c81ffa9a860c2987064d1ad61ab305/deny.toml)

The Apache-only `self_cell 1.2.0` is outside egui's `^1.3` requirement. Cargo rejected it in the isolated resolver. The current Craft Libraries head observed at `dc2cb42b15676a6cf8a2b1f42fe29b261d1fa56d` still uses the same egui fork pin, and the sampled egui fork head and two earlier consumer pins also require 1.3. The dependency comes from rendering, so changing the picker alone leaves the conflict. [Current Craft manifest at observation](https://github.com/storytold/craft-libs/blob/dc2cb42b15676a6cf8a2b1f42fe29b261d1fa56d/Cargo.toml)

D09c records the owner's selection of Apache-2.0 for `self_cell` and authorizes the desktop implementation under that interpretation. The discrepancy with the published suite standard remains open upstream. [Decision registry](../decisions/REGISTRY.md)

### D09 desktop implementation and verification

The desktop binary uses `ShellApp`, the Craft options bar, `FileService` and `DialogHost` for file selection, and one lane on the Craft host job queue for catalog work. The CLI and desktop use the same catalog library and schema. AccessKit is enabled through the same Storytold eframe revision used by Craft. The final locked Linux x86_64 normal and build graph contains 360 dependencies outside AssetCraft. AccessKit added dependencies with permissive licences and introduced no further exceptions to the review above.

```sh
cargo tree --locked --offline --features desktop --target x86_64-unknown-linux-gnu -e normal,build --prefix none -f '{p}|{l}'
cargo build --locked --offline --features desktop --bins
cargo fmt --check
cargo clippy --locked --offline --all-targets --features desktop -- -D warnings
cargo test --locked --offline
cargo test --locked --offline --features desktop
```

The build, formatter, Clippy and both runs of all five integration tests passed on Linux. A window on Fedora X11/XWayland ran against a private test file chooser portal with generated 2×2 PNG and JPEG files and one text file. It opened a CLI catalog, cancelled a pick, created a catalog via the keyboard, and imported two references with a per-file rejection for the text file. The CLI read two records from the desktop catalog, and the source file hashes matched. After renaming the JPEG outside AssetCraft, reopening the catalog and submitting a filename search with Enter retained its entry with the original reported unavailable. AT-SPI exposed the window controls, list item names and formats, and the import outcome.

GNOME's system picker opened and listed the CLI catalog. The automated GTK interaction did not complete selection, so system-picker acceptance and focus return remain unverified. The framework's shell runner leaves the system picker without an explicit parent window in this slice.

### SC-01a Native workspace

The desktop uses the Craft title bar and platform caption policy, independently hideable navigation and inspector panes, and the standard full-width Craft status bar. Sidebar widths are derived from window width rather than measured from pane contents. The regular and Justified views share `TileGrid` selection by catalog ID. `craft-browse` requests thumbnails through the shared cache with distinct keys for grid and inspector sizes. The `craft-image-codecs` adapter reads PNG and JPEG off the UI thread, with a 64 MiB input limit, a 32 megapixel limit and a 128 MiB codec allocation limit for this slice. The flat grid keeps a selection outline and uses the Craft cover fit. Filename and dimensions are centered below the preview only when the user enables thumbnail details. The CLI catalog schema still stores references.

The Fedora X11/XWayland window opened a CLI catalog with a generated 240 × 160 PNG and 160 × 240 JPEG via a private test chooser portal. AT-SPI verified both tiles, hidden dimensions on launch, both dimensions after opt-in, sidebar collapse and reopening from the toolbar, Justified selection, and filtering by filename with Tab and Enter. The GNOME native chooser end-to-end result and pointer focus on the integrated search field remain unverified. Collections, collection history and the filter and sort menus are outside this implemented slice.

The shell uses Craft's `ModeChoice::System`. A private XDG Settings portal drove Light → Dark → Light while the window stayed running, and the window background followed each transition. Shared theme selection across Craft apps uses `craft-theme-store` and is not connected in this slice. [Craft theme architecture](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/docs/craft-framework.md#4-the-design-system)

### SC-01b Waterfall in TileGrid

Propose a third `TileGrid` layout alongside equal cells and Justified in `craft-ui-widgets`. At the existing zoom, compute a common column width, preserve each image's aspect, and put each following item in the shortest column. Break ties at the reading start. Keep stable collection keys and model order for selection and accessibility. Define arrow navigation against tile geometry. Reuse thumbnail loading, captions and the collection component. Group headings would span all columns and restart the layout beneath the tallest preceding column. [TileGrid layout at the AssetCraft revision](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/crates/ui-widgets/src/tile_grid/layout.rs)

The shared implementation would affect layout, hit testing and viewport queries. The existing arranged layout relies on row geometry, so the proposal must prove visible-tile lookup without scanning the entire catalog on each scroll. Test containment, no overlap, mixed shapes including unknown dimensions, one narrow column, group boundaries, RTL tie-breaking, keyboard selection, viewport virtualization and the existing equal/Justified behavior. Add a gallery story with mixed aspect ratios and keyboard focus. Before any push, run the affected crate's formatter, Clippy and tests in an isolated local upstream checkout and show the resulting behavior. [Craft repository checks](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/AGENTS.md)

### SC-02 Start page and recent libraries

The desktop uses the Craft `StartScreen` column, with Create and Open in their existing catalog flow. A Craft list shows the catalog name and path when recent libraries exist. The last 12 successful opens or creations are remembered in the local Craft settings file, with newest first and deduplication by canonical path. The catalog stays separate from preferences. The window uses Craft's integrated platform controls. The feature uses the Craft Settings and Serde packages already in the locked Linux desktop dependency graph.

The Linux window test used a private chooser portal and isolated settings. It checked the first-launch actions, cancellation of Create, the integrated caption, opening a catalog, and reopening it from the recent list after restarting. Moving the fixture catalog made the recent entry report an error and left the start page usable.

The workspace's top bar uses the same recents for its library switcher. Linux X11/XWayland verification opened a different catalog from the switcher and switched back from recents. It also verified both thumbnail zoom directions, complete sidebar hiding and reopening, stable grid pixels with an image selected, and resize from 1,200 to 425 logical pixels and back.
