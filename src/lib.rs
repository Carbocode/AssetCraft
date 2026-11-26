#![forbid(unsafe_code)]

#[doc(hidden)]
pub mod local_file;

use chrono::Utc;
use craft::atomic::{AtomicFs, RealFs};
use craft::sniff::{HEAD, Kind, identify};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqliteSynchronous};
use sqlx::{Connection, Row};
use std::collections::{BTreeMap, HashMap};
use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const APPLICATION_ID: u32 = 0x5354_4346;
const SCHEMA_VERSION: u32 = 6;
const SCHEMA: &str = include_str!("schema.sql");

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Database(sqlx::Error),
    EmptyName,
    EmptyFolderName,
    FolderNameExists,
    FolderMissing,
    InvalidRating,
    EmptyTagName,
    TagExists,
    UnknownTag,
    UnrecognizedCatalog,
    UnsupportedSchema(u32),
    InvalidRecord(&'static str),
    UnexpectedJournal(String),
    NotARegularFile,
    UnsupportedImage,
    FileChanged,
}

impl fmt::Display for Error {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(out, "{error}"),
            Self::Database(error) => write!(out, "{error}"),
            Self::EmptyName => out.write_str("The library name is empty"),
            Self::EmptyFolderName => out.write_str("Enter a folder name"),
            Self::FolderNameExists => out.write_str("A folder with that name already exists here"),
            Self::EmptyTagName => out.write_str("Enter a tag name"),
            Self::TagExists => out.write_str("That tag already exists"),
            Self::UnknownTag => out.write_str("Create the tag in the library before assigning it"),
            Self::InvalidRating => out.write_str("Choose a rating from zero to five stars"),
            Self::FolderMissing => out.write_str("This folder no longer exists"),
            Self::UnrecognizedCatalog => out.write_str("This file is not an AssetCraft catalog"),
            Self::UnsupportedSchema(version) => {
                write!(out, "AssetCraft catalog version {version} is unsupported")
            }
            Self::InvalidRecord(field) => write!(out, "Invalid catalog field {field}"),
            Self::UnexpectedJournal(mode) => write!(out, "Unsupported catalog journal mode {mode}"),
            Self::NotARegularFile => out.write_str("Choose a regular file"),
            Self::UnsupportedImage => out.write_str("Expected a PNG or JPEG file signature"),
            Self::FileChanged => out.write_str("The original changed while it was being read"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub id: Uuid,
    pub name: String,
    pub created_at_unix_ms: i64,
    pub appearance: Appearance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub id: Uuid,
    pub path: PathBuf,
    pub display_name: String,
    pub format: Format,
    pub sha256_at_import: [u8; 32],
    pub imported_at_unix_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum AssetView {
    #[default]
    All,
    Random,
    Recent,
    Favorites,
    Untagged,
    Trash,
    Folder(Uuid),
    Tag(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: Uuid,
    pub parent: Option<Uuid>,
    pub name: String,
    pub appearance: FolderAppearance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "desktop", derive(serde::Deserialize, serde::Serialize))]
pub struct Appearance {
    pub icon: String,
    pub color: Option<String>,
}

pub type FolderAppearance = Appearance;

impl Default for Appearance {
    fn default() -> Self {
        Self {
            icon: "folder".to_owned(),
            color: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssetState {
    pub favorite: bool,
    pub trashed: bool,
}

#[derive(Debug, Default)]
pub struct AssetDetails {
    pub rating: u8,
    pub notes: String,
    pub tags: Vec<String>,
    pub folders: Vec<Uuid>,
}

#[derive(Debug, Default)]
pub struct Organization {
    pub assets: HashMap<Uuid, AssetDetails>,
    pub counts: HashMap<AssetView, u64>,
    pub tags: BTreeMap<String, u64>,
}

#[derive(Debug)]
pub enum MetadataEdit {
    Tags {
        add: Vec<String>,
        remove: Vec<String>,
    },
    Folder {
        id: Uuid,
        linked: bool,
    },
    Rating(u8),
    Notes(String),
}

#[derive(Debug)]
pub enum Added {
    Imported(Asset),
    Existing(Asset),
    Rejected { path: PathBuf, error: Error },
}

pub struct Catalog {
    db: SqliteConnection,
    library: Library,
}

impl Catalog {
    pub async fn create(path: &Path, name: &str) -> Result<Self, Error> {
        Self::create_with_appearance(path, name, &Appearance::default()).await
    }

    pub async fn create_with_appearance(
        path: &Path,
        name: &str,
        appearance: &Appearance,
    ) -> Result<Self, Error> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::EmptyName);
        }
        let path = database_path(path)?;
        ensure_catalog_destination_unused(&path)?;
        // Initialize privately so a failed SQL operation leaves no destination behind.
        let mut db =
            SqliteConnection::connect_with(&catalog_options().filename(":memory:").in_memory(true))
                .await?;
        let library = Library {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            created_at_unix_ms: Utc::now().timestamp_millis(),
            appearance: appearance.clone(),
        };
        let mut transaction = db.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::raw_sql(SCHEMA).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO library (singleton, id, name, created_at_unix_ms, icon, color) VALUES (1, ?, ?, ?, ?, ?)")
            .bind(library.id.as_bytes().as_slice())
            .bind(&library.name)
            .bind(library.created_at_unix_ms)
            .bind(&appearance.icon)
            .bind(appearance.color.as_deref())
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        let bytes = db.serialize(None).await?;
        db.close().await?;
        publish_catalog(&path, &bytes)?;
        Self::open(&path).await
    }

    pub async fn open(path: &Path) -> Result<Self, Error> {
        let path = database_path(path)?;
        // Reject unrelated SQLite databases before SQLite can recover one of their journals.
        check_header(&path)?;
        let mut db = SqliteConnection::connect_with(&catalog_options().filename(&path)).await?;
        let application_id: u32 = sqlx::query_scalar("PRAGMA application_id")
            .fetch_one(&mut db)
            .await?;
        let schema: u32 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut db)
            .await?;
        check_version(application_id, schema)?;
        let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&mut db)
            .await?;
        if journal != "delete" {
            return Err(Error::UnexpectedJournal(journal));
        }
        let row = sqlx::query(
            "SELECT id, name, created_at_unix_ms, icon, color FROM library WHERE singleton = 1",
        )
        .fetch_optional(&mut db)
        .await?
        .ok_or(Error::InvalidRecord("library"))?;
        let library = Library {
            id: read_id(&row)?,
            name: row.try_get("name")?,
            created_at_unix_ms: row.try_get("created_at_unix_ms")?,
            appearance: Appearance {
                icon: row.try_get("icon")?,
                color: row.try_get("color")?,
            },
        };
        Ok(Self { db, library })
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub async fn set_library_appearance(&mut self, appearance: Appearance) -> Result<(), Error> {
        let result = sqlx::query("UPDATE library SET icon = ?, color = ? WHERE singleton = 1")
            .bind(&appearance.icon)
            .bind(appearance.color.as_deref())
            .execute(&mut self.db)
            .await?;
        if result.rows_affected() == 0 {
            return Err(Error::InvalidRecord("library"));
        }
        self.library.appearance = appearance;
        Ok(())
    }

    /// Reads originals with blocking file I/O. A graphical caller should run this off its UI thread.
    pub async fn add(&mut self, paths: &[PathBuf]) -> Result<Vec<Added>, Error> {
        self.add_to_folder(paths, None).await
    }

    pub async fn add_to_folder(
        &mut self,
        paths: &[PathBuf],
        folder: Option<Uuid>,
    ) -> Result<Vec<Added>, Error> {
        enum Candidate {
            Existing(Asset),
            New(Asset),
            Rejected { path: PathBuf, error: Error },
        }

        let mut candidates = Vec::with_capacity(paths.len());
        for path in paths {
            let resolved = match resolve_file(path) {
                Ok(resolved) => resolved,
                Err(error) => {
                    candidates.push(Candidate::Rejected {
                        path: path.clone(),
                        error,
                    });
                    continue;
                }
            };
            if let Some(asset) = find_path(&mut self.db, &resolved).await? {
                candidates.push(Candidate::Existing(asset));
                continue;
            }
            candidates.push(match inspect_file(resolved) {
                Ok(asset) => Candidate::New(asset),
                Err(error) => Candidate::Rejected {
                    path: path.clone(),
                    error,
                },
            });
        }

        let mut transaction = self.db.begin_with("BEGIN IMMEDIATE").await?;
        let mut outcomes = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            outcomes.push(match candidate {
                Candidate::Existing(asset) => Added::Existing(asset),
                Candidate::Rejected { path, error } => Added::Rejected { path, error },
                Candidate::New(asset) => {
                    let inserted = sqlx::query(
                        "INSERT INTO asset VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(path) DO NOTHING",
                    )
                    .bind(asset.id.as_bytes().as_slice())
                    .bind(local_file::encode_path(&asset.path))
                    .bind(&asset.display_name)
                    .bind(asset.format.as_str())
                    .bind(asset.sha256_at_import.as_slice())
                    .bind(asset.imported_at_unix_ms)
                    .execute(&mut *transaction)
                    .await?
                    .rows_affected();
                    if inserted == 1 {
                        Added::Imported(asset)
                    } else {
                        let existing = find_path(&mut transaction, &asset.path)
                            .await?
                            .ok_or(Error::InvalidRecord("conflicting asset"))?;
                        Added::Existing(existing)
                    }
                }
            });
        }
        if let Some(folder) = folder {
            for outcome in &outcomes {
                let asset = match outcome {
                    Added::Imported(asset) | Added::Existing(asset) => asset,
                    Added::Rejected { .. } => continue,
                };
                sqlx::query("INSERT INTO folder_asset VALUES (?, ?) ON CONFLICT DO NOTHING")
                    .bind(folder.as_bytes().as_slice())
                    .bind(asset.id.as_bytes().as_slice())
                    .execute(&mut *transaction)
                    .await?;
            }
        }
        transaction.commit().await?;
        Ok(outcomes)
    }

