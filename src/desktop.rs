#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod desktop_audio;
mod desktop_comments;
mod desktop_documents;
mod desktop_extra;
mod desktop_models;
#[cfg(target_os = "linux")]
mod desktop_movie_audio;
mod desktop_player;
mod desktop_recents;
mod desktop_scene_render;
mod desktop_scene_viewer;
mod desktop_sound;
mod desktop_textures;
mod desktop_thumbnails;
mod desktop_transport;
mod desktop_video;
mod desktop_viewer;

use assetcraft::{
    Added, Appearance, Asset, AssetState, AssetView, Catalog, Error, Folder, Format, Library,
    MetadataEdit, Organization,
};
use craft::behavior::{
    ColumnKey, ColumnWidth, DisplayOrder, EditPolicy, Flow, IconId, ItemSource, SizeHint, keyed,
    placement::Defaults, selection::Selection,
};
use craft::dialogs::service::{FilePanels, FileService};
use craft::egui::{Align, Context, Frame, Layout, Margin, Rect, Ui, UiBuilder, ViewportCommand};
use craft::host::queue::{JobQueue, Priority, QueueOptions};
use craft::host::{AppInfo, Host, Wake};
use craft::host::{PickId, PickRequest, Picked};
use craft::i18n::Word;
use craft::theme::ControlSize;
use craft::theme::variant::{Tone, Variant};
use craft::theme::{LabelColor, ModeChoice, ThemeId, TypeRole};
use craft::ui::buttons::{Button, IconButton};
use craft::ui::choice::{Checkbox, Rating, Segmented, Switch};
use craft::ui::contract::{
    ActivateOn, Bind, CollectionEvent, ColumnSpec, DropAt, DropKind, DropPlace, Entry, Pad,
    PaneEdge, Props, RowHeight, RowPreview, Scroll, SelectMode, ShellPane, Show, ShowCollection,
    ShowContainer, Sizing, Trigger,
};
use craft::ui::dialog::{Dialog, DialogHost};
use craft::ui::display::{Divider, Elide, Icon, Picture, Shade, Text};
use craft::ui::layout_components::{Row as ControlRow, Stack};
use craft::ui::list::{
    List, ListRow, ListWords, OpenSet, ThumbAspect, TreeLeaves, label_color_label,
};
use craft::ui::menu::{Menu, MenuItem};
use craft::ui::notices::Notice;
use craft::ui::popover::DropVeil;
use craft::ui::primitives::{ContentPainterCx, IconSpec, TextRun};
use craft::ui::runtime::access::{Politeness, announce};
use craft::ui::runtime::drag::{self, OsFiles};
use craft::ui::runtime::env::{Env, RootEnv, WindowKey};
use craft::ui::runtime::focus::FocusRequest;
use craft::ui::runtime::frame::FrameInput;
use craft::ui::sections::{DialogRole, FooterButton};
use craft::ui::shell::app::{AppSpec, ShellApp, run};
use craft::ui::shell::embed::CloseDecision;
use craft::ui::shell::{
    AppShell, RecentSort, RecentView, ShellRegion, StartAction, StartLayout, StartScreen, TitleBar,
};
use craft::ui::style::{DataColor, GlyphRole, Ink, StyleCx, UiTokens};
use craft::ui::table::Table;
use craft::ui::text::TextField;
use craft::ui::tile_grid::{
    CaptionPlace, GridEvent, GridZoom, Tile, TileGrid, TileGroup, TileStatus, TileZoom, ZoomStep,
};
use craft::ui::value_drag::Slider;
use craft::window::Prefix;
use craft::window::policy::{
    Backend, Frame as WindowFrame, Os, Runtime, TitleBarPref, WindowKind, decide, decide_for,
};
use desktop_comments::{CommentChange, CommentEdit};
use desktop_recents::RecentLibrary;
use desktop_thumbnails::{PreviewSurface, Previews};
use desktop_viewer::{Viewer, ViewerAction};
use futures_executor::block_on;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

const APP_ID: &str = "assetcraft";

enum Pick {
    Open,
    Create(LibraryDraft),
    Add,
}

enum Operation {
    Open(PathBuf),
    Create(PathBuf, LibraryDraft),
    Add(PathBuf, Vec<PathBuf>, Browse),
    Find(PathBuf, Browse),
    CreateFolder(PathBuf, Folder),
    SetFolderAppearance(PathBuf, Uuid, Appearance),
    SetLibraryAppearance(PathBuf, Appearance),
    Edit(PathBuf, Uuid, AssetEdit, Browse),
    Metadata(PathBuf, Vec<Uuid>, MetadataEdit, Browse),
    DeleteFolder(PathBuf, Uuid, Browse),
    CreateTag(PathBuf, String),
    SaveNote(PathBuf, Uuid, String),
    EditComment(PathBuf, Uuid, CommentEdit),
}

enum AssetEdit {
    Favorite(bool),
    Trash(bool),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileFilter {
    Video,
    Audio,
    Model,
    Cad,
    Exact(Format),
}

impl FileFilter {
    fn matches(self, format: Format) -> bool {
        match self {
            Self::Video => format.is_video(),
            Self::Audio => format.is_audio(),
            Self::Model => format.is_model(),
            Self::Cad => format.is_cad(),
            Self::Exact(expected) => expected == format,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Filters {
    format: Option<FileFilter>,
    available: Option<bool>,
    tag: Option<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum AssetOrder {
    #[default]
    ViewDefault,
    Name,
    NameDescending,
    Newest,
    Oldest,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Browse {
    view: AssetView,
    query: String,
    filters: Filters,
    order: AssetOrder,
    shuffle: Uuid,
    hide_subfolders: bool,
}

impl Browse {
    fn folder(&self) -> Option<Uuid> {
        match self.view {
            AssetView::Folder(id) => Some(id),
            _ => None,
        }
    }
}

#[derive(Clone, Default)]
struct LibraryDraft {
    name: String,
    appearance: Appearance,
}

struct DraggedAssets(Vec<Uuid>);

struct FolderDraft {
    folder: Folder,
    focus: bool,
}

const FOLDER_NAME: ColumnKey = ColumnKey::new("folder-name");
const DRAFT_FOLDER: Uuid = Uuid::nil();
const APPEARANCE_ICONS: &[(&str, &str)] = &[
    ("folder", "Folder"),
    ("image", "Image"),
    ("star", "Star"),
    ("bookmark", "Bookmark"),
    ("palette", "Palette"),
    ("brush", "Brush"),
    ("file", "Document"),
    ("play", "Play"),
    ("scan", "Scan"),
    ("pen_tool", "Vector"),
    ("text", "Typography"),
    ("layers", "Layers"),
    ("swatch_book", "Swatches"),
    ("diamond", "Geometry"),
    ("ruler", "Measurement"),
    ("chart_line", "Signal"),
    ("keyframe_linear", "Keyframe"),
    ("globe", "World"),
];

fn appearance_icon(appearance: &Appearance) -> IconSpec<'static> {
    let id = APPEARANCE_ICONS
        .iter()
        .find(|(id, _)| *id == appearance.icon)
        .map_or("folder", |(id, _)| *id);
    let icon = IconSpec::svg(IconId::new(id));
    let color = LabelColor::ALL
        .into_iter()
        .find(|color| Some(color.name()) == appearance.color.as_deref());
    match color {
        Some(color) => icon.data_color(DataColor::new(
            craft::theme::Srgba::from(color.swatch()).into(),
        )),
        None => icon,
    }
}

fn appearance_menu(ui: &mut Ui, current: &Appearance) -> Option<Appearance> {
    let mut appearance = current.clone();
    Menu::submenu_row(ui, Word::plain("Icon"), |ui, _| {
        for &(id, name) in APPEARANCE_ICONS {
            if MenuItem::new(Word::plain(name))
                .icon(IconSpec::svg(IconId::new(id)))
                .check(current.icon == id)
                .show(ui)
                .ran
            {
                appearance.icon = id.to_owned();
            }
        }
    });
    Menu::submenu_row(ui, Word::plain("Color"), |ui, _| {
        let words = ListWords::default();
        for color in std::iter::once(None).chain(LabelColor::ALL.into_iter().map(Some)) {
            let name = color.map(LabelColor::name);
            let icon = color.map_or_else(IconSpec::empty_swatch, |color| {
                IconSpec::swatch(color.swatch().into())
            });
            if MenuItem::new(label_color_label(color, &words))
                .icon(icon)
                .check(current.color.as_deref() == name)
                .show(ui)
                .ran
            {
                appearance.color = name.map(str::to_owned);
            }
        }
    });
    (appearance != *current).then_some(appearance)
}

const NAVIGATION_ITEMS: [(Option<AssetView>, &str, &str); 7] = [
    (Some(AssetView::All), "All assets", "layout_grid"),
    (Some(AssetView::Random), "Random", "refresh"),
    (Some(AssetView::Recent), "Recently added", "clock"),
    (Some(AssetView::Favorites), "Favorites", "star"),
    (Some(AssetView::Untagged), "Untagged", "bookmark"),
    (None, "All Tags", "bookmark"),
    (Some(AssetView::Trash), "Trash", "trash"),
];

struct FolderTree<'a> {
    rows: Vec<(&'a Folder, usize)>,
    children: HashMap<Option<Uuid>, Vec<&'a Folder>>,
    open: &'a OpenSet<Uuid>,
}

impl<'a> FolderTree<'a> {
    fn new(folders: &'a [Folder], draft: Option<&'a Folder>, open: &'a OpenSet<Uuid>) -> Self {
        let mut children: HashMap<_, Vec<_>> = HashMap::new();
        for folder in folders.iter().chain(draft) {
            children.entry(folder.parent).or_default().push(folder);
        }
        let mut rows = Vec::new();
        let mut pending = Vec::new();
        if let Some(roots) = children.get(&None) {
            pending.extend(roots.iter().rev().map(|&folder| (folder, 0)));
        }
        while let Some((folder, depth)) = pending.pop() {
            rows.push((folder, depth));
            if open.is_open(&folder.id)
                && let Some(branch) = children.get(&Some(folder.id))
            {
                pending.extend(branch.iter().rev().map(|&child| (child, depth + 1)));
            }
        }
        Self {
            rows,
            children,
            open,
        }
    }
}

impl DisplayOrder<Uuid> for FolderTree<'_> {
    fn len(&self) -> usize {
        self.rows.len()
    }
    fn key(&self, at: usize) -> Uuid {
        self.rows[at].0.id
    }
    fn index_of(&self, key: &Uuid) -> Option<usize> {
        self.rows.iter().position(|(folder, _)| folder.id == *key)
    }
    fn text(&self, at: usize) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(&self.rows[at].0.name))
    }
    fn depth(&self, at: usize) -> usize {
        self.rows[at].1
    }
    fn has_children(&self, at: usize) -> bool {
        self.children.contains_key(&Some(self.key(at)))
    }
    fn open(&self, at: usize) -> bool {
        self.open.is_open(&self.key(at))
    }
}

impl ItemSource<Uuid> for FolderTree<'_> {}

struct Row {
    asset: Asset,
    available: bool,
    file: Option<FileInfo>,
    state: AssetState,
}

struct FileInfo {
    bytes: u64,
    created: Option<SystemTime>,
    modified: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GridEntry {
    Asset { id: Uuid, folder: Option<Uuid> },
    Folder(Uuid),
}

impl GridEntry {
    fn asset(self) -> Option<Uuid> {
        match self {
            Self::Asset { id, .. } => Some(id),
            Self::Folder(_) => None,
        }
    }
}

fn unique_assets(entries: &[GridEntry]) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    entries
        .iter()
        .filter_map(|entry| entry.asset())
        .filter(|id| seen.insert(*id))
        .collect()
}

#[derive(Default)]
struct AssetGrid {
    entries: Vec<GridEntry>,
    positions: HashMap<GridEntry, usize>,
    groups: Vec<TileGroup<GridEntry>>,
    visible_assets: Vec<Uuid>,
    visible_positions: HashMap<Uuid, usize>,
}

impl AssetGrid {
    fn prepare(
        rows: &[Row],
        browse: &Browse,
        folders: &[Folder],
        organization: &Organization,
        collapsed: &HashSet<Uuid>,
    ) -> Self {
        let parent = browse.folder();
        let sections = parent
            .filter(|_| !browse.hide_subfolders)
            .map_or_else(Vec::new, |parent| {
                folder_sections(folders, parent, collapsed)
            });
        let (entries, groups) = if sections.is_empty() {
            let entries = rows
                .iter()
                .map(|row| GridEntry::Asset {
                    id: row.asset.id,
                    folder: parent,
                })
                .collect::<Vec<_>>();
            (entries, Vec::new())
        } else {
            let indices: HashMap<_, _> = sections
                .iter()
                .enumerate()
                .filter(|(_, (folder, _))| !collapsed.contains(&folder.id))
                .map(|(at, (folder, _))| (folder.id, at + 1))
                .chain(parent.map(|id| (id, 0)))
                .collect();
            let mut entries = vec![Vec::new(); sections.len() + 1];
            for row in rows {
                let id = row.asset.id;
                if let Some(details) = organization.assets.get(&id) {
                    for folder in &details.folders {
                        if let Some(&at) = indices.get(folder) {
                            entries[at].push(GridEntry::Asset {
                                id,
                                folder: Some(*folder),
                            });
                        }
                    }
                } else {
                    entries[0].push(GridEntry::Asset { id, folder: parent });
                }
            }
            let mut groups = Vec::with_capacity(sections.len());
            let mut start = entries[0].len();
            for ((folder, depth), members) in sections.into_iter().zip(&entries[1..]) {
                groups.push(
                    TileGroup::new(
                        GridEntry::Folder(folder.id),
                        start,
                        members.len(),
                        &folder.name,
                    )
                    .open(!collapsed.contains(&folder.id))
                    .depth(depth)
                    .icon(IconId::new("folder")),
                );
                start += members.len();
            }
            (entries.into_iter().flatten().collect(), groups)
        };
        let positions = entries
            .iter()
            .enumerate()
            .map(|(at, key)| (*key, at))
            .collect();
        let visible_assets = unique_assets(&entries);
        let visible_positions = visible_assets
            .iter()
            .copied()
            .enumerate()
            .map(|(at, id)| (id, at))
            .collect();
        Self {
            entries,
            positions,
            groups,
            visible_assets,
            visible_positions,
        }
    }

