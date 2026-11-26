# AssetCraft repository guidance

## Upstream standards

Use the [Craft repository agent rules](https://github.com/storytold/craft-contrib/blob/e0bf5c9a60fc25b61fd7c1f4f4efb8cea33b3c8b/standards/app-agents.md) and their linked standards. AssetCraft is an independent project and the [master development brief](AssetCraft_MASTER_PROMPT.md) records its approval process.

## This repository

- `src/lib.rs` owns catalog behavior shared by the CLI in `src/main.rs` and the Linux desktop in `src/desktop.rs`. Desktop-only settings and previews live in sibling modules.
- `src/schema.sql` is the public version 6 catalog baseline. Keep original files in place and test future migrations with a copy of the prior public format.
- Run `cargo fmt --all -- --check`, `cargo clippy --locked --features desktop --all-targets -- -D warnings`, and `cargo test --locked --features desktop`. Keep integration cases in `tests/it/main.rs`.
- Use original or appropriately licensed fixtures. Record third-party licences and notices in `ATTRIBUTION.md` and `NOTICE`. Keep private catalogs, access logs, and local screenshots out of the repository.