    pub async fn folders(&mut self) -> Result<Vec<Folder>, Error> {
        let rows =
            sqlx::query("SELECT id, parent_id, name, icon, color FROM folder ORDER BY name, id")
                .fetch_all(&mut self.db)
                .await?;
        rows.iter()
            .map(|row| {
                let parent: Option<Vec<u8>> = row.try_get("parent_id")?;
                Ok(Folder {
                    id: read_id(row)?,
                    parent: parent
                        .map(|bytes| Uuid::from_slice(&bytes))
                        .transpose()
                        .map_err(|_| Error::InvalidRecord("parent folder"))?,
                    name: row.try_get("name")?,
                    appearance: FolderAppearance {
                        icon: row.try_get("icon")?,
                        color: row.try_get("color")?,
                    },
                })
            })
            .collect()
    }

    pub async fn create_folder(&mut self, parent: Option<Uuid>, name: &str) -> Result<Uuid, Error> {
        self.create_folder_with_appearance(parent, name, &FolderAppearance::default())
            .await
    }

    pub async fn create_folder_with_appearance(
        &mut self,
        parent: Option<Uuid>,
        name: &str,
        appearance: &FolderAppearance,
    ) -> Result<Uuid, Error> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::EmptyFolderName);
        }
        let id = Uuid::new_v4();
        let result = sqlx::query("INSERT INTO folder (id, parent_id, name, icon, color) VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING")
            .bind(id.as_bytes().as_slice())
            .bind(parent.as_ref().map(|value| value.as_bytes().as_slice()))
            .bind(name)
            .bind(&appearance.icon)
            .bind(appearance.color.as_deref())
            .execute(&mut self.db)
            .await?;
        if result.rows_affected() == 0 {
            return Err(Error::FolderNameExists);
        }
        Ok(id)
    }

    pub async fn set_folder_appearance(
        &mut self,
        id: Uuid,
        appearance: &FolderAppearance,
    ) -> Result<(), Error> {
        let result = sqlx::query("UPDATE folder SET icon = ?, color = ? WHERE id = ?")
            .bind(&appearance.icon)
            .bind(appearance.color.as_deref())
            .bind(id.as_bytes().as_slice())
            .execute(&mut self.db)
            .await?;
        if result.rows_affected() == 0 {
            return Err(Error::FolderMissing);
        }
        Ok(())
    }

    pub async fn delete_folder(&mut self, id: Uuid) -> Result<(), Error> {
        let result = sqlx::query("DELETE FROM folder WHERE id = ?")
            .bind(id.as_bytes().as_slice())
            .execute(&mut self.db)
            .await?;
        if result.rows_affected() == 0 {
            return Err(Error::FolderMissing);
        }
        Ok(())
    }

    pub async fn set_favorite(&mut self, id: Uuid, value: bool) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO asset_state (asset_id, favorite) VALUES (?, ?)
                     ON CONFLICT(asset_id) DO UPDATE SET favorite = excluded.favorite",
        )
        .bind(id.as_bytes().as_slice())
        .bind(value)
        .execute(&mut self.db)
        .await?;
        Ok(())
    }

    pub async fn set_trashed(&mut self, id: Uuid, value: bool) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO asset_state (asset_id, trashed) VALUES (?, ?)
                     ON CONFLICT(asset_id) DO UPDATE SET trashed = excluded.trashed",
        )
        .bind(id.as_bytes().as_slice())
        .bind(value)
        .execute(&mut self.db)
        .await?;
        Ok(())
    }

    pub async fn find(&mut self, needle: &str) -> Result<Vec<Asset>, Error> {
        Ok(self
            .find_in(AssetView::All, needle)
            .await?
            .into_iter()
            .map(|(asset, _)| asset)
            .collect())
    }

    pub async fn find_in(
        &mut self,
        view: AssetView,
        needle: &str,
    ) -> Result<Vec<(Asset, AssetState)>, Error> {
        self.find_filtered(view, needle, None, false).await
    }

    pub async fn find_filtered(
        &mut self,
        view: AssetView,
        needle: &str,
        tag: Option<&str>,
        include_subfolders: bool,
    ) -> Result<Vec<(Asset, AssetState)>, Error> {
        let mut query = sqlx::QueryBuilder::new(
            "SELECT asset.*, COALESCE(favorite, 0) AS favorite, COALESCE(trashed, 0) AS trashed
             FROM asset LEFT JOIN asset_state ON asset_state.asset_id = asset.id
             WHERE COALESCE(trashed, 0) = ",
        );
        query
            .push_bind(view == AssetView::Trash)
            .push(" AND (instr(display_name, ")
            .push_bind(needle)
            .push(") > 0 OR EXISTS (SELECT 1 FROM asset_tag WHERE asset_tag.asset_id = asset.id AND instr(tag, ")
            .push_bind(needle)
            .push(") > 0))");
        match &view {
            AssetView::Recent => {
                let cutoff = Utc::now() - chrono::Duration::days(30);
                query
                    .push(" AND imported_at_unix_ms >= ")
                    .push_bind(cutoff.timestamp_millis());
            }
            AssetView::Favorites => {
                query.push(" AND favorite = 1");
            }
            AssetView::Untagged => {
                query.push(
                    " AND NOT EXISTS (SELECT 1 FROM asset_tag WHERE asset_tag.asset_id = asset.id)",
                );
            }
            AssetView::Folder(id) => {
                query.push(" AND EXISTS (SELECT 1 FROM folder_asset WHERE folder_asset.asset_id = asset.id AND ");
                if include_subfolders {
                    query.push("folder_id IN (WITH RECURSIVE descendants(id) AS (SELECT ")
                        .push_bind(id.as_bytes().to_vec())
                        .push(" UNION SELECT folder.id FROM folder JOIN descendants ON folder.parent_id = descendants.id) SELECT id FROM descendants))");
                } else {
                    query
                        .push("folder_id = ")
                        .push_bind(id.as_bytes().to_vec())
                        .push(")");
                }
            }
            AssetView::Tag(tag) => {
                query.push(" AND EXISTS (SELECT 1 FROM asset_tag WHERE asset_tag.asset_id = asset.id AND tag = ")
                    .push_bind(tag).push(")");
            }
            AssetView::All | AssetView::Random | AssetView::Trash => {}
        }
        if let Some(tag) = tag {
            query.push(" AND EXISTS (SELECT 1 FROM asset_tag WHERE asset_tag.asset_id = asset.id AND tag = ")
                .push_bind(tag).push(")");
        }
        if view == AssetView::Recent {
            query.push(" ORDER BY imported_at_unix_ms DESC, display_name, path");
        } else {
            query.push(" ORDER BY display_name, path");
        }
        let rows = query.build().fetch_all(&mut self.db).await?;
        rows.iter()
            .map(|row| {
                Ok((
                    read_asset(row)?,
                    AssetState {
                        favorite: row.try_get("favorite")?,
                        trashed: row.try_get("trashed")?,
                    },
                ))
            })
            .collect()
    }

    pub async fn organization(&mut self) -> Result<Organization, Error> {
        let mut result = Organization::default();
        let cutoff = Utc::now() - chrono::Duration::days(30);
        let mut snapshot = self.db.begin().await?;
        let totals = sqlx::query(
            "WITH active AS (
                 SELECT a.id, a.imported_at_unix_ms, s.favorite
                 FROM asset a LEFT JOIN asset_state s ON s.asset_id = a.id
                 WHERE COALESCE(s.trashed, 0) = 0
             )
             SELECT COUNT(*) AS total,
                COUNT(CASE WHEN imported_at_unix_ms >= ? THEN 1 END) AS recent,
                COUNT(CASE WHEN favorite = 1 THEN 1 END) AS favorites,
                COUNT(CASE WHEN NOT EXISTS (SELECT 1 FROM asset_tag t WHERE t.asset_id = active.id) THEN 1 END) AS untagged,
                (SELECT COUNT(*) FROM asset_state WHERE trashed = 1) AS trash
             FROM active",
        ).bind(cutoff.timestamp_millis()).fetch_one(&mut *snapshot).await?;
        for (view, field) in [
            (AssetView::All, "total"),
            (AssetView::Recent, "recent"),
            (AssetView::Favorites, "favorites"),
            (AssetView::Untagged, "untagged"),
            (AssetView::Trash, "trash"),
        ] {
            result
                .counts
                .insert(view, totals.try_get::<i64, _>(field)? as u64);
        }
        let details = sqlx::query("SELECT asset_id AS id, rating, notes FROM asset_details")
            .fetch_all(&mut *snapshot)
            .await?;
        for row in details {
            result.assets.insert(
                read_id(&row)?,
                AssetDetails {
                    rating: row.try_get("rating")?,
                    notes: row.try_get("notes")?,
                    ..AssetDetails::default()
                },
            );
        }
        let names = sqlx::query_scalar::<_, String>("SELECT name FROM library_tag")
            .fetch_all(&mut *snapshot)
            .await?;
        result.tags.extend(names.into_iter().map(|tag| (tag, 0)));
        let tags = sqlx::query(
            "SELECT t.asset_id AS id, tag, COALESCE(s.trashed, 0) AS trashed FROM asset_tag t
            LEFT JOIN asset_state s ON s.asset_id = t.asset_id ORDER BY tag",
        )
        .fetch_all(&mut *snapshot)
        .await?;
        for row in tags {
            let tag: String = row.try_get("tag")?;
            let count = result.tags.entry(tag.clone()).or_default();
            *count += u64::from(!row.try_get::<bool, _>("trashed")?);
            result
                .assets
                .entry(read_id(&row)?)
                .or_default()
                .tags
                .push(tag);
        }
        let folders = sqlx::query("SELECT f.asset_id AS id, folder_id, COALESCE(s.trashed, 0) AS trashed FROM folder_asset f
            LEFT JOIN asset_state s ON s.asset_id = f.asset_id")
            .fetch_all(&mut *snapshot).await?;
        for row in folders {
            let bytes: Vec<u8> = row.try_get("folder_id")?;
            let folder = Uuid::from_slice(&bytes).map_err(|_| Error::InvalidRecord("folder ID"))?;
            if !row.try_get::<bool, _>("trashed")? {
                *result.counts.entry(AssetView::Folder(folder)).or_default() += 1;
            }
            result
                .assets
                .entry(read_id(&row)?)
                .or_default()
                .folders
                .push(folder);
        }
        snapshot.commit().await?;
        Ok(result)
    }

    pub async fn create_tag(&mut self, name: &str) -> Result<(), Error> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::EmptyTagName);
        }
        let result =
            sqlx::query("INSERT INTO library_tag (name) VALUES (?) ON CONFLICT DO NOTHING")
                .bind(name)
                .execute(&mut self.db)
                .await?;
        if result.rows_affected() == 0 {
            return Err(Error::TagExists);
        }
        Ok(())
    }

    pub async fn edit_metadata(
        &mut self,
        assets: &[Uuid],
        edit: &MetadataEdit,
    ) -> Result<(), Error> {
        if matches!(edit, MetadataEdit::Rating(rating) if *rating > 5) {
            return Err(Error::InvalidRating);
        }
        let mut transaction = self.db.begin_with("BEGIN IMMEDIATE").await?;
        if let MetadataEdit::Tags { add, .. } = edit {
            for tag in add {
                let exists: bool =
                    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM library_tag WHERE name = ?)")
                        .bind(tag.trim())
                        .fetch_one(&mut *transaction)
                        .await?;
                if !exists {
                    return Err(Error::UnknownTag);
                }
            }
        }
        for id in assets {
            let id = id.as_bytes().as_slice();
            match edit {
                MetadataEdit::Tags { add, remove } => {
                    for tag in remove {
                        sqlx::query("DELETE FROM asset_tag WHERE asset_id = ? AND tag = ?")
                            .bind(id)
                            .bind(tag.trim())
                            .execute(&mut *transaction)
                            .await?;
                    }
                    for tag in add
                        .iter()
                        .map(|tag| tag.trim())
                        .filter(|tag| !tag.is_empty())
                    {
                        sqlx::query("INSERT INTO asset_tag VALUES (?, ?) ON CONFLICT DO NOTHING")
                            .bind(id)
                            .bind(tag)
                            .execute(&mut *transaction)
                            .await?;
                    }
                }
                MetadataEdit::Folder { id: folder, linked } => {
                    let sql = if *linked {
                        "INSERT INTO folder_asset VALUES (?, ?) ON CONFLICT DO NOTHING"
                    } else {
                        "DELETE FROM folder_asset WHERE folder_id = ? AND asset_id = ?"
                    };
                    sqlx::query(sql)
                        .bind(folder.as_bytes().as_slice())
                        .bind(id)
                        .execute(&mut *transaction)
                        .await?;
                }
                MetadataEdit::Rating(rating) => {
                    sqlx::query("INSERT INTO asset_details (asset_id, rating) VALUES (?, ?) ON CONFLICT(asset_id) DO UPDATE SET rating = excluded.rating")
                        .bind(id).bind(rating).execute(&mut *transaction).await?;
                }
                MetadataEdit::Notes(notes) => {
                    sqlx::query("INSERT INTO asset_details (asset_id, notes) VALUES (?, ?) ON CONFLICT(asset_id) DO UPDATE SET notes = excluded.notes")
                        .bind(id).bind(notes).execute(&mut *transaction).await?;
                }
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn close(self) -> Result<(), Error> {
        self.db.close().await?;
        Ok(())
    }
}

fn catalog_options() -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .foreign_keys(true)
        .synchronous(SqliteSynchronous::Full)
        .pragma("trusted_schema", "OFF")
}