    fn retain_selection(&self, selection: &mut Selection<GridEntry>) {
        if selection.items.is_empty() && selection.lead.is_none() && selection.anchor.is_none() {
            return;
        }
        let mut first_placements = HashMap::new();
        let mut restore = |key: GridEntry| {
            if self.positions.contains_key(&key) || self.groups.iter().any(|group| group.key == key)
            {
                return Some(key);
            }
            let id = key.asset()?;
            if first_placements.is_empty() {
                for &entry in &self.entries {
                    if let Some(id) = entry.asset() {
                        first_placements.entry(id).or_insert(entry);
                    }
                }
            }
            first_placements.get(&id).copied()
        };
        let mut seen = HashSet::new();
        selection.items = selection
            .items
            .iter()
            .copied()
            .filter_map(&mut restore)
            .filter(|key| seen.insert(*key))
            .collect();
        selection.lead = selection.lead.and_then(&mut restore);
        selection.anchor = selection.anchor.and_then(restore);
    }
}

fn folder_sections<'a>(
    folders: &'a [Folder],
    parent: Uuid,
    collapsed: &HashSet<Uuid>,
) -> Vec<(&'a Folder, usize)> {
    let mut children: HashMap<Uuid, Vec<&Folder>> = HashMap::new();
    for folder in folders {
        if let Some(above) = folder.parent {
            children.entry(above).or_default().push(folder);
        }
    }
    let mut pending: Vec<_> = children
        .remove(&parent)
        .into_iter()
        .flatten()
        .rev()
        .map(|folder| (folder, 0))
        .collect();
    let mut sections = Vec::new();
    while let Some((folder, depth)) = pending.pop() {
        if !collapsed.contains(&folder.id)
            && let Some(branch) = children.remove(&folder.id)
        {
            pending.extend(branch.into_iter().rev().map(|child| (child, depth + 1)));
        }
        sections.push((folder, depth));
    }
    sections
}

struct Assets<'a> {
    rows: &'a [Row],
    positions: &'a HashMap<Uuid, usize>,
    grid: &'a AssetGrid,
    previews: &'a Previews,
}

impl Assets<'_> {
    fn row(&self, key: &GridEntry) -> Option<&Row> {
        let at = self.positions.get(&key.asset()?)?;
        self.rows.get(*at)
    }
}

impl DisplayOrder<GridEntry> for Assets<'_> {
    fn len(&self) -> usize {
        self.grid.entries.len()
    }

    fn key(&self, at: usize) -> GridEntry {
        self.grid.entries[at]
    }

    fn index_of(&self, key: &GridEntry) -> Option<usize> {
        self.grid.positions.get(key).copied()
    }

    fn text(&self, at: usize) -> Option<Cow<'_, str>> {
        self.row(&self.key(at))
            .map(|row| Cow::Borrowed(row.asset.display_name.as_str()))
    }
}

