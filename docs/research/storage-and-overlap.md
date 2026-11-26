# Library storage and product overlap

## Eagle V4 evidence

Eagle copies imported originals into a local `.library` folder. Its public Item API exposes paths to the original, the thumbnail and a per-item `metadata.json`. The backup guide describes copying the whole library folder, and the incremental backup guide discourages selecting files solely by folder modification time. These facts make option A the closest visible storage pattern. The public sources consulted leave Eagle's search index, authoritative catalog and commit protocol unspecified. [Import behavior](https://en.eagle.cool/support/article/does-eagle-import-files-as-copies-or-links-what-to-do-when-storage-space-is-limited) · [Item API](https://developer.eagle.cool/plugin-api/api/item) · [Library backup](https://en.eagle.cool/support/article/back-up-libraries) · [Incremental backup](https://en.eagle.cool/support/article/how-to-backup-eagle-library-efficiently-by-only-copying-recently-changed-content)

## Comparison

The initial comparison assumed managed copies to compare three storage designs on equal terms. File ownership and catalog persistence are independent choices. A and C can index external files. B can hold reference records as well, while the all-in-database variant evaluated below stores image bytes in the database. Each design can keep generated thumbnails in a disposable cache and give assets IDs independent of location.

| Option | Search and updates | Backup and portability | Crash recovery and cost |
| --- | --- | --- | --- |
| **A. Files and metadata records with a rebuildable index** | The index answers name and tag queries. Structured records or an operation journal persist tags, memberships and ratings. Rebuilds scan the authoritative records and need a consistency check after interrupted edits. | Originals stay readable as files and metadata can be inspected independently. A quiesced copy or a consistent snapshot needs a documented procedure. Large catalogs generate many file entries. | Single-file atomic replacement is reusable. Bulk updates and cross-record consistency still need a journal or equivalent. Initial implementation cost is medium. Correct recovery and migrations raise the cost. |
| **B. Transactional database holding originals and metadata** | One transaction can commit an image, metadata and queryable relationships. The engine handles indexing and locking. | A database-aware backup can capture the catalog and originals together. Large asset bytes increase the size of copies and may increase write amplification. Opening an original in another app needs extraction or export. | A single transactional boundary gives the simplest consistency model if the engine and filesystem meet their durability assumptions. Streaming blobs and efficient backup need verification. Initial implementation cost is medium. |
| **C. Filesystem originals with a transactional metadata catalog** | Database queries and transactions handle tags, search and bulk edits. Large file bytes stay outside the catalog and can be streamed directly. | Originals remain ordinary files. Backups must capture one consistent catalog plus the files it references. A consistent copy while writers and cleanup are paused is a candidate for the first release. | Publishing a file and committing a catalog row cross two durability boundaries. Staging, recovery of interrupted imports and orphan handling require explicit design. Initial implementation cost is medium to high. |

For managed copies, option C balances ordinary original files with reliable multi-asset catalog edits. Option A favors independent metadata inspection and recovery. Option B favors a single transactional snapshot. The owner chose SQLite for the catalog of referenced originals under D05c. The estimates above remain qualitative.

## File ownership and portable export