fn database_path(path: &Path) -> Result<PathBuf, Error> {
    let path = std::path::absolute(path)?;
    if path.to_str().is_none() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "Catalog paths must be UTF-8").into(),
        );
    }
    Ok(path)
}

fn ensure_catalog_destination_unused(path: &Path) -> io::Result<()> {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let mut candidate = path.as_os_str().to_os_string();
        candidate.push(suffix);
        let candidate = Path::new(&candidate);
        match fs::symlink_metadata(candidate) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "Catalog destination or SQLite sidecar already exists: {}",
                        candidate.display()
                    ),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn publish_catalog(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let mut stage = tempfile::NamedTempFile::with_prefix_in(".assetcraft-", parent)?;
    stage.write_all(bytes)?;
    RealFs.sync(stage.as_file())?;
    ensure_catalog_destination_unused(path)?;
    drop(stage.persist_noclobber(path)?);
    // Publication has completed. A directory sync is best effort, as in craft-atomic.
    let _ = RealFs.sync_dir(parent);
    Ok(())
}

fn check_header(path: &Path) -> Result<(), Error> {
    let mut file = local_file::open_read(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::NotARegularFile);
    }
    let mut header = [0; 72];
    match file.read_exact(&mut header) {
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            return Err(Error::UnrecognizedCatalog);
        }
        result => result?,
    }
    if &header[..16] != b"SQLite format 3\0" {
        return Err(Error::UnrecognizedCatalog);
    }
    // The database header stores user_version at byte 60 and application_id at byte 68.
    let word = |offset| u32::from_be_bytes(std::array::from_fn(|byte| header[offset + byte]));
    check_version(word(68), word(60))
}