impl ItemSource<GridEntry> for Assets<'_> {
    fn size_hint(&self, at: usize) -> SizeHint {
        self.row(&self.key(at))
            .and_then(|row| self.previews.dimensions(&row.asset.path))
            .map_or(SizeHint::Auto, |size| {
                SizeHint::Aspect(size.width as f32 / size.height.max(1) as f32)
            })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DisplayMode {
    #[default]
    Grid,
    Justified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sidebar {
    Navigation,
    Inspector,
}

struct OpenLibrary {
    path: PathBuf,
    library: Library,
}

enum Update {
    Opened(
        OpenLibrary,
        Vec<Row>,
        Vec<Folder>,
        Organization,
        Result<Vec<RecentLibrary>, craft::settings::SettingsError>,
    ),
    Found(Browse, Vec<Row>, Vec<Folder>, Organization),
    FolderCreated(Uuid, Option<Uuid>, Vec<Folder>),
    FolderAppearanceChanged(Uuid, Appearance),
    LibraryAppearanceChanged(
        PathBuf,
        Library,
        Result<Vec<RecentLibrary>, craft::settings::SettingsError>,
    ),
    Edited(&'static str, Result<(Vec<Row>, Organization), Error>),
    Added(Vec<Added>, Result<(Vec<Row>, Organization), Error>),
    FolderDeleted(Vec<Folder>, Browse, Vec<Row>, Organization),
    TagCreated(Organization),
    NoteSaved(PathBuf, Uuid, String, Result<(), Error>),
    CommentChanged(Uuid, Option<i64>, Result<CommentChange, Error>),
}

impl Operation {
    async fn run(self) -> Result<Update, Error> {
        match self {
            Self::Open(path) => {
                let path = path.canonicalize().unwrap_or(path);
                let mut catalog = Catalog::open(&path).await?;
                let library = catalog.library().clone();
                let rows = lookup(&mut catalog, &Browse::default()).await?;
                let folders = catalog.folders().await?;
                let organization = catalog.organization().await?;
                catalog.close().await?;
                let recents = desktop_recents::remember(&path, &library);
                Ok(Update::Opened(
                    OpenLibrary { path, library },
                    rows,
                    folders,
                    organization,
                    recents,
                ))
            }
            Self::Create(path, draft) => {
                let catalog =
                    Catalog::create_with_appearance(&path, &draft.name, &draft.appearance).await?;
                let path = path.canonicalize().unwrap_or(path);
                let library = catalog.library().clone();
                catalog.close().await?;
                let recents = desktop_recents::remember(&path, &library);
                Ok(Update::Opened(
                    OpenLibrary { path, library },
                    Vec::new(),
                    Vec::new(),
                    Organization::default(),
                    recents,
                ))
            }
            Self::Add(path, paths, browse) => {
                let mut catalog = Catalog::open(&path).await?;
                let outcomes = catalog.add_to_folder(&paths, browse.folder()).await?;
                let rows = refresh(&mut catalog, &browse).await;
                catalog.close().await?;
                Ok(Update::Added(outcomes, rows))
            }
            Self::Find(path, browse) => {
                let mut catalog = Catalog::open(&path).await?;
                let (assets, folders, organization) = catalog
                    .browse_snapshot(
                        browse.view.clone(),
                        &browse.query,
                        browse.filters.tag.as_deref(),
                        !browse.hide_subfolders,
                    )
                    .await?;
                catalog.close().await?;
                let rows = browse_rows(assets, &browse);
                Ok(Update::Found(browse, rows, folders, organization))
            }
            Self::CreateFolder(path, folder) => {
                let mut catalog = Catalog::open(&path).await?;
                let id = catalog
                    .create_folder_with_appearance(folder.parent, &folder.name, &folder.appearance)
                    .await?;
                let folders = catalog.folders().await?;
                catalog.close().await?;
                Ok(Update::FolderCreated(id, folder.parent, folders))
            }
            Self::SetFolderAppearance(path, id, appearance) => {
                let mut catalog = Catalog::open(&path).await?;
                catalog.set_folder_appearance(id, &appearance).await?;
                catalog.close().await?;
                Ok(Update::FolderAppearanceChanged(id, appearance))
            }
            Self::SetLibraryAppearance(path, appearance) => {
                let mut catalog = Catalog::open(&path).await?;
                catalog.set_library_appearance(appearance).await?;
                let library = catalog.library().clone();
                catalog.close().await?;
                let recents = desktop_recents::remember(&path, &library);
                Ok(Update::LibraryAppearanceChanged(path, library, recents))
            }
            Self::CreateTag(path, name) => {
                let mut catalog = Catalog::open(&path).await?;
                catalog.create_tag(&name).await?;
                let organization = catalog.organization().await?;
                catalog.close().await?;
                Ok(Update::TagCreated(organization))
            }
            Self::SaveNote(path, id, text) => {
                let result = async {
                    let mut catalog = Catalog::open(&path).await?;
                    catalog
                        .edit_metadata(&[id], &MetadataEdit::Notes(text.clone()))
                        .await?;
                    catalog.close().await
                }
                .await;
                Ok(Update::NoteSaved(path, id, text, result))
            }
            Self::EditComment(path, asset, edit) => {
                let retry = match &edit {
                    CommentEdit::Move(id, _) => Some(*id),
                    _ => None,
                };
                let result = async {
                    let mut catalog = Catalog::open(&path).await?;
                    let change = match edit {
                        CommentEdit::Add(point, body) => CommentChange::Upsert(
                            catalog.add_point_comment(asset, point, &body).await?,
                        ),
                        CommentEdit::Text(id, body) => CommentChange::Upsert(
                            catalog.edit_point_comment(asset, id, &body).await?,
                        ),
                        CommentEdit::Move(id, point) => CommentChange::Upsert(
                            catalog.move_point_comment(asset, id, point).await?,
                        ),
                        CommentEdit::Delete(id) => {
                            catalog.delete_point_comment(asset, id).await?;
                            CommentChange::Deleted(id)
                        }
                    };
                    Ok(change)
                }
                .await;
                Ok(Update::CommentChanged(asset, retry, result))
            }
            Self::Metadata(path, assets, edit, browse) => {
                let mut catalog = Catalog::open(&path).await?;
                catalog.edit_metadata(&assets, &edit).await?;
                let result = refresh(&mut catalog, &browse).await;
                catalog.close().await?;
                Ok(Update::Edited("Asset details saved", result))
            }
            Self::DeleteFolder(path, id, mut browse) => {
                let mut catalog = Catalog::open(&path).await?;
                catalog.delete_folder(id).await?;
                let folders = catalog.folders().await?;
                if let Some(id) = browse.folder()
                    && !folders.iter().any(|folder| folder.id == id)
                {
                    browse.view = AssetView::All;
                    browse.query.clear();
                }
                let (rows, organization) = refresh(&mut catalog, &browse).await?;
                catalog.close().await?;
                Ok(Update::FolderDeleted(folders, browse, rows, organization))
            }
            Self::Edit(path, id, edit, browse) => {
                let mut catalog = Catalog::open(&path).await?;
                let message = match edit {
                    AssetEdit::Favorite(value) => {
                        catalog.set_favorite(id, value).await?;
                        if value {
                            "Added to favorites"
                        } else {
                            "Removed from favorites"
                        }
                    }
                    AssetEdit::Trash(value) => {
                        catalog.set_trashed(id, value).await?;
                        if value {
                            "Moved to trash"
                        } else {
                            "Restored from trash"
                        }
                    }
                };
                let rows = refresh(&mut catalog, &browse).await;
                catalog.close().await?;
                Ok(Update::Edited(message, rows))
            }
        }
    }
}

async fn refresh(
    catalog: &mut Catalog,
    browse: &Browse,
) -> Result<(Vec<Row>, Organization), Error> {
    Ok((
        lookup(catalog, browse).await?,
        catalog.organization().await?,
    ))
}

async fn lookup(catalog: &mut Catalog, browse: &Browse) -> Result<Vec<Row>, Error> {
    let assets = catalog
        .find_filtered(
            browse.view.clone(),
            &browse.query,
            browse.filters.tag.as_deref(),
            !browse.hide_subfolders,
        )
        .await?;
    Ok(browse_rows(assets, browse))
}

fn browse_rows(assets: Vec<(Asset, AssetState)>, browse: &Browse) -> Vec<Row> {
    let mut rows: Vec<_> = assets
        .into_iter()
        .filter(|(asset, _)| {
            browse
                .filters
                .format
                .is_none_or(|filter| filter.matches(asset.format))
        })
        .map(|(asset, state)| {
            let file = std::fs::metadata(&asset.path)
                .ok()
                .filter(|file| file.is_file())
                .map(|file| FileInfo {
                    bytes: file.len(),
                    created: file.created().ok(),
                    modified: file.modified().ok(),
                });
            Row {
                available: file.is_some(),
                file,
                state,
                asset,
            }
        })
        .filter(|row| {
            browse
                .filters
                .available
                .is_none_or(|available| available == row.available)
        })
        .collect();
    match browse.order {
        AssetOrder::ViewDefault if browse.view == AssetView::Random => {
            rows.sort_by_cached_key(|row| {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                (browse.shuffle, row.asset.id).hash(&mut hash);
                (hash.finish(), row.asset.id)
            });
        }
        AssetOrder::ViewDefault => {}
        AssetOrder::Name => rows.sort_by(|a, b| a.asset.display_name.cmp(&b.asset.display_name)),
        AssetOrder::NameDescending => {
            rows.sort_by(|a, b| b.asset.display_name.cmp(&a.asset.display_name))
        }
        AssetOrder::Newest => {
            rows.sort_by_key(|row| std::cmp::Reverse(row.asset.imported_at_unix_ms))
        }
        AssetOrder::Oldest => rows.sort_by_key(|row| row.asset.imported_at_unix_ms),
    }
    rows
}

fn filter_menu(ui: &mut Ui, filters: &mut Filters, tags: &BTreeMap<String, u64>) {
    Menu::submenu_row(ui, Word::plain("File type"), |ui, _| {
        let groups = [
            (None, "All formats"),
            (Some(FileFilter::Video), "Video"),
            (Some(FileFilter::Audio), "Audio"),
            (Some(FileFilter::Model), "3D"),
            (Some(FileFilter::Cad), "CAD"),
        ];
        let formats = Format::ALL
            .iter()
            .copied()
            .map(|format| (Some(FileFilter::Exact(format)), format.name()));
        for (filter, label) in groups.into_iter().chain(formats) {
            if MenuItem::new(Word::plain(label))
                .check(filters.format == filter)
                .show(ui)
                .ran
            {
                filters.format = filter;
            }
        }
    });
    Divider::horizontal().show(ui);
    MenuItem::heading(Word::plain("Original file")).show(ui);
    for (available, text) in [
        (None, "Any availability"),
        (Some(true), "Available"),
        (Some(false), "Missing"),
    ] {
        if MenuItem::new(Word::plain(text))
            .check(filters.available == available)
            .show(ui)
            .ran
        {
            filters.available = available;
        }
    }
    if !tags.is_empty() {
        Divider::horizontal().show(ui);
        Menu::submenu_row(ui, Word::plain("Tags"), |ui, _| {
            if MenuItem::new(Word::plain("Any tag"))
                .check(filters.tag.is_none())
                .show(ui)
                .ran
            {
                filters.tag = None;
            }
            for tag in tags.keys() {
                if MenuItem::new(tag.clone())
                    .check(filters.tag.as_ref() == Some(tag))
                    .show(ui)
                    .ran
                {
                    filters.tag = Some(tag.clone());
                }
            }
        });
    }
    if *filters != Filters::default() {
        Divider::horizontal().show(ui);
        if MenuItem::new(Word::plain("Clear filters")).show(ui).ran {
            *filters = Filters::default();
        }
    }
}

fn order_menu(ui: &mut Ui, order: &mut AssetOrder) {
    for (value, name) in [
        (AssetOrder::ViewDefault, "Default order"),
        (AssetOrder::Name, "Name A–Z"),
        (AssetOrder::NameDescending, "Name Z–A"),
        (AssetOrder::Newest, "Newest added"),
        (AssetOrder::Oldest, "Oldest added"),
    ] {
        if MenuItem::new(Word::plain(name))
            .check(*order == value)
            .show(ui)
            .ran
        {
            *order = value;
        }
    }
}

struct Status {
    tone: Tone,
    summary: String,
    details: Vec<String>,
}

struct PendingNote {
    text: String,
    due: Instant,
}

struct AddTagsDraft {
    assets: Vec<Uuid>,
    available: Vec<String>,
    selected: BTreeSet<String>,
}

const NOTE_DELAY: Duration = Duration::from_millis(400);

struct Desktop {
    host: Arc<Host>,
    environment: RootEnv,
    panels: FilePanels,
    dialogs: DialogHost,
    queue: JobQueue<(), Result<Update, Error>>,
    pending_pick: Option<(PickId, Pick)>,
    work: Option<Word>,
    library: Option<OpenLibrary>,
    library_draft: Option<LibraryDraft>,
    recents: Vec<RecentLibrary>,
    query: String,
    browse: Browse,
    folders: Vec<Folder>,
    organization: Organization,
    notes_draft: Option<(Uuid, String)>,
    pending_notes: HashMap<(PathBuf, Uuid), PendingNote>,
    closing: bool,
    close_note_dialog: bool,
    tag_draft: Option<String>,
    add_tags_draft: Option<AddTagsDraft>,
    delete_folder: Option<Uuid>,
    open_folders: OpenSet<Uuid>,
    collapsed_sections: HashSet<Uuid>,
    folder_draft: Option<FolderDraft>,
    rows: Vec<Row>,
    positions: HashMap<Uuid, usize>,
    asset_grid: AssetGrid,
    status: Option<Status>,
    announcement: Option<String>,
    previews: Previews,
    viewer: Viewer,
    restore_grid_focus: bool,
    selection: Selection<GridEntry>,
    display: DisplayMode,
    zoom: f64,
    zoom_limit: f64,
    show_details: bool,
    navigation_hidden: bool,
    inspector_hidden: bool,
    sidebar_focus: Option<Sidebar>,
}

impl Desktop {
    fn new(ctx: &Context) -> Self {
        let previews = Previews::new(ctx);
        let ctx = ctx.clone();
        let wake = Wake::new(move || ctx.request_repaint());
        let (files, panels) = FileService::new(wake.clone());
        let host = Arc::new(
            Host::builder(AppInfo {
                id: APP_ID,
                name: "AssetCraft",
                version: env!("CARGO_PKG_VERSION"),
            })
            .files(Arc::new(files))
            .build(),
        );
        let options = QueueOptions {
            lanes: 1,
            ..QueueOptions::default()
        };
        let queue = JobQueue::new(host.jobs_shared().clone(), wake.clone(), options);
        let viewer = Viewer::new(&host, wake);
        Self {
            host,
            environment: RootEnv::default().mode(ModeChoice::System).theme(
                ThemeId::parse("craft.graphite").unwrap_or_else(|_| ThemeId::suite_default()),
            ),
            panels,
            dialogs: DialogHost::new(),
            queue,
            pending_pick: None,
            work: None,
            library: None,
            library_draft: None,
            recents: desktop_recents::load(),
            query: String::new(),
            browse: Browse::default(),
            folders: Vec::new(),
            organization: Organization::default(),
            notes_draft: None,
            pending_notes: HashMap::new(),
            closing: false,
            close_note_dialog: false,
            tag_draft: None,
            add_tags_draft: None,
            delete_folder: None,
            open_folders: OpenSet::new(),
            collapsed_sections: HashSet::new(),
            folder_draft: None,
            rows: Vec::new(),
            positions: HashMap::new(),
            asset_grid: AssetGrid::default(),
            status: None,
            announcement: None,
            previews,
            viewer,
            restore_grid_focus: false,
            selection: Selection::default(),
            display: DisplayMode::default(),
            zoom: TileZoom::Ladder.default_value(),
            zoom_limit: 5.0,
            show_details: false,
            navigation_hidden: false,
            inspector_hidden: false,
            sidebar_focus: None,
        }
    }

    fn idle(&self) -> bool {
        self.pending_pick.is_none() && self.work.is_none()
    }

    fn pick(&mut self, kind: Pick, request: PickRequest) {
        if self.idle() {
            self.pending_pick = Some((self.host.files().pick(request), kind));
        }
    }

    fn submit(&mut self, operation: Operation, text: Word) {
        self.status = None;
        self.announcement = Some(text.english.to_owned());
        self.work = Some(text);
        self.queue
            .submit((), Priority::VISIBLE, move |_| block_on(operation.run()));
    }

    fn receive_pick(&mut self, ctx: &Context) {
        let Some((id, _)) = &self.pending_pick else {
            return;
        };
        let Some(result) = self.host.files().take_answer(*id) else {
            return;
        };
        let Some((_, pick)) = self.pending_pick.take() else {
            return;
        };
        ctx.request_repaint();
        match result {
            Picked::Cancelled => {}
            Picked::Unavailable(message) | Picked::Failed(message) => self.failure(message),
            picked => match pick {
                Pick::Open => {
                    if let Some(path) = picked.first_path() {
                        self.submit(Operation::Open(path), Word::plain("Opening library…"));
                    }
                }
                Pick::Create(draft) => {
                    if let Some(path) = picked.first_path() {
                        self.submit(
                            Operation::Create(path, draft),
                            Word::plain("Creating library…"),
                        );
                    }
                }
                Pick::Add => {
                    let paths = picked.paths();
                    if let Some(library) = &self.library
                        && !paths.is_empty()
                    {
                        self.submit(
                            Operation::Add(library.path.clone(), paths, self.browse.clone()),
                            Word::plain("Adding files…"),
                        );
                    }
                }
            },
        }
    }

    fn receive_work(&mut self) {
        let Some(finished) = self.queue.try_recv() else {
            return;
        };
        self.work = None;
        match finished.result {
            Ok(Update::Opened(library, rows, folders, organization, recents)) => {
                self.library = Some(library);
                self.previews.clear();
                self.viewer.close();
                self.restore_grid_focus = false;
                self.selection = Selection::default();
                self.library_draft = None;
                self.query.clear();
                self.browse = Browse::default();
                self.folders = folders;
                self.organization = organization;
                self.notes_draft = None;
                self.tag_draft = None;
                self.add_tags_draft = None;
                self.delete_folder = None;
                self.open_folders = OpenSet::new();
                self.collapsed_sections.clear();
                self.folder_draft = None;
                self.replace_rows(rows);
                self.announcement = Some(Word::plain("Library open").english.to_owned());
                self.receive_recents(recents);
            }
            Ok(Update::Found(browse, rows, folders, organization)) => {
                self.folders = folders;
                self.organization = organization;
                if self.browse.view != browse.view {
                    self.query.clone_from(&browse.query);
                    if let Some(id) = browse.folder() {
                        self.open_folder_ancestors(id);
                    }
                    if self.viewer.asset().is_some() {
                        self.viewer.close();
                        self.restore_grid_focus = true;
                    }
                }
                self.browse = browse;
                self.replace_rows(rows);
                self.announcement = Some(format!("{} results", self.rows.len()));
            }
            Ok(Update::FolderCreated(id, parent, folders)) => {
                self.folders = folders;
                self.folder_draft = None;
                if let Some(parent) = parent {
                    self.open_folders.set(&parent, true);
                }
                self.browse = Browse {
                    view: AssetView::Folder(id),
                    query: String::new(),
                    hide_subfolders: false,
                    ..self.browse.clone()
                };
                self.query.clear();
                self.replace_rows(Vec::new());
                self.announcement = Some("Folder created".to_owned());
            }
            Ok(Update::FolderAppearanceChanged(id, appearance)) => {
                if let Some(folder) = self.folders.iter_mut().find(|folder| folder.id == id) {
                    folder.appearance = appearance;
                }
                self.announcement = Some("Folder appearance saved".to_owned());
            }
            Ok(Update::LibraryAppearanceChanged(path, library, recents)) => {
                for entry in &mut self.recents {
                    if entry.path() == path {
                        entry.appearance = library.appearance.clone();
                    }
                }
                if let Some(open) = &mut self.library
                    && open.path == path
                {
                    open.library = library;
                }
                self.announcement = Some("Library appearance saved".to_owned());
                self.receive_recents(recents);
            }
            Ok(Update::TagCreated(organization)) => {
                self.organization = organization;
                if let Some(draft) = &mut self.tag_draft {
                    draft.clear();
                }
                self.announcement = Some("Tag created".to_owned());
            }
            Ok(Update::CommentChanged(asset, retry, result)) => {
                let message = match &result {
                    Ok(CommentChange::Deleted(_)) => "Comment deleted",
                    Ok(CommentChange::Upsert(_)) if retry.is_some() => "Comment moved",
                    Ok(CommentChange::Upsert(_)) => "Comment saved",
                    Err(_) => {
                        self.closing = false;
                        "Could not apply comment change"
                    }
                };
                self.announcement = Some(Word::plain(message).english.to_owned());
                self.viewer.comment_changed(asset, retry, result);
            }
            Ok(Update::NoteSaved(path, id, text, result)) => {
                let key = (path, id);
                match result {
                    Ok(()) => {
                        if self
                            .pending_notes
                            .get(&key)
                            .is_some_and(|note| note.text == text)
                        {
                            self.pending_notes.remove(&key);
                        }
                        if self
                            .library
                            .as_ref()
                            .is_some_and(|library| library.path == key.0)
                        {
                            self.organization.assets.entry(id).or_default().notes = text;
                        }
                    }
                    Err(error) => {
                        if let Some(note) = self.pending_notes.get_mut(&key) {
                            note.due = Instant::now() + Duration::from_secs(2);
                        }
                        if self.closing {
                            self.closing = false;
                            self.close_note_dialog = true;
                        }
                        self.failure(format!("Could not save note: {error}"));
                    }
                }
            }
            Ok(Update::FolderDeleted(folders, browse, rows, organization)) => {
                self.folders = folders;
                self.query.clone_from(&browse.query);
                self.browse = browse;
                self.organization = organization;
                self.replace_rows(rows);
                self.announcement = Some("Folder deleted".to_owned());
            }
            Ok(Update::Edited(message, rows)) => match rows {
                Ok((rows, organization)) => {
                    self.organization = organization;
                    self.replace_rows(rows);
                    self.announcement = Some(message.to_owned());
                }
                Err(error) => {
                    self.set_status(Tone::Warn, message.to_owned(), vec![error.to_string()])
                }
            },
            Ok(Update::Added(outcomes, rows)) => self.report_import(outcomes, rows),
            Err(error) => self.failure(error.to_string()),
        }
    }

    fn receive_recents(
        &mut self,
        recents: Result<Vec<RecentLibrary>, craft::settings::SettingsError>,
    ) {
        match recents {
            Ok(recents) => self.recents = recents,
            Err(warning) => self.set_status(
                Tone::Warn,
                "Could not save recent libraries".to_owned(),
                vec![warning.to_string()],
            ),
        }
    }

    fn set_status(&mut self, tone: Tone, summary: String, details: Vec<String>) {
        self.announcement = Some(summary.clone());
        self.status = Some(Status {
            tone,
            summary,
            details,
        });
    }

    fn failure(&mut self, message: String) {
        self.set_status(Tone::Bad, message, Vec::new());
    }

    fn report_import(
        &mut self,
        outcomes: Vec<Added>,
        rows: Result<(Vec<Row>, Organization), Error>,
    ) {
        let mut imported = 0;
        let mut existing = 0;
        let mut details = Vec::new();
        for outcome in outcomes {
            match outcome {
                Added::Imported(_) => imported += 1,
                Added::Existing(_) => existing += 1,
                Added::Rejected { path, error } => {
                    details.push(format!("{}: {error}", path.display()))
                }
            }
        }
        let rejected = details.len();
        match rows {
            Ok((rows, organization)) => {
                self.organization = organization;
                self.replace_rows(rows);
            }
            Err(error) => details.push(format!("Could not refresh the list: {error}")),
        }
        let tone = if details.is_empty() {
            Tone::Good
        } else {
            Tone::Warn
        };
        self.set_status(
            tone,
            format!("Added {imported} · Already present {existing} · Rejected {rejected}"),
            details,
        );
    }

    fn replace_rows(&mut self, rows: Vec<Row>) {
        for row in &rows {
            self.previews.observe(
                &row.asset.path,
                row.file
                    .as_ref()
                    .map(|file| craft::browse::thumbs::FileStat {
                        len: file.bytes,
                        modified: file.modified,
                    }),
            );
        }
        self.positions = rows
            .iter()
            .enumerate()
            .map(|(at, row)| (row.asset.id, at))
            .collect();
        self.rows = rows;
        self.refresh_grid();
    }

    fn refresh_grid(&mut self) {
        let asset_grid = AssetGrid::prepare(
            &self.rows,
            &self.browse,
            &self.folders,
            &self.organization,
            &self.collapsed_sections,
        );
        if self
            .viewer
            .asset()
            .is_some_and(|id| !asset_grid.visible_positions.contains_key(&id))
        {
            self.viewer.close();
            self.restore_grid_focus = true;
        }
        asset_grid.retain_selection(&mut self.selection);
        if self
            .notes_draft
            .as_ref()
            .is_some_and(|(id, _)| unique_assets(&self.selection.items).as_slice() != [*id])
        {
            self.notes_draft = None;
        }
        self.asset_grid = asset_grid;
    }

    fn open_picker(&mut self) {
        self.pick(
            Pick::Open,
            PickRequest::open("open-library").title("Open library"),
        );
    }

    fn sidebar_button(&mut self, ui: &mut Ui, sidebar: Sidebar) {
        let compact = match sidebar {
            Sidebar::Navigation => &mut self.navigation_hidden,
            Sidebar::Inspector => &mut self.inspector_hidden,
        };
        let label = match (sidebar, *compact) {
            (Sidebar::Navigation, false) => "Hide navigation",
            (Sidebar::Navigation, true) => "Show navigation",
            (Sidebar::Inspector, false) => "Hide inspector",
            (Sidebar::Inspector, true) => "Show inspector",
        };
        let icon = match sidebar {
            Sidebar::Navigation => "panel_left",
            Sidebar::Inspector => "panel_right",
        };
        let mut button = chrome_icon(icon, label);
        if self.sidebar_focus == Some(sidebar) {
            button = button.request_focus(FocusRequest::Take);
            self.sidebar_focus = None;
        }
        if button.show(ui).clicked() {
            *compact = !*compact;
            self.sidebar_focus = Some(sidebar);
            ui.ctx().request_repaint();
        }
    }

    fn add_button(&mut self, ui: &mut Ui) {
        if chrome_icon("plus", "Add files")
            .enabled(self.can_import())
            .show(ui)
            .clicked()
        {
            self.pick(
                Pick::Add,
                PickRequest::open_many("add-files").title("Add files"),
            );
        }
    }

    fn can_import(&self) -> bool {
        self.idle() && self.library.is_some() && self.browse.view != AssetView::Trash
    }

    fn navigation(&mut self, ui: &mut Ui) {
        let source = keyed(&NAVIGATION_ITEMS, |(view, _, _)| view.clone())
            .with_text(|(_, name, _)| Cow::Borrowed(*name));
        let current = Some(self.browse.view.clone());
        let selection = if NAVIGATION_ITEMS.iter().any(|(view, _, _)| *view == current) {
            Selection::single(current)
        } else {
            Selection::default()
        };
        let out = List::new(Word::plain("Library views"))
            .activate(ActivateOn::Click)
            .selection(Bind::given(selection))
            .sizing(Sizing::FillWidth)
            .natural_height(true)
            .show_items(ui, &source, &mut |view, row| {
                let (_, name, icon) = NAVIGATION_ITEMS[row.at];
                let item = ListRow::new(Word::plain(name)).icon(IconSpec::svg(IconId::new(icon)));
                if let Some(view) = view {
                    item.trailing(self.count(view).to_string())
                } else {
                    item.trailing(self.organization.tags.len().to_string())
                        .enabled(self.idle())
                }
            });
        for event in out.events {
            if let CollectionEvent::Activate(view) = event {
                if let Some(view) = view {
                    self.navigate_to(view, ui.ctx());
                } else if self.idle() {
                    self.open_tag_manager();
                }
            }
        }
        ui.add_space(Env::of(ui).tokens.space.gap.section);
        ui.horizontal(|ui| {
            Text::new(Word::plain("Folders"))
                .role(TypeRole::SectionCaption)
                .shade(Shade::Muted)
                .show(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let parent = self.browse.folder();
                let label = if parent.is_some() {
                    "New subfolder"
                } else {
                    "New folder"
                };
                if chrome_icon("plus", label)
                    .enabled(self.idle())
                    .show(ui)
                    .clicked()
                {
                    self.start_folder(parent);
                }
            });
        });
        self.folder_tree(ui);
    }

    fn count(&self, view: &AssetView) -> u64 {
        let view = if *view == AssetView::Random {
            &AssetView::All
        } else {
            view
        };
        self.organization.counts.get(view).copied().unwrap_or(0)
    }

    fn navigate_to(&mut self, view: AssetView, ctx: &Context) {
        if !self.idle() {
            return;
        }
        if self.browse.view == view {
            self.viewer_action(ViewerAction::Back, ctx);
        } else if let Some(library) = &self.library {
            self.folder_draft = None;
            self.submit(
                Operation::Find(
                    library.path.clone(),
                    Browse {
                        shuffle: if view == AssetView::Random {
                            Uuid::new_v4()
                        } else {
                            self.browse.shuffle
                        },
                        order: if view == AssetView::Random {
                            AssetOrder::ViewDefault
                        } else {
                            self.browse.order
                        },
                        view,
                        query: String::new(),
                        hide_subfolders: false,
                        ..self.browse.clone()
                    },
                ),
                Word::plain("Opening view…"),
            );
        }
    }

    fn open_folder_ancestors(&mut self, id: Uuid) {
        let mut current = id;
        while let Some(ancestor) = self
            .folders
            .iter()
            .find(|folder| folder.id == current)
            .and_then(|folder| folder.parent)
        {
            self.open_folders.set(&ancestor, true);
            current = ancestor;
        }
    }

    fn start_folder(&mut self, parent: Option<Uuid>) {
        if let Some(parent) = parent {
            self.open_folders.set(&parent, true);
            self.open_folder_ancestors(parent);
        }
        self.folder_draft = Some(FolderDraft {
            folder: Folder {
                id: DRAFT_FOLDER,
                parent,
                name: String::new(),
                appearance: Appearance::default(),
            },
            focus: true,
        });
    }

    fn folder_tree(&mut self, ui: &mut Ui) {
        let draft = self.folder_draft.as_ref();
        if self.folders.is_empty() && draft.is_none() {
            return;
        }
        let idle = self.idle();
        let focus = draft.is_some_and(|draft| draft.focus);
        let selection = draft
            .map(|_| DRAFT_FOLDER)
            .or_else(|| self.browse.folder())
            .map_or_else(Selection::default, Selection::single);
        let tree = FolderTree::new(
            &self.folders,
            draft.map(|draft| &draft.folder),
            &self.open_folders,
        );
        let edited = RefCell::new(None);
        let create = Cell::new(false);
        let cancel = Cell::new(false);
        let draft_icon = Cell::new(None);
        let can_drop = idle && self.browse.view != AssetView::Trash;
        let mut list = List::new(Word::plain("Custom folders"))
            .tree(true)
            .tree_leaves(TreeLeaves::Aligned)
            .natural_height(true)
            .takes_drop(DropKind::new::<DraggedAssets>(
                DropPlace::Onto,
                move |_, at| can_drop && matches!(at, DropAt::Onto(id) if *id != DRAFT_FOLDER),
            ))
            .activate(ActivateOn::Click)
            .selection(Bind::given(selection))
            .sizing(Sizing::FillWidth)
            .header(false)
            .columns(vec![ColumnSpec::new(
                FOLDER_NAME,
                "",
                ColumnWidth::fill(1.0),
            )]);
        if focus {
            list = list.reveal(DRAFT_FOLDER);
        }
        let out = list.show_items(ui, &tree, &mut |id, cx| {
            let (folder, depth) = tree.rows[cx.at];
            let title = if *id == DRAFT_FOLDER {
                if folder.parent.is_some() {
                    "New subfolder"
                } else {
                    "New folder"
                }
            } else {
                &folder.name
            };
            let row = ListRow::new(title.to_owned())
                .depth(depth)
                .cells(cx.cells());
            if *id == DRAFT_FOLDER {
                let label = Word::plain(if folder.parent.is_some() {
                    "Subfolder name"
                } else {
                    "Folder name"
                });
                let mut field = TextField::new(label, Bind::given(folder.name.clone()))
                    .placeholder(label)
                    .size(ControlSize::Sm)
                    .submits()
                    .enabled(idle);
                if focus {
                    field = field.request_focus(FocusRequest::Take);
                }
                let (edited, create, cancel) = (&edited, &create, &cancel);
                let draft_icon = &draft_icon;
                row.lead_painter(move |_, rect| draft_icon.set(Some(rect)))
                    .cell_field(FOLDER_NAME, field, move |output| {
                        let name = output.shown.as_ref().and_then(|shown| shown.same());
                        edited.replace(Some(name.map(|name| name.as_ref().clone())));
                        create.set(create.get() || output.submitted);
                        cancel.set(cancel.get() || output.escaped);
                    })
                    .trailing_field(
                        chrome_icon("check", "Create folder")
                            .enabled(idle && !folder.name.trim().is_empty()),
                        move |output| {
                            create.set(create.get() || output.clicked());
                        },
                    )
            } else {
                let row = row
                    .icon(appearance_icon(&folder.appearance))
                    .trailing(self.count(&AssetView::Folder(*id)).to_string());
                if tree.has_children(cx.at) {
                    row.group(tree.open(cx.at))
                } else {
                    row
                }
            }
        });
        let mut change = None;
        if let (Some(rect), Some(draft)) = (draft_icon.get(), draft) {
            let opener = chrome_icon_with(
                appearance_icon(&draft.folder.appearance),
                "Folder appearance",
            )
            .id_salt("draft-folder-appearance")
            .size(ControlSize::Sm)
            .enabled(idle)
            .show_at(ui, rect);
            let _ = Menu::new(Word::plain("Folder appearance"))
                .opened_by(&opener, Trigger::Click)
                .show(ui, |ui, _| {
                    change = appearance_menu(ui, &draft.folder.appearance)
                        .map(|look| (DRAFT_FOLDER, look));
                    Divider::horizontal().show(ui);
                    if MenuItem::new(Word::plain("Cancel")).show(ui).ran {
                        cancel.set(true);
                    }
                });
        }
        if let Some(id) = out.menu.as_ref().map(|request| request.key)
            && id != DRAFT_FOLDER
            && let Some(at) = tree.index_of(&id)
        {
            let appearance = &tree.rows[at].0.appearance;
            let _ =
                Menu::context(Word::plain("Folder appearance"), &out.response).show(ui, |ui, _| {
                    change = appearance_menu(ui, appearance).map(|look| (id, look));
                    Divider::horizontal().show(ui);
                    if MenuItem::new(Word::plain("Delete folder…"))
                        .enabled(idle)
                        .show(ui)
                        .ran
                    {
                        self.delete_folder = Some(id);
                    }
                });
        }
        if let Some(name) = edited.into_inner()
            && let Some(draft) = &mut self.folder_draft
        {
            draft.focus = false;
            if let Some(name) = name {
                draft.folder.name = name;
            }
        }
        if idle {
            if let Some((id, appearance)) = change {
                if id == DRAFT_FOLDER {
                    if let Some(draft) = &mut self.folder_draft {
                        draft.folder.appearance = appearance;
                        draft.focus = true;
                    }
                } else if let Some(library) = &self.library {
                    self.submit(
                        Operation::SetFolderAppearance(library.path.clone(), id, appearance),
                        Word::plain("Saving folder appearance…"),
                    );
                }
            }
            if cancel.get() {
                self.folder_draft = None;
            } else if create.get()
                && let Some(draft) = &self.folder_draft
                && !draft.folder.name.trim().is_empty()
                && let Some(library) = &self.library
            {
                self.submit(
                    Operation::CreateFolder(library.path.clone(), draft.folder.clone()),
                    Word::plain("Creating folder…"),
                );
            }
        }
        for event in out.events {
            match event {
                CollectionEvent::Dropped {
                    at: DropAt::Onto(id),
                    payload,
                    ..
                } if can_drop && id != DRAFT_FOLDER => {
                    if let Some(assets) = payload.get::<DraggedAssets>() {
                        self.edit_metadata(
                            assets.0.clone(),
                            MetadataEdit::Folder { id, linked: true },
                        );
                    }
                }
                CollectionEvent::Activate(id) if id != DRAFT_FOLDER => {
                    self.navigate_to(AssetView::Folder(id), ui.ctx())
                }
                CollectionEvent::Open(change) => {
                    let groups = self
                        .folders
                        .iter()
                        .filter_map(|folder| folder.parent)
                        .chain(
                            self.folder_draft
                                .as_ref()
                                .and_then(|draft| draft.folder.parent),
                        )
                        .collect::<std::collections::HashSet<_>>();
                    self.open_folders
                        .apply(&change.key, change.open, change.all, &groups);
                }
                _ => {}
            }
        }
    }

    fn edit_metadata(&mut self, assets: Vec<Uuid>, edit: MetadataEdit) {
        if self.idle()
            && !assets.is_empty()
            && let Some(library) = &self.library
        {
            self.submit(
                Operation::Metadata(library.path.clone(), assets, edit, self.browse.clone()),
                Word::plain("Saving asset details…"),
            );
        }
    }

    fn folder_delete_dialog(&mut self, ui: &mut Ui) {
        let folder = self
            .delete_folder
            .and_then(|id| self.folders.iter().find(|folder| folder.id == id));
        let mut open = folder.is_some();
        let buttons = [
            FooterButton::new(Word::plain("Delete folder"), DialogRole::Accept),
            FooterButton::new(Word::plain("Cancel"), DialogRole::Cancel),
        ];
        let shown = Dialog::prompt(Word::plain("Delete folder"))
            .opened(Bind::bound(&mut open))
            .buttons(buttons)
            .show(ui, |ui, _| {
                if let Some(folder) = folder {
                    Text::new(format!("Delete “{}”?", folder.name)).show(ui);
                    if self
                        .folders
                        .iter()
                        .any(|child| child.parent == Some(folder.id))
                    {
                        Text::new(Word::plain("Its subfolders will also be deleted.")).show(ui);
                    }
                    Text::new(Word::plain("The assets will stay in the library.")).show(ui);
                }
            });
        if shown.output.closed.is_some() {
            if shown.output.picked == Some(0)
                && self.idle()
                && let (Some(id), Some(library)) = (self.delete_folder, &self.library)
            {
                self.submit(
                    Operation::DeleteFolder(library.path.clone(), id, self.browse.clone()),
                    Word::plain("Deleting folder…"),
                );
            }
            self.delete_folder = None;
        }
    }

    fn inspector(&mut self, ui: &mut Ui) {
        ui.add_enabled_ui(!self.viewer.editing_comment(), |ui| {
            self.inspector_content(ui)
        });
    }

    fn preview_paths(&self, assets: &[Uuid]) -> [Option<&std::path::Path>; 3] {
        let mut first = [usize::MAX; 3];
        for &at in assets.iter().filter_map(|id| self.positions.get(id)) {
            if let Some(place) = first.iter().position(|previous| at < *previous) {
                first[place..].rotate_right(1);
                first[place] = at;
            }
        }
        first.map(|at| {
            self.rows
                .get(at)
                .filter(|row| row.available)
                .map(|row| row.asset.path.as_path())
        })
    }

    fn inspector_preview(&self, ui: &mut Ui, assets: &[Uuid]) {
        let count = assets.len();
        let paths = self.preview_paths(assets);
        let label: craft::ui::runtime::env::Label = if count > 1 {
            let label = Env::of(ui)
                .view()
                .resolve(&Word::plain("Selected images preview").into());
            format!("{} · {count}", label.as_str()).into()
        } else {
            Word::plain("Selected image").into()
        };
        let width = ui.available_width();
        let sizes = &Env::of(ui).tokens.size.thumb;
        let height = if count == 1 {
            paths[0]
                .and_then(|path| self.previews.dimensions(path))
                .map_or(width, |size| {
                    width * size.height as f32 / size.width.max(1) as f32
                })
                .clamp(sizes.lg.min(width), width)
        } else {
            width
        };
        let (rect, _) = ui.allocate_exact_size(
            craft::egui::vec2(width, height),
            craft::egui::Sense::hover(),
        );
        Picture::painter(label, ThumbAspect::SQUARE, |cx, rect| {
            self.previews
                .paint_selection(cx, rect, paths, count, PreviewSurface::Inspector);
        })
        .show_at(ui, rect);
    }

    fn paint_drag_preview(&self, ui: &Ui) {
        let ctx = ui.ctx();
        let Some(assets) = drag::active::<DraggedAssets>(ctx) else {
            return;
        };
        let Some(pointer) = ctx.input(|input| {
            input
                .pointer
                .primary_down()
                .then(|| input.pointer.latest_pos())
                .flatten()
        }) else {
            return;
        };
        let env = Env::of(ui);
        let window = ctx.content_rect();
        let side = env
            .tokens
            .size
            .thumb
            .card
            .min(window.width() * 0.45)
            .min(window.height() * 0.45);
        let offset = env.tokens.space.gap.row;
        let x = if pointer.x + offset + side <= window.right() {
            pointer.x + offset
        } else {
            pointer.x - offset - side
        };
        let y = if pointer.y + offset + side <= window.bottom() {
            pointer.y + offset
        } else {
            pointer.y - offset - side
        };
        let origin = craft::egui::pos2(
            x.clamp(window.left(), window.right() - side),
            y.clamp(window.top(), window.bottom() - side),
        );
        let rect = Rect::from_min_size(origin, craft::egui::Vec2::splat(side));
        let painter = craft::ui::contract::ghost::painter(ctx);
        let cx = ContentPainterCx::new(painter, &env, Ink::OnPanel);
        self.previews.paint_selection(
            &cx,
            rect,
            self.preview_paths(&assets.0),
            assets.0.len(),
            PreviewSurface::Drag,
        );
    }

    fn inspector_content(&mut self, ui: &mut Ui) {
        let assets = self
            .viewer
            .asset()
            .map_or_else(|| unique_assets(&self.selection.items), |id| vec![id]);
        if assets.len() != 1 {
            Text::new(Word::plain("Inspector"))
                .role(TypeRole::BodyStrong)
                .show(ui);
            let text = if assets.is_empty() {
                Word::plain("No selection")
            } else {
                Word::plain("Multiple assets selected")
            };
            Text::new(text).shade(Shade::Muted).show(ui);
            if !assets.is_empty() {
                self.inspector_preview(ui, &assets);
                self.asset_metadata(ui, &assets);
            }
            return;
        }
        let selected = assets[0];
        let Some(&at) = self.positions.get(&selected) else {
            return;
        };
        let row = &self.rows[at];
        let dedicated_viewer = row.asset.format.is_font() || row.asset.format.is_audio();
        if row.available && !(dedicated_viewer && self.viewer.asset().is_some()) {
            self.inspector_preview(ui, &assets);
        }
        let gap = Env::of(ui).tokens.space.gap;
        ui.add_space(gap.row);
        Text::new(row.asset.display_name.clone())
            .role(TypeRole::BodyStrong)
            .elide(Elide::Middle)
            .show(ui);
        if row.available && self.viewer.asset().is_none() {
            let label = if row.asset.format.is_model() {
                "Open 3D preview"
            } else if row.asset.format.is_video() || row.asset.format.is_audio() {
                "Open player"
            } else if row.asset.format.is_font() {
                "Open font specimen"
            } else {
                "Open preview"
            };
            if Button::new(Word::plain(label))
                .size(ControlSize::Sm)
                .enabled(self.idle())
                .show(ui)
                .clicked()
            {
                self.open_asset(at, ui.ctx());
                return;
            }
        } else if !row.available {
            Text::new(Word::plain("Original unavailable"))
                .tone(Tone::Warn)
                .show(ui);
        }

        let state = row.state;
        self.asset_metadata(ui, &assets);
        self.file_properties(ui, &self.rows[at]);
        ui.add_space(gap.section);
        let idle = self.idle();
        let trash_action = if state.trashed {
            Button::new(Word::plain("Restore from trash"))
        } else {
            Button::new(Word::plain("Move to trash"))
                .icon(IconSpec::svg(IconId::new("trash")))
                .variant(Variant::DANGER)
        };
        if trash_action
            .size(ControlSize::Sm)
            .enabled(idle)
            .show(ui)
            .clicked()
            && let Some(library) = &self.library
        {
            self.submit(
                Operation::Edit(
                    library.path.clone(),
                    selected,
                    AssetEdit::Trash(!state.trashed),
                    self.browse.clone(),
                ),
                Word::plain("Updating asset…"),
            );
        }
    }

    fn file_properties(&self, ui: &mut Ui, row: &Row) {
        const NAME: ColumnKey = ColumnKey::new("property-name");
        const VALUE: ColumnKey = ColumnKey::new("property-value");
        let format = row.asset.format.name();
        let mut properties = vec![("Format", format.to_owned())];
        if row.available
            && let Some(size) = self.previews.dimensions(&row.asset.path)
        {
            properties.push(("Dimensions", format!("{} × {} px", size.width, size.height)));
        }
        properties.extend(self.previews.properties(&row.asset.path));
        if let Some(file) = &row.file {
            let size = if file.bytes < 1024 {
                format!("{} B", file.bytes)
            } else if file.bytes < 1024 * 1024 {
                format!("{:.1} KiB", file.bytes as f64 / 1024.0)
            } else {
                format!("{:.1} MiB", file.bytes as f64 / (1024.0 * 1024.0))
            };
            properties.push(("File size", size));
            if let Some(created) = file.created {
                properties.push(("File created", format_local_date(created.into())));
            }
            if let Some(modified) = file.modified {
                properties.push(("File modified", format_local_date(modified.into())));
            }
        }
        if let Some(imported) =
            chrono::DateTime::from_timestamp_millis(row.asset.imported_at_unix_ms)
        {
            properties.push((
                "Imported",
                format_local_date(imported.with_timezone(&chrono::Local)),
            ));
        }
        properties.push(("Original location", row.asset.path.display().to_string()));

        ui.add_space(Env::of(ui).tokens.space.gap.section);
        Text::new(Word::plain("Properties"))
            .role(TypeRole::SectionCaption)
            .shade(Shade::Muted)
            .show(ui);
        let columns = vec![
            ColumnSpec::new(NAME, Word::plain("Property"), ColumnWidth::Content).at_most(0.42),
            ColumnSpec::new(VALUE, Word::plain("Value"), ColumnWidth::fill(1.0)),
        ];
        let source =
            keyed(&properties, |(name, _)| *name).with_text(|(name, _)| Cow::Borrowed(*name));
        Table::new(Word::plain("File properties"), columns)
            .header(false)
            .select(SelectMode::None)
            .natural_height(true)
            .row_height(RowHeight::Text)
            .size(ControlSize::Sm)
            .sizing(Sizing::FillWidth)
            .striped(true)
            .show_items(ui, &source, &mut |_, cx| {
                let (name, value) = &properties[cx.at];
                ListRow::new(Word::plain(name))
                    .cells(cx.cells())
                    .striped(cx.row.striped)
                    .cell_muted(NAME)
                    .tooltip(Word::plain(name))
                    .cell_lines(VALUE, value.clone())
                    .cell_selectable(VALUE)
            });
    }

    fn asset_metadata(&mut self, ui: &mut Ui, ids: &[Uuid]) {
        let mut tag_counts = BTreeMap::new();
        let mut folder_counts = HashMap::new();
        let mut ratings = ids.iter().map(|id| {
            self.organization
                .assets
                .get(id)
                .map_or(0, |asset| asset.rating)
        });
        let mut rating = ratings.next().unwrap_or(0);
        let mixed_rating = ratings.any(|value| value != rating);
        if mixed_rating {
            rating = 0;
        }
        for details in ids.iter().filter_map(|id| self.organization.assets.get(id)) {
            for tag in &details.tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
            for folder in &details.folders {
                *folder_counts.entry(*folder).or_insert(0) += 1;
            }
        }
        let enabled = self.idle() && self.browse.view != AssetView::Trash;
        let gap = Env::of(ui).tokens.space.gap.section;
        let mut edit = None;
        ui.add_space(gap);
        Text::new(Word::plain("Tags"))
            .role(TypeRole::SectionCaption)
            .shade(Shade::Muted)
            .show(ui);
        if !tag_counts.is_empty() {
            let max_width = ui.available_width();
            ui.horizontal_wrapped(|ui| {
                for (tag, count) in &tag_counts {
                    if membership_chip(
                        ui,
                        ("tag", tag),
                        IconSpec::svg(IconId::new("bookmark")),
                        membership_label(tag, *count, ids.len()),
                        format!("Remove tag: {tag}"),
                        max_width,
                        enabled,
                    ) {
                        edit = Some(MetadataEdit::Tags {
                            add: Vec::new(),
                            remove: vec![tag.clone()],
                        });
                    }
                }
            });
        }
        let available = self
            .organization
            .tags
            .keys()
            .filter(|tag| tag_counts.get(*tag) != Some(&ids.len()));
        if Button::new(Word::plain("Add tag"))
            .size(ControlSize::Sm)
            .enabled(enabled && available.clone().next().is_some())
            .show(ui)
            .clicked()
        {
            self.add_tags_draft = Some(AddTagsDraft {
                assets: ids.to_vec(),
                available: available.cloned().collect(),
                selected: BTreeSet::new(),
            });
        }

        ui.add_space(gap);
        Text::new(Word::plain("Folders"))
            .role(TypeRole::SectionCaption)
            .shade(Shade::Muted)
            .show(ui);
        let mut folders: Vec<_> = self
            .folders
            .iter()
            .map(|folder| (folder_path(&self.folders, folder), folder))
            .collect();
        folders.sort_by(|(a, _), (b, _)| a.cmp(b));
        if !folder_counts.is_empty() {
            let max_width = ui.available_width();
            ui.horizontal_wrapped(|ui| {
                for (path, folder) in &folders {
                    if let Some(count) = folder_counts.get(&folder.id)
                        && membership_chip(
                            ui,
                            ("folder", folder.id),
                            appearance_icon(&folder.appearance),
                            membership_label(&folder.name, *count, ids.len()),
                            format!("Remove from {path}"),
                            max_width,
                            enabled,
                        )
                    {
                        edit = Some(MetadataEdit::Folder {
                            id: folder.id,
                            linked: false,
                        });
                    }
                }
            });
        }
        let opener = Button::new(Word::plain("Add folder"))
            .size(ControlSize::Sm)
            .enabled(
                enabled
                    && folders
                        .iter()
                        .any(|(_, folder)| folder_counts.get(&folder.id) != Some(&ids.len())),
            )
            .show(ui);
        let _ = Menu::new(Word::plain("Add folder"))
            .opened_by(&opener, Trigger::Click)
            .show(ui, |ui, _| {
                for (path, folder) in folders
                    .iter()
                    .filter(|(_, folder)| folder_counts.get(&folder.id) != Some(&ids.len()))
                {
                    if MenuItem::new(path.clone())
                        .id_salt(folder.id)
                        .icon(appearance_icon(&folder.appearance))
                        .show(ui)
                        .ran
                    {
                        edit = Some(MetadataEdit::Folder {
                            id: folder.id,
                            linked: true,
                        });
                    }
                }
            });

        ui.add_space(gap);
        Text::new(Word::plain("Rating"))
            .role(TypeRole::SectionCaption)
            .shade(Shade::Muted)
            .show(ui);
        if mixed_rating {
            Text::new(Word::plain("Mixed ratings"))
                .shade(Shade::Muted)
                .show(ui);
        }
        let previous = rating;
        Rating::new(Bind::bound(&mut rating), 5)
            .label(Word::plain("Rating"))
            .size(ControlSize::Sm)
            .enabled(enabled)
            .show(ui);
        if rating != previous {
            edit = Some(MetadataEdit::Rating(rating));
        }
        if (rating > 0 || mixed_rating)
            && Button::new(Word::plain("Clear rating"))
                .size(ControlSize::Sm)
                .enabled(enabled)
                .show(ui)
                .clicked()
        {
            edit = Some(MetadataEdit::Rating(0));
        }

        if let [id] = ids {
            self.favorite_switch(ui, *id);
            self.edit_notes(ui, *id);
        }
        if let Some(edit) = edit {
            self.edit_metadata(ids.to_vec(), edit);
        }
    }

    fn favorite_switch(&mut self, ui: &mut Ui, id: Uuid) {
        let Some(&at) = self.positions.get(&id) else {
            return;
        };
        let state = self.rows[at].state;
        if state.trashed {
            return;
        }
        let mut favorite = state.favorite;
        ui.add_space(Env::of(ui).tokens.space.gap.row);
        Switch::new(Bind::bound(&mut favorite))
            .label(Word::plain("Favorite"))
            .size(ControlSize::Sm)
            .enabled(self.idle())
            .show(ui);
        if favorite != state.favorite
            && let Some(library) = &self.library
        {
            self.submit(
                Operation::Edit(
                    library.path.clone(),
                    id,
                    AssetEdit::Favorite(favorite),
                    self.browse.clone(),
                ),
                Word::plain("Updating asset…"),
            );
        }
    }

    fn edit_notes(&mut self, ui: &mut Ui, id: Uuid) {
        let Some(library) = &self.library else {
            return;
        };
        let key = (library.path.clone(), id);
        if self
            .notes_draft
            .as_ref()
            .is_none_or(|(current, _)| *current != id)
        {
            let stored = self
                .organization
                .assets
                .get(&id)
                .map_or("", |asset| asset.notes.as_str());
            let text = self
                .pending_notes
                .get(&key)
                .map_or(stored, |note| note.text.as_str());
            self.notes_draft = Some((id, text.to_owned()));
        }
        let Some((_, draft)) = &mut self.notes_draft else {
            return;
        };
        let previous = draft.clone();
        ui.add_space(Env::of(ui).tokens.space.gap.section);
        Text::new(Word::plain("Notes"))
            .role(TypeRole::SectionCaption)
            .shade(Shade::Muted)
            .show(ui);
        TextField::new(
            Word::plain("Notes"),
            Bind::bound(draft).policy(EditPolicy::WriteThrough),
        )
        .id_salt(id)
        .lines(3)
        .max_lines(5)
        .sizing(Sizing::FillWidth)
        .enabled(self.browse.view != AssetView::Trash && !self.closing)
        .show(ui);
        if *draft != previous {
            self.pending_notes.insert(
                key,
                PendingNote {
                    text: draft.clone(),
                    due: Instant::now() + NOTE_DELAY,
                },
            );
            ui.ctx().request_repaint_after(NOTE_DELAY);
        }
    }

    fn open_tag_manager(&mut self) {
        self.status = None;
        self.tag_draft = Some(String::new());
    }

    fn add_tags_dialog(&mut self, ui: &mut Ui) {
        let idle = self.idle();
        let Some(draft) = &mut self.add_tags_draft else {
            return;
        };
        let mut open = true;
        let has_selection = Cell::new(!draft.selected.is_empty());
        let buttons = [
            FooterButton::new(Word::plain("Add selected tags"), DialogRole::Accept)
                .enabled(idle && has_selection.get()),
            FooterButton::new(Word::plain("Cancel"), DialogRole::Cancel),
        ];
        let shown = Dialog::prompt(Word::plain("Add tags"))
            .opened(Bind::bound(&mut open))
            .buttons(buttons)
            .settle_buttons(|_, buttons| buttons[0].set_enabled(idle && has_selection.get()))
            .show(ui, |ui, _| {
                set_tag_dialog_width(ui);
                let height = Env::of(ui).tokens.size.control.sm;
                let gap = ui.spacing().item_spacing.y;
                let visible = draft.available.len().min(8);
                let list_height = height * visible as f32 + gap * visible.saturating_sub(1) as f32;
                let (list_rect, _) = ui.allocate_exact_size(
                    craft::egui::vec2(ui.available_width(), list_height),
                    craft::egui::Sense::hover(),
                );
                Stack::new()
                    .id_salt("add-tags-list")
                    .scroll(Scroll::vertical())
                    .show_at(ui, list_rect, |ui, _| {
                        for tag in &draft.available {
                            let previous = draft.selected.contains(tag);
                            let mut selected = previous;
                            let uses = self.organization.tags.get(tag).copied().unwrap_or_default();
                            ui.allocate_ui_with_layout(
                                craft::egui::vec2(ui.available_width(), height),
                                Layout::left_to_right(Align::Center),
                                |ui| {
                                    Checkbox::new(Bind::bound(&mut selected))
                                        .id_salt(tag)
                                        .label(tag.clone())
                                        .size(ControlSize::Sm)
                                        .enabled(idle)
                                        .show(ui);
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        Text::new(uses.to_string()).shade(Shade::Muted).show(ui);
                                    });
                                },
                            );
                            if selected != previous {
                                if selected {
                                    draft.selected.insert(tag.clone());
                                } else {
                                    draft.selected.remove(tag);
                                }
                            }
                        }
                    });
                has_selection.set(!draft.selected.is_empty());
            });
        if shown.output.closed.is_some()
            && let Some(draft) = self.add_tags_draft.take()
            && shown.output.picked == Some(0)
            && !draft.selected.is_empty()
        {
            self.edit_metadata(
                draft.assets,
                MetadataEdit::Tags {
                    add: draft.selected.into_iter().collect(),
                    remove: Vec::new(),
                },
            );
        }
    }

    fn manage_tags_dialog(&mut self, ui: &mut Ui) {
        const TAG_NAME: ColumnKey = ColumnKey::new("tag-name");
        const TAG_USES: ColumnKey = ColumnKey::new("tag-uses");
        let visible = self.tag_draft.is_some() && self.library.is_some();
        let mut open = visible;
        let idle = self.idle();
        let mut empty = String::new();
        let draft = self.tag_draft.as_mut().unwrap_or(&mut empty);
        let buttons = [
            FooterButton::new(Word::plain("Create tag"), DialogRole::Accept)
                .enabled(idle && !draft.trim().is_empty())
                .stays_open(true),
            FooterButton::new(Word::plain("Close"), DialogRole::Cancel),
        ];
        let shown = Dialog::prompt(Word::plain("Manage tags"))
            .opened(Bind::bound(&mut open))
            .buttons(buttons)
            .show(ui, |ui, _| {
                set_tag_dialog_width(ui);
                if let Some(status) = &self.status {
                    Notice::new(status.tone, status.summary.clone()).show(ui);
                }
                let entered = TextField::new(Word::plain("Tag name"), Bind::bound(&mut *draft))
                    .placeholder(Word::plain("New tag name"))
                    .submits()
                    .enabled(idle)
                    .sizing(Sizing::FillWidth)
                    .show(ui)
                    .submitted;
                let entries: Vec<_> = self.organization.tags.iter().collect();
                let source = keyed(&entries, |(tag, _)| (*tag).clone())
                    .with_text(|(tag, _)| Cow::Borrowed(tag.as_str()));
                let columns = vec![
                    ColumnSpec::new(TAG_NAME, Word::plain("Tag"), ColumnWidth::fill(1.0)),
                    ColumnSpec::new(TAG_USES, Word::plain("Uses"), ColumnWidth::Content),
                ];
                List::new(Word::plain("Library tags"))
                    .columns(columns)
                    .header(false)
                    .natural_height(true)
                    .max_rows(8)
                    .empty(Word::plain("No tags in this library"))
                    .sizing(Sizing::FillWidth)
                    .show_items(ui, &source, &mut |tag, cx| {
                        ListRow::new(tag.clone())
                            .cells(cx.cells())
                            .cell(TAG_USES, self.organization.tags[tag].to_string())
                            .cell_muted(TAG_USES)
                            .tooltip(tag.clone())
                    });
                entered
            });
        if visible && shown.output.closed.is_some() {
            self.tag_draft = None;
        } else if visible
            && idle
            && (shown.output.picked == Some(0) || shown.inner.unwrap_or(false))
            && !draft.trim().is_empty()
            && let Some(library) = &self.library
        {
            let tag = draft.trim().to_owned();
            self.submit(
                Operation::CreateTag(library.path.clone(), tag),
                Word::plain("Creating tag…"),
            );
        }
    }

    fn workspace_header(&mut self, ui: &mut Ui) {
        let available = ui.available_rect_before_wrap();
        let viewport = ui.ctx().content_rect();
        let left = (viewport.left() + sidebar_width(ui, self.navigation_hidden))
            .clamp(available.left(), available.right());
        let viewed = self.viewed_position();
        let right = (viewport.right() - sidebar_width(ui, self.inspector_hidden))
            .clamp(left, available.right());
        let inspector_button_left =
            right.min((available.right() - Env::of(ui).tokens.size.control.md).max(left));
        let toolbar_right = (inspector_button_left - ui.spacing().item_spacing.x).max(left);

        if !self.navigation_hidden {
            let rect =
                Rect::from_min_max(available.min, craft::egui::pos2(left, available.bottom()));
            ui.scope_builder(
                UiBuilder::new()
                    .id_salt("nav-header")
                    .max_rect(rect)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| {
                    self.add_button(ui);
                    self.sidebar_button(ui, Sidebar::Navigation);
                },
            );
        }
        let rect = Rect::from_min_max(
            craft::egui::pos2(left, available.top()),
            craft::egui::pos2(toolbar_right, available.bottom()),
        );
        if let Some(at) = viewed {
            let writable = self.can_write_comment();
            ui.scope_builder(
                UiBuilder::new().id_salt("viewer-toolbar").max_rect(rect),
                |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(rect));
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        if self.navigation_hidden {
                            self.sidebar_button(ui, Sidebar::Navigation);
                        }
                        if let Some(action) = self.viewer.header(
                            ui,
                            &self.rows[at].asset.display_name,
                            self.asset_grid.visible_positions[&self.rows[at].asset.id],
                            self.asset_grid.visible_assets.len(),
                            writable,
                        ) {
                            self.viewer_action(action, ui.ctx());
                        }
                    });
                },
            );
        } else {
            self.workspace_toolbar(ui, rect);
        }
        let rect = Rect::from_min_max(
            craft::egui::pos2(inspector_button_left, available.top()),
            available.max,
        );
        ui.scope_builder(
            UiBuilder::new()
                .id_salt("inspector-header")
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| self.sidebar_button(ui, Sidebar::Inspector),
        );
    }

    fn viewed_position(&self) -> Option<usize> {
        self.positions.get(&self.viewer.asset()?).copied()
    }

    fn can_write_comment(&self) -> bool {
        self.idle() && self.browse.view != AssetView::Trash && !self.closing
    }

    fn open_asset(&mut self, at: usize, ctx: &Context) {
        let Some(library) = &self.library else { return };
        let row = &self.rows[at];
        let Some(&position) = self.asset_grid.visible_positions.get(&row.asset.id) else {
            return;
        };
        self.viewer.open(&row.asset, library.path.clone());
        self.announcement = Some(format!(
            "{} · {} / {}",
            row.asset.display_name,
            position + 1,
            self.asset_grid.visible_assets.len()
        ));
        ctx.request_repaint();
    }

    fn viewer_action(&mut self, action: ViewerAction, ctx: &Context) {
        let Some(at) = self.viewed_position() else {
            return;
        };
        let position = self.asset_grid.visible_positions[&self.rows[at].asset.id];
        let next = match action {
            ViewerAction::ToggleCommentTool => {
                if self.can_write_comment() {
                    self.viewer.toggle_comment_tool();
                }
                ctx.request_repaint();
                return;
            }
            ViewerAction::EditComment(edit) => {
                if self.can_write_comment()
                    && let Some(library) = &self.library
                {
                    self.viewer.set_saving_comment();
                    self.submit(
                        Operation::EditComment(library.path.clone(), self.rows[at].asset.id, edit),
                        Word::plain("Updating comment…"),
                    );
                }
                return;
            }
            ViewerAction::Back => {
                self.viewer.close();
                self.restore_grid_focus = true;
                self.announcement = Some(Word::plain("Library").english.to_owned());
                ctx.request_repaint();
                return;
            }
            ViewerAction::Previous => position.checked_sub(1),
            ViewerAction::Next => Some(position + 1),
        };
        if let Some(next) = next.and_then(|index| self.asset_grid.visible_assets.get(index))
            && let Some(&at) = self.positions.get(next)
        {
            self.open_asset(at, ctx);
        }
    }

    fn viewer_status(&mut self, ui: &mut Ui) {
        if let Some(at) = self.viewed_position() {
            let pad = panel_padding(ui);
            Frame::new()
                .inner_margin(Margin::symmetric(pad, 0))
                .show(ui, |ui| {
                    self.viewer.status(ui, self.rows[at].asset.format);
                });
        }
    }

    fn library_menu(&mut self, ui: &mut Ui) {
        let Some(library) = &self.library else {
            return;
        };
        let name = library.library.name.clone();
        let width = ui.available_width();
        let rail = Env::of(ui).tokens.size.rail.width;
        let idle = self.idle();
        let opener = if width >= rail * 2.5 {
            Button::new(name)
                .icon(appearance_icon(&library.library.appearance))
                .trailing_icon(IconSpec::svg(IconId::new("chevron_down")))
                .tooltip(Word::plain("Switch library"))
                .variant(Variant::GHOST)
                .size(ControlSize::Md)
                .enabled(idle)
                .show(ui)
        } else {
            chrome_icon_with(
                appearance_icon(&library.library.appearance),
                "Switch library",
            )
            .enabled(idle)
            .show(ui)
        };

        let mut open_file = false;
        let mut home = false;
        let mut create = false;
        let mut selected = None;
        let mut manage_tags = false;
        let mut appearance = None;
        let _ = Menu::new(Word::plain("Switch library"))
            .opened_by(&opener, Trigger::Click)
            .show(ui, |ui, _| {
                home = MenuItem::new(Word::plain("Home"))
                    .icon(IconSpec::svg(IconId::new("home")))
                    .show(ui)
                    .ran;
                Divider::horizontal().show(ui);
                create = MenuItem::new(Word::plain("Create library…"))
                    .icon(IconSpec::svg(IconId::new("plus")))
                    .show(ui)
                    .ran;
                open_file = MenuItem::new(Word::plain("Open library…"))
                    .icon(IconSpec::svg(IconId::new("folder_open")))
                    .show(ui)
                    .ran;
                Menu::submenu_row(ui, Word::plain("Edit"), |ui, _| {
                    appearance = appearance_menu(ui, &library.library.appearance);
                });
                manage_tags = MenuItem::new(Word::plain("Manage tags…"))
                    .icon(IconSpec::svg(IconId::new("bookmark")))
                    .show(ui)
                    .ran;
                if !self.recents.is_empty() {
                    Divider::horizontal().show(ui);
                    for entry in &self.recents {
                        let path = entry.path();
                        if MenuItem::new(entry.name.clone())
                            .id_salt(path)
                            .icon(appearance_icon(&entry.appearance))
                            .tooltip(path.display().to_string())
                            .check(entry.path() == library.path)
                            .show(ui)
                            .ran
                            && entry.path() != library.path
                        {
                            selected = Some(path.to_path_buf());
                        }
                    }
                }
            });
        if idle && let Some(path) = selected {
            self.submit(Operation::Open(path), Word::plain("Opening library…"));
        } else if idle && let Some(appearance) = appearance {
            self.submit(
                Operation::SetLibraryAppearance(library.path.clone(), appearance),
                Word::plain("Saving library appearance…"),
            );
        } else if idle && open_file {
            self.open_picker();
        } else if idle && create {
            self.open_create_library(ui.ctx());
        } else if idle && manage_tags {
            self.open_tag_manager();
        } else if idle && home {
            self.library = None;
            self.add_tags_draft = None;
            self.status = None;
            self.folder_draft = None;
            self.folders.clear();
            self.previews.clear();
            self.replace_rows(Vec::new());
            ui.ctx().request_repaint();
        }
    }

    fn workspace_toolbar(&mut self, ui: &mut Ui, room: Rect) {
        let env = Env::of(ui);
        let gap = env.tokens.space.gap.item;
        let rail = env.tokens.size.rail.width;
        let side_width = (room.width() - rail * 3.0).max(0.0) / 2.0;
        let modes = Rect::from_min_max(
            craft::egui::pos2(room.left() + side_width, room.top()),
            craft::egui::pos2(room.right() - side_width, room.bottom()),
        );
        let left = Rect::from_min_max(room.min, craft::egui::pos2(modes.left(), room.bottom()));
        let right = Rect::from_min_max(craft::egui::pos2(modes.right(), room.top()), room.max);

        ui.scope_builder(
            UiBuilder::new()
                .id_salt("view-title")
                .max_rect(left)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                if self.navigation_hidden {
                    self.sidebar_button(ui, Sidebar::Navigation);
                    self.add_button(ui);
                }
                self.library_menu(ui);
            },
        );

        ui.scope_builder(
            UiBuilder::new()
                .id_salt("view-mode")
                .max_rect(modes)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.set_width(modes.width());
                let entries = [
                    Entry::new(DisplayMode::Justified, Word::plain("Justified")),
                    Entry::new(DisplayMode::Grid, Word::plain("Grid")),
                ];
                let _ = Segmented::new(
                    Word::plain("View"),
                    Bind::bound(&mut self.display),
                    &entries,
                )
                .sizing(Sizing::Content)
                .size(ControlSize::Md)
                .show(ui);
            },
        );

        let search_pad = if right.width() >= rail * 2.0 {
            gap
        } else {
            0.0
        };
        let search_room = right.shrink2(craft::egui::vec2(search_pad, 0.0));
        ui.scope_builder(
            UiBuilder::new()
                .id_salt("search")
                .max_rect(search_room)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.set_clip_rect(ui.clip_rect().intersect(search_room));
                self.browse_controls(ui);
            },
        );
    }

    fn browse_controls(&mut self, ui: &mut Ui) {
        let idle = self.idle();
        let rail = Env::of(ui).tokens.size.rail.width;
        let has_children = self.browse.folder().is_some_and(|parent| {
            self.folders
                .iter()
                .any(|folder| folder.parent == Some(parent))
        });
        let toolbar_toggle = has_children && ui.available_width() >= rail * 2.0;
        let width = ui.available_width() - if toolbar_toggle { rail } else { 0.0 };
        let combined = width < rail * 4.0;
        let search_width = width - if combined { rail } else { rail * 2.0 };
        let inline = search_width >= rail;
        let mut next = self.browse.clone();
        let subfolders_label = if next.hide_subfolders {
            "Show subfolders"
        } else {
            "Hide subfolders"
        };
        let mut submitted = inline && self.search_field(ui, search_width, idle);
        if toolbar_toggle
            && chrome_icon("folder_open", subfolders_label)
                .selected(!next.hide_subfolders)
                .enabled(idle)
                .show(ui)
                .clicked()
        {
            next.hide_subfolders = !next.hide_subfolders;
        }
        if !combined {
            let sort = chrome_icon("sort_ascending", "Sort")
                .selected(next.order != AssetOrder::ViewDefault)
                .enabled(idle)
                .show(ui);
            let _ = Menu::new(Word::plain("Sort"))
                .opened_by(&sort, Trigger::Click)
                .show(ui, |ui, _| order_menu(ui, &mut next.order));
        }
        let filters = chrome_icon("funnel", "Filters")
            .selected(
                next.filters != Filters::default()
                    || (combined && next.order != AssetOrder::ViewDefault),
            )
            .enabled(idle)
            .show(ui);
        let _ = Menu::new(Word::plain("Filters"))
            .opened_by(&filters, Trigger::Click)
            .show(ui, |ui, _| {
                if !inline {
                    submitted |= self.search_field(ui, rail * 4.0, idle);
                    Divider::horizontal().show(ui);
                }
                if has_children && !toolbar_toggle {
                    if MenuItem::new(Word::plain(subfolders_label)).show(ui).ran {
                        next.hide_subfolders = !next.hide_subfolders;
                    }
                    Divider::horizontal().show(ui);
                }
                filter_menu(ui, &mut next.filters, &self.organization.tags);
                if combined {
                    Menu::submenu_row(ui, Word::plain("Sort"), |ui, _| {
                        order_menu(ui, &mut next.order)
                    });
                }
            });
        if submitted {
            next.query.clone_from(&self.query);
        }
        if idle
            && (submitted || next != self.browse)
            && let Some(library) = &self.library
        {
            self.submit(
                Operation::Find(library.path.clone(), next),
                Word::plain("Searching…"),
            );
        }
    }

    fn search_field(&mut self, ui: &mut Ui, width: f32, enabled: bool) -> bool {
        let sizes = Env::of(ui).tokens.size;
        let rail = sizes.rail.width;
        let width = width.min(rail * 3.0);
        let compact = width < rail * 2.0;
        ui.allocate_ui_with_layout(
            craft::egui::vec2(width, sizes.control.md),
            Layout::left_to_right(Align::Center),
            |ui| {
                TextField::search(
                    Word::plain("Search by name or tag"),
                    Bind::bound(&mut self.query),
                )
                .placeholder(Word::plain(if compact { "" } else { "Search" }))
                .sizing(Sizing::FillWidth)
                .clear_button(!compact)
                .size(ControlSize::Md)
                .submits()
                .enabled(enabled)
                .show(ui)
                .submitted
            },
        )
        .inner
    }

    fn zoom_spec(&self, scale: f64) -> TileZoom {
        TileZoom::bounded(0.6 * scale, self.zoom_limit * scale, 0.25 * scale, scale)
    }

    fn zoom_controls(&mut self, ui: &mut Ui) {
        let zoom = self.zoom_spec(1.0);
        let range = zoom.range();
        let _ = ControlRow::new().show(ui, |ui, _| {
            if chrome_icon("grid", "Smaller thumbnails")
                .enabled(self.zoom > *range.start())
                .show(ui)
                .clicked()
            {
                self.zoom = zoom.step(self.zoom, ZoomStep::Out);
            }
            let _ = Slider::new(
                Bind::bound(&mut self.zoom).reset(zoom.default_value()),
                range.clone(),
            )
            .label(Word::plain("Thumbnail zoom"))
            .size(ControlSize::Md)
            .show(ui);
            if chrome_icon("grid_small", "Larger thumbnails")
                .enabled(self.zoom < *range.end())
                .show(ui)
                .clicked()
            {
                self.zoom = zoom.step(self.zoom, ZoomStep::In);
            }
        });
    }

    fn status_strip(&mut self, ui: &mut Ui) {
        let pad = panel_padding(ui);
        Frame::new()
            .inner_margin(Margin::symmetric(pad, 0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    Text::new(format!(
                        "{} assets · {} selected",
                        self.rows.len(),
                        unique_assets(&self.selection.items).len()
                    ))
                    .role(TypeRole::Caption)
                    .shade(Shade::Muted)
                    .show(ui);
                    ui.with_layout(Layout::right_to_left(craft::egui::Align::Center), |ui| {
                        self.zoom_controls(ui);
                        let label = if self.show_details {
                            "Hide thumbnail details"
                        } else {
                            "Show thumbnail details"
                        };
                        if chrome_icon("info", label)
                            .selected(self.show_details)
                            .show(ui)
                            .clicked()
                        {
                            self.show_details = !self.show_details;
                        }
                    });
                })
            });
    }

    fn start_screen(&mut self, ui: &mut Ui) {
        let mut selected = None;
        let mut appearance_change = None;
        let recents = &self.recents;
        let mut recent_rows = |ui: &mut Ui| {
            let indexed: Vec<_> = recents.iter().enumerate().collect();
            let source = keyed(&indexed, |(index, _)| *index)
                .with_text(|(_, entry)| Cow::Borrowed(entry.name.as_str()));
            let rows = List::new(Word::plain("Recent libraries"))
                .activate(ActivateOn::Click)
                .sizing(Sizing::FillWidth)
                .natural_height(true)
                .show_items(ui, &source, &mut |&index, _| {
                    let entry = &recents[index];
                    let path = entry.path().display().to_string();
                    ListRow::new(entry.name.clone())
                        .detail(path.clone())
                        .tooltip(path)
                        .icon(appearance_icon(&entry.appearance))
                });
            if let Some(request) = &rows.menu {
                let entry = &recents[request.key];
                let _ = Menu::context(Word::plain("Library appearance"), &rows.response).show(
                    ui,
                    |ui, _| {
                        appearance_change = appearance_menu(ui, &entry.appearance)
                            .map(|look| (entry.path().to_path_buf(), look));
                    },
                );
            }
            for event in rows.events {
                if let CollectionEvent::Activate(index) = event {
                    selected = Some(recents[index].path().to_path_buf());
                }
            }
        };
        let mut page = StartScreen::new(
            Bind::given(RecentView::List),
            Bind::given(RecentSort::Opened),
            &[],
        )
        .layout(StartLayout::Column)
        .title(Word::plain("AssetCraft"))
        .new_label(Word::plain("Create library"))
        .open_label(Word::plain("Open library"));
        if !recents.is_empty() {
            page = page.app_section(Word::plain("Recent libraries"), &mut recent_rows);
        }
        let idle = self.idle();
        let output = ui.add_enabled_ui(idle, |ui| page.show(ui)).inner;
        if !idle {
            return;
        }
        if let Some(path) = selected {
            self.submit(Operation::Open(path), Word::plain("Opening library…"));
            return;
        }
        if let Some((path, appearance)) = appearance_change {
            self.submit(
                Operation::SetLibraryAppearance(path, appearance),
                Word::plain("Saving library appearance…"),
            );
            return;
        }
        for action in output.actions {
            match action {
                StartAction::Open => self.open_picker(),
                StartAction::New => {
                    self.open_create_library(ui.ctx());
                }
                _ => continue,
            }
            break;
        }
    }

    fn import_window_drop(&mut self, drop: &OsFiles) {
        if self.browse.view == AssetView::Trash {
            self.failure("Open another view before adding files".to_owned());
        } else if self.can_import()
            && self.library_draft.is_none()
            && self.delete_folder.is_none()
            && let Some(library) = &self.library
        {
            let paths: Vec<_> = drop
                .files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect();
            if paths.is_empty() {
                self.failure("Drop files with a location on disk".to_owned());
                return;
            }
            self.submit(
                Operation::Add(library.path.clone(), paths, self.browse.clone()),
                Word::plain("Adding dropped files…"),
            );
        } else {
            self.failure(
                "Open a library and finish the current action before dropping files".to_owned(),
            );
        }
    }

    fn unsaved_notes_dialog(&mut self, ui: &mut Ui) {
        if !self.close_note_dialog {
            return;
        }
        let mut open = true;
        let shown = Dialog::prompt(Word::plain("Notes could not be saved"))
            .opened(Bind::bound(&mut open))
            .buttons([
                FooterButton::new(Word::plain("Retry saving"), DialogRole::Accept),
                FooterButton::new(Word::plain("Discard and close"), DialogRole::Destructive),
                FooterButton::new(Word::plain("Cancel"), DialogRole::Cancel),
            ])
            .show(ui, |ui, _| {
                Text::new(Word::plain(
                    "Retry saving, or copy the unsaved notes before closing.",
                ))
                .show(ui);
                if Button::new(Word::plain("Copy unsaved notes"))
                    .show(ui)
                    .clicked()
                {
                    let mut notes: Vec<_> = self.pending_notes.iter().collect();
                    notes.sort_by_key(|(key, _)| *key);
                    let text = notes
                        .into_iter()
                        .map(|((path, id), note)| {
                            format!("{} · {id}\n{}", path.display(), note.text)
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    ui.ctx().copy_text(text);
                }
            });
        if shown.output.closed.is_some() {
            self.close_note_dialog = false;
            match shown.output.picked {
                Some(0) => self.closing = true,
                Some(1) => {
                    self.pending_notes.clear();
                    self.closing = true;
                }
                _ => {}
            }
            ui.ctx().request_repaint();
        }
    }

    fn workspace(&mut self, ui: &mut Ui) {
        let room = ui.available_rect_before_wrap();
        let left = room.left() + sidebar_width(ui, self.navigation_hidden);
        let right = room.right() - sidebar_width(ui, self.inspector_hidden);
        let at = |x, right| {
            Rect::from_min_max(
                craft::egui::pos2(x, room.top()),
                craft::egui::pos2(right, room.bottom()),
            )
        };
        if !self.navigation_hidden {
            let _ = ShellRegion::chrome(
                ShellPane::new("library-navigation")
                    .edge(PaneEdge::Start)
                    .at(at(room.left(), left)),
            )
            .label(Word::plain("Library navigation"))
            .show(ui, |ui, _| {
                let area = ui.available_rect_before_wrap();
                Stack::new()
                    .id_salt("navigation-scroll")
                    .pad(Pad::Regular)
                    .scroll(Scroll::vertical())
                    .show_at(ui, area, |ui, _| {
                        ui.add_enabled_ui(!self.viewer.editing_comment(), |ui| self.navigation(ui));
                    });
            });
        }
        if !self.inspector_hidden {
            let _ = ShellRegion::chrome(
                ShellPane::new("asset-inspector")
                    .edge(PaneEdge::End)
                    .at(at(right, room.right())),
            )
            .label(Word::plain("Asset inspector"))
            .show(ui, |ui, _| {
                let area = ui.available_rect_before_wrap();
                Stack::new()
                    .id_salt("inspector-scroll")
                    .pad(Pad::Regular)
                    .scroll(Scroll::vertical())
                    .show_at(ui, area, |ui, _| {
                        self.inspector(ui);
                    });
            });
        }
        let viewed = self.viewed_position();
        ui.scope_builder(
            UiBuilder::new()
                .id_salt(if viewed.is_some() {
                    "asset-viewer"
                } else {
                    "asset-grid"
                })
                .max_rect(at(left, right)),
            |ui| {
                ui.set_clip_rect(ui.clip_rect().intersect(at(left, right)));
                self.operation_status(ui);
                if let Some(at) = viewed {
                    let writable = self.can_write_comment();
                    if let Some(action) =
                        self.viewer
                            .body(ui, &self.rows[at].asset.display_name, writable)
                    {
                        self.viewer_action(action, ui.ctx());
                    }
                } else {
                    self.body(ui);
                }
            },
        );
    }

    fn open_create_library(&mut self, ctx: &Context) {
        self.status = None;
        self.library_draft = Some(LibraryDraft::default());
        ctx.request_repaint();
    }

    fn create_library_dialog(&mut self, ui: &mut Ui) {
        let visible = self.idle() && self.library_draft.is_some();
        let mut open = visible;
        let mut empty = LibraryDraft::default();
        let draft = self.library_draft.as_mut().unwrap_or(&mut empty);
        let buttons = [
            FooterButton::new(Word::plain("Choose location…"), DialogRole::Accept)
                .enabled(!draft.name.trim().is_empty())
                .stays_open(true),
            FooterButton::new(Word::plain("Cancel"), DialogRole::Cancel),
        ];
        let shown = Dialog::prompt(Word::plain("Create library"))
            .opened(Bind::bound(&mut open))
            .buttons(buttons)
            .show(ui, |ui, _| {
                if let Some(status) = &self.status {
                    Notice::new(status.tone, status.summary.clone())
                        .lines(status.details.iter().cloned())
                        .show(ui);
                }
                ui.horizontal(|ui| {
                    let opener =
                        chrome_icon_with(appearance_icon(&draft.appearance), "Library appearance")
                            .show(ui);
                    let entered =
                        TextField::new(Word::plain("Library name"), Bind::bound(&mut draft.name))
                            .placeholder(Word::plain("Library name"))
                            .sizing(Sizing::FillWidth)
                            .submits()
                            .show(ui)
                            .submitted;
                    let mut appearance = None;
                    let _ = Menu::new(Word::plain("Library appearance"))
                        .opened_by(&opener, Trigger::Click)
                        .show(ui, |ui, _| {
                            appearance = appearance_menu(ui, &draft.appearance)
                        });
                    if let Some(appearance) = appearance {
                        draft.appearance = appearance;
                    }
                    entered
                })
                .inner
            });
        if !visible {
            return;
        }
        if shown.output.closed.is_some() {
            self.library_draft = None;
            self.status = None;
        } else if (shown.output.picked == Some(0) || shown.inner.unwrap_or(false))
            && !draft.name.trim().is_empty()
        {
            let draft = draft.clone();
            self.status = None;
            self.pick(
                Pick::Create(draft),
                PickRequest::save("create-library")
                    .title("Create library")
                    .file_name("catalog.sqlite"),
            );
        }
    }

    fn operation_status(&self, ui: &mut Ui) {
        if let Some(work) = self.work {
            Text::new(work).show(ui);
        }
        if self.library_draft.is_none()
            && let Some(status) = &self.status
        {
            Notice::new(status.tone, status.summary.clone())
                .lines(status.details.iter().cloned())
                .show(ui);
        }
    }

    fn body(&mut self, ui: &mut Ui) {
        if self.library.is_none() {
            self.start_screen(ui);
            return;
        }
        let empty = if !self.browse.query.is_empty() || self.browse.filters != Filters::default() {
            "No matching files"
        } else {
            match self.browse.view {
                AssetView::All | AssetView::Random => "No files in this library",
                AssetView::Recent => "No files added in the last 30 days",
                AssetView::Favorites => "No favorites",
                AssetView::Untagged => "No untagged files",
                AssetView::Trash => "Trash is empty",
                AssetView::Folder(_) => "This folder is empty",
                AssetView::Tag(_) => "No files with this tag",
            }
        };
        let source = Assets {
            rows: &self.rows,
            positions: &self.positions,
            grid: &self.asset_grid,
            previews: &self.previews,
        };
        let justified = self.display == DisplayMode::Justified && self.zoom < self.zoom_limit;
        let fit_sections = !justified && !source.grid.groups.is_empty();
        let caption = if self.show_details {
            CaptionPlace::Below
        } else {
            CaptionPlace::None
        };
        let pad = panel_padding(ui);
        let env = Env::of(ui);
        let inset = env.tokens.space.gap.inline;
        // Sections already add this inset inside the grid.
        let horizontal_pad = if fit_sections {
            pad.saturating_sub(inset.round() as i8).max(0)
        } else {
            pad
        };
        let scale = if fit_sections {
            let width = (ui.available_width() - 2.0 * (f32::from(horizontal_pad) + inset)).max(1.0);
            let style = StyleCx::of_env(ui.ctx(), &env);
            let surround = Tile::around_picture(&style, caption, self.show_details).width();
            let gap = if self.show_details {
                env.tokens.space.gap.stack
            } else {
                0.0
            };
            let thumb = env.tokens.size.thumb.card;
            let card = self.zoom as f32 * thumb + surround;
            let columns = ((width + gap) / (card + gap)).floor().max(1.0);
            // The grouped arrangement counts columns again inside its insets.
            // Leave a fraction of a point so rounding cannot drop the last column.
            let side = ((width + gap) / columns - gap - surround - 0.01).max(1.0);
            f64::from(side / thumb) / self.zoom
        } else {
            1.0
        };
        let zoom = self.zoom_spec(scale);
        let shown_zoom = self.zoom * scale;
        let mut grid_zoom = shown_zoom;
        let mut grid = TileGrid::new(Word::plain("Files"))
            .activate(ActivateOn::DoubleClick)
            .drag_payload(|drag| Some(DraggedAssets(unique_assets(&drag.keys))))
            .drag_preview(RowPreview::None)
            .empty(Word::plain(empty))
            .groups(&self.asset_grid.groups)
            .collapsible()
            .selection(Bind::bound(&mut self.selection))
            .zoom(GridZoom::Bound(Bind::bound(&mut grid_zoom), zoom))
            .caption(caption);
        if !craft::ui::runtime::measure::active(ui.ctx())
            && std::mem::take(&mut self.restore_grid_focus)
        {
            grid = grid.request_focus(FocusRequest::Take);
        }
        if self.show_details {
            grid = grid.detail_line();
        }
        grid = if justified {
            grid.justified()
        } else if fit_sections {
            grid.aspect(4.0 / 3.0)
        } else {
            grid.aspect(4.0 / 3.0).fill_row()
        };
        let (section, opened) = Frame::new()
            .inner_margin(Margin::symmetric(horizontal_pad, pad))
            .show(ui, |ui| {
                let shown = grid.show_items(ui, &source, &mut |key, _| {
                    let Some(row) = source.row(key) else {
                        return Tile::new("");
                    };
                    let path = &row.asset.path;
                    let size = self.previews.dimensions(path);
                    let mut tile = Tile::new(row.asset.display_name.clone());
                    if self.show_details
                        && let Some(size) = size
                    {
                        tile = tile.detail(format!("{} × {} px", size.width, size.height));
                    }
                    if !row.available {
                        return tile.status(TileStatus::Offline(Some(
                            Word::plain("Original unavailable").into(),
                        )));
                    }
                    let aspect = size.map_or(ThumbAspect::SQUARE, |size| {
                        ThumbAspect::new(size.width as u16, size.height as u16)
                    });
                    tile.thumb(aspect, |cx, rect| {
                        self.previews.paint(cx, rect, path, PreviewSurface::Grid)
                    })
                });
                let section = shown.events.into_iter().find_map(|event| match event {
                    GridEvent::GroupPicked { group, .. } => {
                        let group = source.grid.groups.get(group)?;
                        Some((group.key, !group.open))
                    }
                    GridEvent::GroupOpen { group, open, .. } => {
                        Some((source.grid.groups.get(group)?.key, open))
                    }
                    _ => None,
                });
                let opened = shown
                    .response
                    .events
                    .into_iter()
                    .find_map(|event| match event {
                        CollectionEvent::Activate(key) => key.asset(),
                        _ => None,
                    });
                (section, opened)
            })
            .inner;
        if grid_zoom != shown_zoom {
            self.zoom = if grid_zoom >= self.zoom_limit * scale {
                self.zoom_limit
            } else {
                (grid_zoom / scale).max(0.6)
            };
        }
        if let Some((GridEntry::Folder(id), open)) = section {
            if open {
                self.collapsed_sections.remove(&id);
            } else {
                self.collapsed_sections.insert(id);
            }
            self.refresh_grid();
            ui.ctx().request_repaint();
        } else if self.idle()
            && let Some(at) = opened.and_then(|id| self.positions.get(&id).copied())
        {
            self.open_asset(at, ui.ctx());
        }
    }
}

fn format_local_date(date: chrono::DateTime<chrono::Local>) -> String {
    date.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn set_tag_dialog_width(ui: &mut Ui) {
    let width = Env::of(ui).tokens.size.dialog.wrap;
    ui.set_width(width.min(ui.available_width()));
}

fn membership_chip(
    ui: &mut Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    icon: IconSpec<'_>,
    label: String,
    remove: String,
    max_width: f32,
    enabled: bool,
) -> bool {
    let env = Env::of(ui);
    let height = env.tokens.size.control.sm;
    let icon_side = env.tokens.size.icon.sm;
    let padding = env.tokens.space.pad.control;
    let gap = env.tokens.space.gap.tight;
    let painter = ContentPainterCx::new(ui.painter().clone(), &env, Ink::OnPanel);
    let label_width = painter.with_draw(|draw| {
        draw.layout(
            &TextRun::new(label.as_str()).role(TypeRole::Caption),
            f32::INFINITY,
        )
        .size()
        .x
    });
    let width = (padding + icon_side + 2.0 * gap + label_width.ceil() + height).min(max_width);
    let (rect, _) = ui.allocate_exact_size(
        craft::egui::vec2(width, height),
        craft::egui::Sense::hover(),
    );
    ui.painter().rect_filled(
        rect,
        UiTokens::corner(env.tokens.shape.radius.pill),
        craft::egui::Color32::from(craft::theme::Srgba::from(env.tokens.color.surface.control)),
    );

    let flow = Flow::new(rect, env.direction.is_rtl());
    let (remove_rect, flow) = flow.take_end(height);
    let (_, flow) = flow.take_start(padding);
    let (icon_rect, flow) = flow.take_start(icon_side);
    let (_, flow) = flow.take_start(gap);
    let (_, flow) = flow.take_end(gap);
    Icon::new(icon).size(ControlSize::Sm).show_at(
        ui,
        Rect::from_center_size(icon_rect.center(), craft::egui::Vec2::splat(icon_side)),
    );
    Text::new(label)
        .id_salt((&key, "label"))
        .role(TypeRole::Caption)
        .elide(Elide::Middle)
        .show_at(ui, flow.rect());
    chrome_icon("close", "Remove")
        .id_salt((key, "remove"))
        .label(remove.clone())
        .tooltip(remove)
        .size(ControlSize::Sm)
        .enabled(enabled)
        .show_at(ui, remove_rect)
        .clicked()
}

fn membership_label(name: &str, count: usize, selected: usize) -> String {
    if count == selected {
        name.to_owned()
    } else {
        format!("{name} ({count}/{selected})")
    }
}

fn folder_path(folders: &[Folder], folder: &Folder) -> String {
    let mut names = vec![folder.name.as_str()];
    let mut parent = folder.parent;
    while let Some(above) = parent.and_then(|id| folders.iter().find(|folder| folder.id == id)) {
        if names.len() == folders.len() {
            break;
        }
        names.push(&above.name);
        parent = above.parent;
    }
    names.reverse();
    names.join(" / ")
}

fn chrome_icon(icon: &'static str, label: &'static str) -> IconButton<'static> {
    chrome_icon_with(IconSpec::svg(IconId::new(icon)), label)
}

fn chrome_icon_with(icon: IconSpec<'static>, label: &'static str) -> IconButton<'static> {
    IconButton::new(icon)
        .label(Word::plain(label))
        .tooltip(Word::plain(label))
        .size(ControlSize::Md)
        .glyphs(GlyphRole::Tool)
        .variant(Variant::GHOST)
}

