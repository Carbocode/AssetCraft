// Generated 1×1 grayscale fixtures with a constant pixel value.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3a, 0x7e, 0x9b,
    0x55, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x70, 0x03, 0x00, 0x00,
    0x48, 0x00, 0x47, 0xe6, 0xdf, 0xdc, 0x2f, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];
const JPEG: &[u8] = &[
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08,
    0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0a, 0x0c, 0x14, 0x0d, 0x0c, 0x0b, 0x0b, 0x0c, 0x19, 0x12,
    0x13, 0x0f, 0x14, 0x1d, 0x1a, 0x1f, 0x1e, 0x1d, 0x1a, 0x1c, 0x1c, 0x20, 0x24, 0x2e, 0x27, 0x20,
    0x22, 0x2c, 0x23, 0x1c, 0x1c, 0x28, 0x37, 0x29, 0x2c, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1f, 0x27,
    0x39, 0x3d, 0x38, 0x32, 0x3c, 0x2e, 0x33, 0x34, 0x32, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x01,
    0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0xff, 0xc4, 0x00, 0x14,
    0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f, 0x00, 0x0a, 0xff, 0xd9,
];

use assetcraft::Catalog;
use futures_executor::block_on;
use sha2::{Digest, Sha256};
use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use std::ffi::{OsStr, OsString};
use std::fs;
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const CATALOG: &str = "catalog.sqlite";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("assetcraft-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_assetcraft-cli"));
        command.current_dir(&self.0);
        command
    }

    fn run<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command().args(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        stdout(self.run(args))
    }

    fn create(&self) -> String {
        self.ok(&["create", CATALOG, "Fixture library"])
    }

    fn write(&self, path: impl AsRef<Path>, bytes: &[u8]) {
        fs::write(self.0.join(path), bytes).unwrap();
    }

    async fn execute_sql(&self, sql: &'static str) {
        let options = SqliteConnectOptions::new().filename(self.0.join(CATALOG));
        let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
        sqlx::raw_sql(sql).execute(&mut connection).await.unwrap();
        connection.close().await.unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn library_and_referenced_images_survive_separate_cli_invocations() {
    let fixture = Fixture::new();
    let created = fixture.create();
    fixture.write("icon%.png", PNG);
    fixture.write("iconX.png", JPEG);
    let added = fixture.ok(&["add", CATALOG, "icon%.png", "iconX.png"]);
    let ids: Vec<_> = added
        .lines()
        .map(|line| line.split('\t').nth(1).unwrap())
        .collect();

    let literal = fixture.ok(&["find", CATALOG, "icon%"]);
    assert!(literal.starts_with(ids[0]));
    assert_eq!(literal.lines().count(), 1);
    assert!(fixture.ok(&["find", CATALOG, "Icon%"]).is_empty());
    assert!(
        fixture
            .ok(&["find", CATALOG, ""])
            .contains("\tjpeg\tavailable\t")
    );
    assert_eq!(fs::read(fixture.0.join("icon%.png")).unwrap(), PNG);
    assert_eq!(fs::read(fixture.0.join("iconX.png")).unwrap(), JPEG);

    block_on(async {
        let mut catalog = Catalog::open(&fixture.0.join(CATALOG)).await.unwrap();
        assert!(created.contains(&catalog.library().id.to_string()));
        assert_eq!(catalog.library().name, "Fixture library");
        let assets = catalog.find("icon").await.unwrap();
        assert_eq!(assets[0].id.to_string(), ids[0]);
        assert_eq!(assets[1].id.to_string(), ids[1]);
        assert_eq!(assets[0].path, fixture.0.join("icon%.png"));
        assert_eq!(assets[1].path, fixture.0.join("iconX.png"));
        assert_eq!(
            assets[0].sha256_at_import,
            <[u8; 32]>::from(Sha256::digest(PNG))
        );
        assert_eq!(
            assets[1].sha256_at_import,
            <[u8; 32]>::from(Sha256::digest(JPEG))
        );
        catalog.close().await.unwrap();
    });
}

#[test]
fn duplicate_locations_and_partial_failures_have_distinct_outcomes() {
    let fixture = Fixture::new();
    fixture.create();
    fixture.write("same.png", PNG);
    fixture.write("forged.png", b"not an image");
    fs::create_dir(fixture.0.join("other")).unwrap();
    fixture.write("other/same.png", PNG);

    let output = fixture.run([
        "add",
        CATALOG,
        "same.png",
        "./same.png",
        "forged.png",
        "other/same.png",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("forged.png"));
    let added = String::from_utf8(output.stdout).unwrap();
    let mut lines = added.lines().map(|line| line.split('\t'));
    let first: Vec<_> = lines.next().unwrap().collect();
    let duplicate: Vec<_> = lines.next().unwrap().collect();
    let other: Vec<_> = lines.next().unwrap().collect();
    assert_eq!(
        (first[0], duplicate[0], other[0]),
        ("added", "existing", "added")
    );
    assert_eq!(first[1], duplicate[1]);
    assert_ne!(first[1], other[1]);
    assert_eq!(fixture.ok(&["find", CATALOG, ""]).lines().count(), 2);
    assert!(fixture.ok(&["find", CATALOG, "forged"]).is_empty());
    fixture.write("same.png", b"now contains something else");
    let repeated = fixture.ok(&["add", CATALOG, "same.png"]);
    assert_eq!(repeated.split('\t').nth(1), Some(first[1]));
}

#[test]
fn native_paths_round_trip_and_missing_originals_remain_searchable() {
    let fixture = Fixture::new();
    fixture.create();
    #[cfg(target_os = "linux")]
    let name = OsString::from_vec(b"unusual-\xff.png".to_vec());
    #[cfg(not(target_os = "linux"))]
    let name = OsString::from("unusual-素材-📷.png");
    fixture.write(&name, PNG);
    let added = stdout(fixture.run([OsStr::new("add"), OsStr::new(CATALOG), &name]));
    let id = added.split('\t').nth(1).unwrap();

    block_on(async {
        let mut catalog = Catalog::open(&fixture.0.join(CATALOG)).await.unwrap();
        let found = catalog.find("unusual").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, fixture.0.join(&name));
        catalog.close().await.unwrap();
    });
    fs::rename(fixture.0.join(&name), fixture.0.join("moved.png")).unwrap();
    let found = fixture.ok(&["find", CATALOG, "unusual"]);
    assert!(found.starts_with(id));
    assert!(found.contains("\tunavailable\t"));
}

#[test]
fn catalog_open_and_create_preserve_unrecognized_or_unsupported_files() {
    let fixture = Fixture::new();
    fixture.write(CATALOG, b"Unrelated data");
    let output = fixture.run(["find", CATALOG, ""]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not an AssetCraft catalog"));
    assert!(
        !fixture
            .run(["create", CATALOG, "Replace me"])
            .status
            .success()
    );
    assert_eq!(
        fs::read(fixture.0.join(CATALOG)).unwrap(),
        b"Unrelated data"
    );

    fs::rename(fixture.0.join(CATALOG), fixture.0.join("untouched")).unwrap();
    fixture.create();
    block_on(fixture.execute_sql("PRAGMA user_version = 7"));
    let expected = fs::read(fixture.0.join(CATALOG)).unwrap();
    let output = fixture.run(["add", CATALOG, "image.png"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("version 7 is unsupported"));
    assert_eq!(fs::read(fixture.0.join(CATALOG)).unwrap(), expected);

    block_on(fixture.execute_sql("PRAGMA user_version = 5"));
    let expected = fs::read(fixture.0.join(CATALOG)).unwrap();
    let output = fixture.run(["find", CATALOG, ""]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("version 5 is unsupported"));
    assert_eq!(fs::read(fixture.0.join(CATALOG)).unwrap(), expected);
}

#[test]
fn killing_a_cli_batch_rolls_back_its_first_insert() {
    let fixture = Fixture::new();
    fixture.create();
    fixture.write("kept.png", PNG);
    fixture.write("first.png", PNG);
    fixture.write("slow.jpg", JPEG);
    fixture.ok(&["add", CATALOG, "kept.png"]);
    let previous = fixture.ok(&["find", CATALOG, ""]);
    // Delay the second insert until the parent observes the first insert's journal.
    block_on(fixture.execute_sql(
        "CREATE TRIGGER delay_batch BEFORE INSERT ON asset
         WHEN NEW.display_name = 'slow.jpg'
         BEGIN
            SELECT sum(n) FROM (
                WITH RECURSIVE ticks(n) AS (VALUES(0) UNION ALL
                    SELECT n + 1 FROM ticks WHERE n < 500000000)
                SELECT n FROM ticks
            );
         END;",
    ));

    let mut child = fixture
        .command()
        .args(["add", CATALOG, "first.png", "slow.jpg"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let journal = fixture.0.join("catalog.sqlite-journal");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !journal.is_file() && Instant::now() < deadline && child.try_wait().unwrap().is_none() {
        thread::sleep(Duration::from_millis(10));
    }
    let reached_write = journal.is_file();
    let _ = child.kill();
    let output = child.wait_with_output().unwrap();
    assert!(reached_write, "{}", String::from_utf8_lossy(&output.stderr));
    #[cfg(unix)]
    assert_eq!(output.status.signal(), Some(9));
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fixture.ok(&["find", CATALOG, ""]), previous);
}

#[test]
fn catalog_preserves_assets_and_persists_nested_folders() {
    use assetcraft::{Added, AssetView, Error};

    let fixture = Fixture::new();
    fixture.create();
    fixture.write("existing.png", PNG);
    fixture.write("new.png", PNG);
    let original = fixture.0.join("existing.png");
    let added = fixture.ok(&["add", CATALOG, "existing.png"]);
    let old_id = added.split('\t').nth(1).unwrap();

    block_on(async {
        let db = fixture.0.join(CATALOG);
        let mut catalog = Catalog::open(&db).await.unwrap();
        let assets = catalog.find("").await.unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].id.to_string(), old_id);
        let id = assets[0].id;

        let root = catalog.create_folder(None, "Concepts").await.unwrap();
        let child = catalog
            .create_folder(Some(root), "References")
            .await
            .unwrap();
        assert!(matches!(
            catalog.create_folder(Some(root), " References ").await,
            Err(Error::FolderNameExists)
        ));
        assert!(matches!(
            catalog.create_folder(None, "Concepts").await,
            Err(Error::FolderNameExists)
        ));
        assert!(
            catalog
                .create_folder(Some(Uuid::new_v4()), "Orphan")
                .await
                .is_err()
        );
        let new = fixture.0.join("new.png");
        let outcomes = catalog
            .add_to_folder(&[original.clone(), new], Some(child))
            .await
            .unwrap();
        assert!(matches!(&outcomes[0], Added::Existing(asset) if asset.id == id));
        assert!(matches!(&outcomes[1], Added::Imported(_)));
        catalog.set_favorite(id, true).await.unwrap();
        catalog.set_trashed(id, true).await.unwrap();
        catalog.close().await.unwrap();

        let mut reopened = Catalog::open(&db).await.unwrap();
        let folders = reopened.folders().await.unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(
            folders
                .iter()
                .find(|folder| folder.id == child)
                .unwrap()
                .parent,
            Some(root)
        );
        assert_eq!(
            reopened
                .find_in(AssetView::Folder(child), "")
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            reopened
                .find_in(AssetView::Favorites, "")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            reopened
                .find_in(AssetView::Trash, "existing")
                .await
                .unwrap()[0]
                .0
                .id,
            id
        );
        reopened.set_trashed(id, false).await.unwrap();
        assert_eq!(
            reopened.find_in(AssetView::Favorites, "").await.unwrap()[0]
                .0
                .id,
            id
        );
        assert_eq!(
            reopened.find_in(AssetView::Recent, "").await.unwrap().len(),
            2
        );
        assert_eq!(
            reopened
                .find_in(AssetView::Untagged, "")
                .await
                .unwrap()
                .len(),
            2
        );
        let linked = reopened
            .find_in(AssetView::Folder(child), "existing")
            .await
            .unwrap();
        assert_eq!(linked[0].0.path, original);
        assert_eq!(fs::read(&original).unwrap(), PNG);
        reopened.close().await.unwrap();
    });
}

#[test]
fn folder_appearance_survives_reopen() {
    use assetcraft::FolderAppearance;

    let fixture = Fixture::new();
    fixture.create();
    let db = fixture.0.join(CATALOG);
    block_on(async {
        let mut catalog = Catalog::open(&db).await.unwrap();
        let parent = catalog
            .create_folder(None, "Existing folder")
            .await
            .unwrap();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&db).await.unwrap();
        let initial = catalog.folders().await.unwrap();
        assert_eq!(initial[0].id, parent);
        assert_eq!(initial[0].appearance, FolderAppearance::default());

        let blue_star = FolderAppearance {
            icon: "star".to_owned(),
            color: Some("blue".to_owned()),
        };
        let child = catalog
            .create_folder_with_appearance(Some(parent), "Child", &blue_star)
            .await
            .unwrap();
        let image = FolderAppearance {
            icon: "image".to_owned(),
            color: None,
        };
        catalog.set_folder_appearance(parent, &image).await.unwrap();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&db).await.unwrap();
        let folders = catalog.folders().await.unwrap();
        let child = folders.iter().find(|folder| folder.id == child).unwrap();
        assert_eq!(child.parent, Some(parent));
        assert_eq!(child.appearance, blue_star);
        assert_eq!(
            folders
                .iter()
                .find(|folder| folder.id == parent)
                .unwrap()
                .appearance,
            image
        );
        catalog.close().await.unwrap();
    });
}