fn check_version(application_id: u32, schema: u32) -> Result<(), Error> {
    if application_id != APPLICATION_ID {
        return Err(Error::UnrecognizedCatalog);
    }
    if schema != SCHEMA_VERSION {
        return Err(Error::UnsupportedSchema(schema));
    }
    Ok(())
}

fn resolve_file(path: &Path) -> Result<PathBuf, Error> {
    let path = fs::canonicalize(path)?;
    if !fs::metadata(&path)?.is_file() {
        return Err(Error::NotARegularFile);
    }
    Ok(path)
}

fn inspect_file(path: PathBuf) -> Result<Asset, Error> {
    let mut file = local_file::open_read(&path)?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(Error::NotARegularFile);
    }
    let mut original = (&mut file).take(before.len());
    let mut head = Vec::with_capacity(HEAD);
    (&mut original).take(HEAD as u64).read_to_end(&mut head)?;
    let format = match identify(&head) {
        Some(Kind::Png) => Format::Png,
        Some(Kind::Jpeg) => Format::Jpeg,
        _ => return Err(Error::UnsupportedImage),
    };
    let mut digest = Sha256::new();
    digest.update(head);
    io::copy(&mut original, &mut digest)?;
    if original.limit() != 0 {
        return Err(Error::FileChanged);
    }
    let after = fs::metadata(&path)?;
    if !after.is_file() || before.len() != after.len() || before.modified()? != after.modified()? {
        return Err(Error::FileChanged);
    }
    #[cfg(unix)]
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err(Error::FileChanged);
    }
    let display_name = path
        .file_name()
        .ok_or(Error::NotARegularFile)?
        .to_string_lossy()
        .into_owned();
    Ok(Asset {
        id: Uuid::new_v4(),
        path,
        display_name,
        format,
        sha256_at_import: digest.finalize().into(),
        imported_at_unix_ms: Utc::now().timestamp_millis(),
    })
}

