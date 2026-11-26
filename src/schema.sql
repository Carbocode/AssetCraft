PRAGMA application_id = 0x53544346;
PRAGMA user_version = 6;

CREATE TABLE library (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    id BLOB NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    created_at_unix_ms INTEGER NOT NULL,
    icon TEXT NOT NULL DEFAULT 'folder',
    color TEXT
) STRICT;

CREATE TABLE asset (
    id BLOB NOT NULL PRIMARY KEY CHECK (length(id) = 16),
    path BLOB NOT NULL UNIQUE CHECK (length(path) > 0),
    display_name TEXT NOT NULL CHECK (length(display_name) > 0),
    format TEXT NOT NULL CHECK (format IN ('png', 'jpeg')),
    sha256_at_import BLOB NOT NULL CHECK (length(sha256_at_import) = 32),
    imported_at_unix_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE folder (
    id BLOB NOT NULL PRIMARY KEY CHECK (length(id) = 16),
    parent_id BLOB REFERENCES folder(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    icon TEXT NOT NULL DEFAULT 'folder',
    color TEXT,
    UNIQUE (parent_id, name)
) STRICT;

CREATE UNIQUE INDEX folder_root_name ON folder(name) WHERE parent_id IS NULL;

CREATE TABLE folder_asset (
    folder_id BLOB NOT NULL REFERENCES folder(id) ON DELETE CASCADE,
    asset_id BLOB NOT NULL REFERENCES asset(id) ON DELETE CASCADE,
    PRIMARY KEY (folder_id, asset_id)
) STRICT;

CREATE TABLE asset_state (
    asset_id BLOB NOT NULL PRIMARY KEY REFERENCES asset(id) ON DELETE CASCADE,
    favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    trashed INTEGER NOT NULL DEFAULT 0 CHECK (trashed IN (0, 1))
) STRICT;

CREATE TABLE asset_details (
    asset_id BLOB NOT NULL PRIMARY KEY REFERENCES asset(id) ON DELETE CASCADE,
    rating INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
    notes TEXT NOT NULL DEFAULT ''
) STRICT;

CREATE INDEX folder_asset_by_asset ON folder_asset(asset_id);

CREATE TABLE library_tag (
    name TEXT NOT NULL PRIMARY KEY CHECK (length(trim(name)) > 0)
) STRICT;

CREATE TABLE asset_tag (
    asset_id BLOB NOT NULL REFERENCES asset(id) ON DELETE CASCADE,
    tag TEXT NOT NULL REFERENCES library_tag(name),
    PRIMARY KEY (asset_id, tag)
) STRICT;

CREATE INDEX asset_tag_by_tag ON asset_tag(tag);