#[test]
fn library_appearance_survives_reopen() {
    use assetcraft::Appearance;

    let fixture = Fixture::new();
    fixture.create();
    let db = fixture.0.join(CATALOG);
    block_on(async {
        let catalog = Catalog::open(&db).await.unwrap();
        let original = catalog.library().clone();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&db).await.unwrap();
        assert_eq!(catalog.library(), &original);
        let look = Appearance {
            icon: "star".to_owned(),
            color: Some("blue".to_owned()),
        };
        catalog.set_library_appearance(look.clone()).await.unwrap();
        assert_eq!(catalog.library().appearance, look);
        catalog.close().await.unwrap();

        let reopened = Catalog::open(&db).await.unwrap();
        assert_eq!(reopened.library().id, original.id);
        assert_eq!(reopened.library().appearance, look);
        reopened.close().await.unwrap();

        let new_path = fixture.0.join("custom.sqlite");
        Catalog::create_with_appearance(&new_path, "Custom", &look)
            .await
            .unwrap()
            .close()
            .await
            .unwrap();
        let created = Catalog::open(&new_path).await.unwrap();
        assert_eq!(created.library().appearance, look);
        created.close().await.unwrap();
    });
}

#[test]
fn organization_survives_reopen_and_folder_deletion_preserves_assets() {
    use assetcraft::{AssetView, MetadataEdit};

    let fixture = Fixture::new();
    fixture.create();
    fixture.write("one.png", PNG);
    fixture.write("two.png", PNG);
    let path = fixture.0.join(CATALOG);
    block_on(async {
        let mut catalog = Catalog::open(&path).await.unwrap();
        let first = catalog.create_folder(None, "References").await.unwrap();
        let child = catalog
            .create_folder(Some(first), "Materials")
            .await
            .unwrap();
        let second = catalog.create_folder(None, "Collection").await.unwrap();
        catalog
            .add_to_folder(
                &[fixture.0.join("one.png"), fixture.0.join("two.png")],
                Some(child),
            )
            .await
            .unwrap();
        let assets = catalog.find("").await.unwrap();
        let ids: Vec<_> = assets.iter().map(|asset| asset.id).collect();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&path).await.unwrap();
        catalog.create_tag("Color study").await.unwrap();
        catalog.create_tag("Unused").await.unwrap();
        assert!(catalog.create_tag(" Color study ").await.is_err());
        assert!(
            catalog
                .edit_metadata(
                    &ids,
                    &MetadataEdit::Tags {
                        add: vec!["Uncreated tag".to_owned()],
                        remove: Vec::new()
                    }
                )
                .await
                .is_err()
        );
        catalog
            .edit_metadata(
                &ids[..1],
                &MetadataEdit::Folder {
                    id: first,
                    linked: true,
                },
            )
            .await
            .unwrap();
        catalog
            .edit_metadata(
                &ids,
                &MetadataEdit::Folder {
                    id: second,
                    linked: true,
                },
            )
            .await
            .unwrap();
        catalog
            .edit_metadata(
                &ids,
                &MetadataEdit::Tags {
                    add: vec![" Color study ".to_owned(), "Color study".to_owned()],
                    remove: vec![],
                },
            )
            .await
            .unwrap();
        catalog
            .edit_metadata(
                &ids[..1],
                &MetadataEdit::Notes("A note\non two lines".to_owned()),
            )
            .await
            .unwrap();
        catalog
            .edit_metadata(&ids, &MetadataEdit::Rating(4))
            .await
            .unwrap();
        assert!(
            catalog
                .edit_metadata(&ids, &MetadataEdit::Rating(6))
                .await
                .is_err()
        );
        catalog.set_trashed(ids[1], true).await.unwrap();
        catalog.close().await.unwrap();

        let mut catalog = Catalog::open(&path).await.unwrap();
        let organization = catalog.organization().await.unwrap();
        assert_eq!(organization.counts[&AssetView::All], 1);
        assert_eq!(organization.counts[&AssetView::Trash], 1);
        assert_eq!(organization.counts[&AssetView::Untagged], 0);
        assert_eq!(organization.counts[&AssetView::Folder(child)], 1);
        assert_eq!(organization.counts[&AssetView::Folder(second)], 1);
        assert_eq!(organization.tags["Color study"], 1);
        assert_eq!(organization.tags["Unused"], 0);
        assert!(!organization.tags.contains_key("Uncreated tag"));
        assert_eq!(
            catalog
                .find_filtered(AssetView::Folder(first), "", Some("Color study"), true)
                .await
                .unwrap()
                .len(),
            1
        );
        let first_asset = &organization.assets[&ids[0]];
        assert_eq!(first_asset.folders.len(), 3);
        assert_eq!(first_asset.rating, 4);
        assert_eq!(first_asset.notes, "A note\non two lines");
        assert_eq!(catalog.find("study").await.unwrap(), assets[..1]);
        assert_eq!(
            catalog
                .find_in(AssetView::Tag("Color study".to_owned()), "")
                .await
                .unwrap()
                .len(),
            1
        );

        catalog.delete_folder(first).await.unwrap();
        catalog.set_trashed(ids[1], false).await.unwrap();
        assert_eq!(
            catalog
                .folders()
                .await
                .unwrap()
                .iter()
                .map(|folder| folder.id)
                .collect::<Vec<_>>(),
            [second]
        );
        assert_eq!(
            catalog
                .find_in(AssetView::Folder(second), "")
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(catalog.find("").await.unwrap(), assets);
        assert_eq!(fs::read(fixture.0.join("one.png")).unwrap(), PNG);
        catalog
            .edit_metadata(
                &ids[..1],
                &MetadataEdit::Tags {
                    add: vec![],
                    remove: vec!["Color study".to_owned()],
                },
            )
            .await
            .unwrap();
        assert_eq!(
            catalog.organization().await.unwrap().counts[&AssetView::Untagged],
            1
        );
        catalog.close().await.unwrap();
    });
}

#[test]
fn adding_multiple_tags_preserves_existing_assignments_after_reopening() {
    use assetcraft::MetadataEdit;

    let fixture = Fixture::new();
    fixture.create();
    fixture.write("photo.png", PNG);
    fixture.ok(&["add", CATALOG, "photo.png"]);
    let path = fixture.0.join(CATALOG);
    block_on(async {
        let mut catalog = Catalog::open(&path).await.unwrap();
        let id = catalog.find("").await.unwrap()[0].id;
        for tag in ["Existing", "Editorial", "Palette"] {
            catalog.create_tag(tag).await.unwrap();
        }
        for tags in [
            vec!["Existing".to_owned()],
            vec!["Editorial".to_owned(), "Palette".to_owned()],
        ] {
            catalog
                .edit_metadata(
                    &[id],
                    &MetadataEdit::Tags {
                        add: tags,
                        remove: vec![],
                    },
                )
                .await
                .unwrap();
        }
        catalog.close().await.unwrap();

        let mut reopened = Catalog::open(&path).await.unwrap();
        let organization = reopened.organization().await.unwrap();
        assert_eq!(
            organization.assets[&id].tags,
            ["Editorial", "Existing", "Palette"]
        );
        reopened.close().await.unwrap();
    });
}