fn panel_padding(ui: &Ui) -> i8 {
    Env::of(ui).tokens.space.pad.regular.round() as i8
}

fn sidebar_width(ui: &Ui, hidden: bool) -> f32 {
    if hidden {
        return 0.0;
    }
    let rail = Env::of(ui).tokens.size.rail.width;
    let width = ui.ctx().content_rect().width();
    (width * 0.19)
        .clamp(rail * 3.0, rail * 6.0)
        .min(width * 0.24)
}

impl ShellApp for Desktop {
    fn frame_input(&mut self) -> FrameInput {
        FrameInput::new(self.environment.clone()).host(self.host.clone())
    }

    fn logic(&mut self, _ctx: &Context) {
        self.receive_work();
        if let Some(message) = self.viewer.receive() {
            self.announcement = Some(message.english.to_owned());
        }
    }

    fn tick(&mut self, now: Instant) -> Option<Instant> {
        if !self.idle() || self.close_note_dialog {
            return None;
        }
        let (key, note) = self.pending_notes.iter().min_by_key(|(_, note)| note.due)?;
        let due = if self.closing { now } else { note.due };
        if due <= now {
            let operation = Operation::SaveNote(key.0.clone(), key.1, note.text.clone());
            self.submit(operation, Word::plain("Saving notes…"));
            None
        } else {
            Some(due)
        }
    }

