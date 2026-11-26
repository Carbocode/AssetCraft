CREATE TABLE asset_comment (
    id INTEGER PRIMARY KEY,
    asset_id BLOB NOT NULL REFERENCES asset(id) ON DELETE CASCADE,
    x REAL NOT NULL CHECK (x BETWEEN 0.0 AND 1.0),
    y REAL NOT NULL CHECK (y BETWEEN 0.0 AND 1.0),
    body TEXT NOT NULL CHECK (length(trim(body)) BETWEEN 1 AND 4096)
) STRICT;

CREATE INDEX asset_comment_order ON asset_comment(asset_id, id);

PRAGMA user_version = 7;
