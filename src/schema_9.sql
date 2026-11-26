CREATE TABLE asset_v9 (
    id BLOB NOT NULL PRIMARY KEY CHECK (length(id) = 16),
    path BLOB NOT NULL UNIQUE CHECK (length(path) > 0),
    display_name TEXT NOT NULL CHECK (length(display_name) > 0),
    format TEXT NOT NULL CHECK (length(format) > 0),
    sha256_at_import BLOB NOT NULL CHECK (length(sha256_at_import) = 32),
    imported_at_unix_ms INTEGER NOT NULL
) STRICT;

INSERT INTO asset_v9 (id, path, display_name, format, sha256_at_import, imported_at_unix_ms)
SELECT id, path, display_name, format, sha256_at_import, imported_at_unix_ms FROM asset;

DROP TABLE asset;
ALTER TABLE asset_v9 RENAME TO asset;

PRAGMA user_version = 9;