- **Reference existing files in place by default.** D05b approves this default. A PNG in a project folder stays there, and AssetCraft owns the catalog data and derived thumbnails. A permanent asset ID should survive a change of location. Lightroom Classic offers in-place import and relinking for files and folders. LightCraft documents in-place references and relinking for imported Lightroom catalogs. Eagle's managed-copy workflow favors a self-contained library. [Decision registry](../decisions/REGISTRY.md) · [Lightroom import](https://helpx.adobe.com/lightroom-classic/desktop/import-photos/import-photos-video-catalog.html) · [Lightroom relink](https://helpx.adobe.com/uk/lightroom-classic/desktop/manage-catalogs-and-files/locate-missing-photos.html) · [LightCraft catalog import](https://github.com/storytold/lightcraft/blob/ea033fbe1c6dacdc7e1e99ee5a3e9fa0ce35d78c/docs/lightroom-catalog-import.md)
- **Keep catalog state when originals are unavailable.** Distinguish missing, offline and inaccessible files where the platform provides evidence. Show retained metadata and cached previews, report unavailable originals for export, and support relinking one file or a moved folder without losing tags and memberships. In-place changes should invalidate stale thumbnails. A recreated path may contain a different asset, so an ambiguous match needs confirmation. Trashing a referenced record should affect only the catalog. Evaluate the shared Craft Browser link monitoring and relink UI before building equivalent components. [Craft Browser](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/crates/browse/README.md)
- **Offer a portable export when the user needs an independent copy.** A future export can copy selected originals and catalog metadata into one folder with relative paths and integrity hashes. Report unavailable originals explicitly and require a clean-machine round trip for a portable export claim. Backing up just the catalog of references preserves organization and needs a separate backup of the source files. Lightroom can export a catalog subset with its originals, and Apple Photos documents the backup implications of references. Core MVP export of original bytes and a reimportable portable catalog are distinct scope decisions. [Lightroom export](https://helpx.adobe.com/ro/lightroom-classic/desktop/manage-catalogs-and-files/create-catalogs.html) · [Apple Photos references](https://support.apple.com/en-euro/guide/photos/pht1ed9b966d/12.0/mac/27)
- **Keep managed copies as an explicit option for a later scope decision.** A removable card or a temporary file can warrant copying to a durable location. Use the same asset identity rules for both ownership modes. For inter-app use, first validate opening the actual file. A future durable Craft reference can resolve an asset ID to its current readable path through the shared IPC protocol once a compatible peer and authorization policy are tested. [Craft IPC](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/crates/ipc/PROTOCOL.md#4-methods)

## Recovery and comparison criteria

- Compare all three on the same generated PNG and JPEG files, tag and collection operations, query results and filesystem. Record import, search, startup, index rebuild, backup time and storage overhead separately. Verify byte equality after export and restore.
- Interrupt each design during import, bulk tag updates and backup. Include duplicate input, a second writer, read-only destination and space exhaustion. Check after restart that acknowledged assets have their original bytes and metadata and interrupted imports can be retried consistently.
- For option C, evaluate streaming to a staging file on the library filesystem, durable publication before committing the catalog row, and reconciliation of staging entries and unreferenced files after restart. Protect trash and in-flight backups from cleanup. Require a writer lock and reserve cleanup for confirmed library-owned paths. Test both process interruption and the durability assumptions separately.
- For SQLite, validate backup and journal mode on the target filesystem. Its documented online backup snapshots the database. In WAL mode, journal state can contain committed data, and the mode requires clients on the same host. A database snapshot of references still needs matching original files to restore the assets. The Craft `craft-sqlite` crate contains format helpers. The application binding remains to be selected. [SQLite backup](https://sqlite.org/backup.html) · [SQLite WAL](https://www.sqlite.org/wal.html) · [Craft SQLite helpers](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/crates/sqlite/src/lib.rs)

## Craft ecosystem overlap

- **LightCraft** already has a photo catalog, search, ratings, albums, trash and virtualized browsing. Its persistence is an append-only journal with snapshots and a writer lock. It is evidence for option A and a reference for behavior. Reuse supported shared browser and media crates. [Catalog architecture](https://github.com/storytold/lightcraft/blob/ea033fbe1c6dacdc7e1e99ee5a3e9fa0ce35d78c/docs/architecture.md#L17-L36)
- **VaultCraft** is an open proposal with direct Eagle overlap and a broader scope covering media provenance, local AI and a suite asset pool. The reviewed issue showed no assignee or linked development. Product coordination remains a separate decision. [VaultCraft request](https://github.com/storytold/artcraft/issues/2135)
- **Craft Libraries** proposes a portable manifest and files for shared creative resources, including colors, fonts, brushes and materials. Its proposed exchange model is relevant to future interoperability. It remains a proposal for shared tools and assets, with no format commitment for the approved AssetCraft MVP. [Craft Libraries request](https://github.com/storytold/artcraft/issues/2182)

## D05d SQLite validation

An isolated prototype outside the repository used Python 3.15.2's standard-library bindings to SQLite 3.53.4 on Linux with a local Btrfs filesystem. It generated 32 distinct PNGs per journal mode. The experiment used both `DELETE` and `WAL` with `synchronous=FULL`, exercising each mode independently. Its schema and flags are test inputs.

- After `SIGKILL` during an uncommitted bulk operation, reopening the database recovered the original catalog. None of the 16 staged tag assignments and no new tag remained. A concurrent reader saw no partial update.
- After `SIGKILL` following commit, all 32 new tag assignments survived reopening in both modes.
- After moving the project folder, the prototype rejected a candidate folder with one different file under the expected name. The failed attempt left all stored paths intact. Relinking the correct folder preserved 32 asset IDs, 64 tag assignments, 32 collection memberships and all original SHA-256 digests. SQLite integrity and foreign key checks passed after reopening.

The prototype tests process interruption and a controlled folder move. Hardware power loss, network and removable storage, live file changes during relinking, backup performance and the Rust integration are outside the evidence. D05c selects SQLite. The production schema, journal configuration, Rust binding and portable export format still require decisions.

## D05e Rust SQLite bindings

| Option | What it gives the MVP | Integration cost |
| --- | --- | --- |
| **rusqlite 0.42.0** | Synchronous SQLite calls and explicit transactions. `bundled` compiles SQLite from the bundled C source. The optional `backup` feature exposes snapshots through SQLite's backup API. The manifest declares MIT and Rust 1.89. [Manifest](https://github.com/rusqlite/rusqlite/blob/v0.42.0/Cargo.toml) · [Release](https://github.com/rusqlite/rusqlite/releases/tag/v0.42.0) | Keep the connection in a storage worker outside the GUI. The application defines queueing, lifecycle and transaction boundaries. A C compiler is needed for a bundled build. |
| **SQLx 0.9.0** | Async SQLite connection backed by a worker thread. A single connection can operate independently of the selected async runtime. Pool management requires a runtime feature. SQLite-only features and optional query macros can be selected explicitly. [Runtime support](https://docs.rs/sqlx/0.9.0/sqlx/#runtime-support) · [SQLite connection](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnection.html) · [Manifest](https://github.com/transact-rs/sqlx/blob/v0.9.0/Cargo.toml) | Useful if pooling or an async SQL architecture is justified. The documented SQLite driver uses a native SQLite dependency as well. Query macros add a build-time database description or offline metadata workflow when enabled. |

The initial recommendation favored `rusqlite` for a local catalog with a single storage worker. The owner chose SQLx under D05g. D05h validated the bundled driver in isolation. The production configuration, including any pool, query macros, journal mode and backup scope, remains open.

## D05h SQLx validation

The isolated prototype built on Linux x86_64 with Rust 1.96.0. SQLx 0.9.0 used `default-features = false` and `sqlite-bundled`. `libsqlite3-sys` 0.37.0 supplied SQLite 3.51.3, as reported by the running binary. The Linux build graph contained 96 dependencies including the test harness's `futures-executor` and `sha2`. It omitted Tokio, async-std, SQLx query macros and the MySQL and PostgreSQL drivers. Cargo reported a license for each active dependency. This is an inventory of declared licenses for the prototype.

Both independent runs passed with rollback journal mode `DELETE` and with `WAL`, using `synchronous=FULL`. An independent reader saw no partial edits. Killing the writer before commit rolled back all 16 staged assignments. Killing it after commit preserved all 32 new assignments. After a folder move, the prototype rejected a different file with the same name and left the catalog intact. The correct relink preserved 32 asset IDs, 64 tag assignments, 32 collection memberships and the hashes of all 32 PNGs. SQLite integrity and foreign key checks passed on reopen.

`cargo build --locked --offline`, the repeat run via `cargo run --locked --offline`, and `cargo fmt --check` passed. The fixture schema and journal settings are test inputs. The test covers process interruption on a local Linux filesystem. Hardware power loss, other platforms and a build combining SQLx with the Craft Framework remain outside this result.

## D07 first library workflow

The first slice lets a person create a named library, add local PNG and JPEG files by reference, close the app, reopen the library and find the files by name. One SQLite catalog owns one library record and any number of asset records. Each asset holds one current filesystem reference. The original file is outside the catalog's ownership. D07b approves this first slice and its minimal schema.

| Record | Fields and meaning |
| --- | --- |
| `library` | One row with a stable library ID, the display name and creation time. The ID survives moving or renaming the catalog file. |
| `asset` | A stable asset ID, the current absolute Linux path stored as bytes, a UTF-8 display name, the detected format, the import time and a SHA-256 digest of the bytes observed at import. Index the path with a uniqueness constraint. The digest records the observed content and permits later verification after a move. |

Approved schema excerpt:

```sql
CREATE TABLE library (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    id BLOB NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    created_at_unix_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE asset (
    id BLOB NOT NULL PRIMARY KEY CHECK (length(id) = 16),
    path BLOB NOT NULL UNIQUE CHECK (length(path) > 0),
    display_name TEXT NOT NULL CHECK (length(display_name) > 0),
    format TEXT NOT NULL CHECK (format IN ('png', 'jpeg')),
    sha256_at_import BLOB NOT NULL CHECK (length(sha256_at_import) = 32),
    imported_at_unix_ms INTEGER NOT NULL
) STRICT;
```

- Create only at an unused destination. On open, check the SQLite application marker, schema version and library row before enabling writes. Explicit creation owns initialization. Opening an unknown or newer catalog returns an error and leaves it intact. Propose `PRAGMA user_version = 1`; the application marker value, filename convention and migration mechanism remain open. [SQLite pragmas](https://www.sqlite.org/pragma.html#pragma_application_id) · [Strict tables](https://www.sqlite.org/stricttables.html)
- Import only explicitly selected regular local files. Store the resolved absolute path as raw Unix bytes for lossless reopening. Generate the display name separately. Probe the content through Craft before committing, and fingerprint with a bounded-memory read outside the UI. Directory scanning, recursive symlink handling and sources on remote filesystems require their own design. [Unix path bytes](https://doc.rust-lang.org/std/os/unix/ffi/trait.OsStrExt.html) · [Craft format detection](https://github.com/storytold/craft-libs/blob/90e14375a4a461fb42eba802846250b2473366f7/crates/file-formats/src/lib.rs)
- Use opaque 16-byte IDs independent of paths and hashes. The ID generator remains a dependency choice. Re-adding a resolved path returns the existing asset. Identical bytes at distinct resolved paths remain distinct assets, since each path can belong to a different project. A changed file at the same path keeps its catalog identity and requires a later metadata refresh policy. The initial digest remains historical evidence of the imported bytes.
- Keep one transaction per import batch for the successfully validated records, with per-file errors for rejected inputs and a success result only after commit. Closing and reopening reloads the committed records. Text search can use a literal substring against the display name for this first slice. An unavailable file still appears in results. The name comparison, full-text indexing, tags, collections, trash, relinking writes and derivative cache design are later increments of the approved MVP.

| Acceptance check | Expected result |
| --- | --- |
| Create, import one PNG and one JPEG, close and reopen | Same library and asset IDs, detected formats, source paths and content digests. The original files are unchanged. |
| Add the same path twice, then add an identical file from another directory | One record for the repeated path. A separate record for the other location. |
| Add a valid file alongside an unsupported file | Commit the valid record and report the rejection. The unsupported file has no record. |
| Open from a path containing non-UTF-8 filename bytes on Linux | Preserve the exact source path while displaying a readable name. |
| Remove access to an original, then reopen and search by name | Keep the record and report the original as unavailable when accessed. |
| Interrupt before batch commit or try to open an unknown schema version | Reopen with the previous committed data intact. |

The schema excerpt passed a SQLite in-memory syntax check, preserved a non-UTF-8 path byte-for-byte and rejected a duplicate path, an unsupported format and a short hash. D08 records the implemented CLI and end-to-end checks.

## D08 first CLI implementation

The implementation puts the approved storage slice in one Rust package with a library API and a `assetcraft-cli` binary. The binary provides `create <catalog> <name>`, `add <catalog> <file>...` and `find <catalog> <query>`. The caller supplies the catalog filename. Search uses a literal, case-sensitive substring.

- Use edition 2024 and Rust 1.95 as the package minimum. Keep Cargo's lockfile and verify the actual resolved build and license metadata.
- Start with SQLx 0.9.0, `default-features = false` and `sqlite-bundled`, as validated under D05h. Use `futures-executor` 0.3.32 at the CLI boundary. Use SHA-256 via `sha2` 0.10.9, UUID v4 via `uuid` 1.27.0 and UTC milliseconds via `chrono` 0.4.45. Use the official `craft` facade at the D01 revision with the `sniff` feature for byte-based format detection. Full image validation and preview belong to the preview slice.
- Propose `application_id = 0x53544346` and `user_version = 1`. Initialize them with the schema. Use rollback journal mode `DELETE` and `synchronous=FULL` for this local CLI slice. Open verifies the catalog before changing persistent settings, with versioned migrations to be decided when a version change is needed.
- Verify the approved D07 acceptance checks through normal CLI invocations with original generated fixtures. Include regression coverage for interrupted writes, unchanged source files, literal search, repeat import, non-UTF-8 paths and unsupported catalogs.

The combined Craft and SQLx graph built on Linux x86_64 with Rust 1.96.0. Its 152 active build and runtime dependencies all declared licenses using identifiers in the previously reviewed upstream allowlist. This inventory covers the CLI graph and does not establish release compliance.

`cargo build --locked --offline`, `cargo test --locked --offline` (five integration tests), `cargo clippy --locked --offline --all-targets -- -D warnings` and `cargo fmt --check` passed. The tests exercised separate CLI invocations, literal search, SHA-256 preservation, mixed batches, non-UTF-8 paths, unavailable originals, and refusal to overwrite or change an unrelated or newer catalog. Killing a process after it began a batch write preserved the previous catalog and left no partial insert.

D08 covers the first Linux CLI slice. Catalog creation reserves its destination before SQLite initialization, so an interruption during creation can leave an uninitialized file that the CLI refuses to reopen. Detection checks signatures. Full image validation, the desktop shell, packaging and portable export have their own design and verification.