async fn find_path(db: &mut SqliteConnection, path: &Path) -> Result<Option<Asset>, Error> {
    sqlx::query(
        "SELECT id, path, display_name, format, sha256_at_import, imported_at_unix_ms
         FROM asset WHERE path = ?",
    )
    .bind(local_file::encode_path(path))
    .fetch_optional(db)
    .await?
    .as_ref()
    .map(read_asset)
    .transpose()
}

fn read_id(row: &sqlx::sqlite::SqliteRow) -> Result<Uuid, Error> {
    Uuid::from_slice(row.try_get::<&[u8], _>("id")?).map_err(|_| Error::InvalidRecord("id"))
}

fn read_asset(row: &sqlx::sqlite::SqliteRow) -> Result<Asset, Error> {
    let format = match row.try_get::<&str, _>("format")? {
        "png" => Format::Png,
        "jpeg" => Format::Jpeg,
        _ => return Err(Error::InvalidRecord("format")),
    };
    let bytes: Vec<u8> = row.try_get("path")?;
    let path = local_file::decode_path(bytes).ok_or(Error::InvalidRecord("path"))?;
    Ok(Asset {
        id: read_id(row)?,
        path,
        display_name: row.try_get("display_name")?,
        format,
        sha256_at_import: row
            .try_get::<&[u8], _>("sha256_at_import")?
            .try_into()
            .map_err(|_| Error::InvalidRecord("sha256_at_import"))?,
        imported_at_unix_ms: row.try_get("imported_at_unix_ms")?,
    })
}