    fn close_requested(&mut self) -> CloseDecision {
        if self.viewer.saving_comment() {
            self.closing = true;
            return CloseDecision::Hold;
        }
        if self.pending_notes.is_empty() {
            CloseDecision::Close
        } else {
            self.closing = !self.close_note_dialog;
            CloseDecision::Hold
        }
    }

    fn ui(&mut self, ui: &mut Ui) {
        let viewing = self.viewed_position().is_some();
        let opened = self.library.is_some();
        let width = ui.ctx().content_rect().width()
            - sidebar_width(ui, self.navigation_hidden)
            - sidebar_width(ui, self.inspector_hidden);
        let next_zoom_limit = f64::from(width / Env::of(ui).tokens.size.thumb.card).max(5.0);
        self.zoom = if self.zoom >= self.zoom_limit {
            next_zoom_limit
        } else {
            self.zoom.min(next_zoom_limit)
        };
        self.zoom_limit = next_zoom_limit;
        let mut shell = AppShell::new(WindowKey::main()).title_bar(|ui, _, app: &mut Self| {
            let output = TitleBar::new()
                .show(ui, |ui, _| {
                    if opened {
                        app.workspace_header(ui);
                    }
                })
                .output;
            if output.close_requested {
                ui.ctx().send_viewport_cmd(ViewportCommand::Close);
            }
        });
        if viewing {
            shell = shell.status(true, |ui, _, app| app.viewer_status(ui));
        } else if opened {
            shell = shell.status(true, |ui, _, app| app.status_strip(ui));
        }
        shell
            .body_label(Word::plain(if viewing {
                "Asset viewer"
            } else {
                "Library"
            }))
            .body(|ui, _, app| {
                DropVeil::new(Word::plain("Drop files to import")).show_with_files(
                    ui,
                    |ui, drop| {
                        if let Some(drop) = drop {
                            app.import_window_drop(drop);
                        }
                        if opened {
                            app.workspace(ui);
                        } else {
                            app.operation_status(ui);
                            app.body(ui);
                        }
                    },
                );
            })
            .show(ui, self);
        self.create_library_dialog(ui);
        self.folder_delete_dialog(ui);
        let tag_manager_open = self.tag_draft.is_some();
        self.manage_tags_dialog(ui);
        self.add_tags_dialog(ui);
        self.unsaved_notes_dialog(ui);
        if self.closing && self.pending_notes.is_empty() && !self.viewer.saving_comment() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        } else if self.closing && self.idle() {
            ui.ctx().request_repaint();
        }
        self.panels
            .pump_with(&mut self.dialogs, None::<&eframe::Frame>);
        self.paint_drag_preview(ui);
        self.receive_pick(ui.ctx());
        self.dialogs.run(ui);
        if !tag_manager_open
            && !self.close_note_dialog
            && let Some(action) = self.viewer.keyboard(ui)
        {
            self.viewer_action(action, ui.ctx());
        }
        if let Some(message) = self.announcement.take() {
            announce(ui.ctx(), message, Politeness::Polite);
        }
    }
}

fn main() -> eframe::Result {
    if let Some(code) = craft::dwg::serve_if_asked() {
        std::process::exit(code);
    }
    craft::dwg::use_own_worker();
    let backend = Backend::from_env();
    let mut frame = decide_for(Prefix::new("ASSETCRAFT"), backend, TitleBarPref::Integrated);
    if matches!(frame.frame, WindowFrame::Native) {
        frame = decide(
            Os::current(),
            Runtime::current(),
            backend,
            &frame.desktop,
            WindowKind::Main,
            TitleBarPref::Integrated,
            Some("app"),
        );
    }
    let spec = AppSpec::new(
        APP_ID,
        "AssetCraft",
        Defaults::new([1200.0, 760.0], [420.0, 320.0]),
    )
    .frame(frame);
    let catalog = std::env::args_os().nth(1).map(PathBuf::from);
    run(spec, move |ctx| {
        let mut desktop = Desktop::new(ctx);
        if let Some(catalog) = catalog {
            desktop.submit(Operation::Open(catalog), Word::plain("Opening library…"));
        }
        desktop
    })
}
