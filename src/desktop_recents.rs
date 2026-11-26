use assetcraft::{Appearance, Library};
use craft::settings::{Location, MergeRule, Settings, SettingsError, SettingsStore};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

const LIMIT: usize = 12;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentLibrary {
    pub name: String,
    #[serde(default)]
    pub appearance: Appearance,
    path: Vec<u8>,
}

impl RecentLibrary {
    pub fn path(&self) -> &Path {
        Path::new(OsStr::from_bytes(&self.path))
    }
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct DesktopSettings {
    recent_libraries: Vec<RecentLibrary>,
}

impl Settings for DesktopSettings {
    const APP_ID: &'static str = super::APP_ID;
    const SCHEMA_VERSION: u32 = 1;

    fn merge_rule(path: &[&str]) -> MergeRule {
        match path {
            ["recent_libraries"] => MergeRule::Recent {
                cap: LIMIT,
                key: |entry| {
                    entry
                        .as_inline_table()?
                        .get("path")
                        .map(ToString::to_string)
                },
            },
            _ => MergeRule::ByKey,
        }
    }
}

pub fn load() -> Vec<RecentLibrary> {
    let (store, _) = SettingsStore::<DesktopSettings>::open(Location::Auto);
    store
        .get()
        .recent_libraries
        .iter()
        .take(LIMIT)
        .cloned()
        .collect()
}

pub fn remember(path: &Path, library: &Library) -> Result<Vec<RecentLibrary>, SettingsError> {
    let canonical = path.canonicalize();
    let path = canonical.as_deref().unwrap_or(path);
    let (mut store, _) = SettingsStore::<DesktopSettings>::open(Location::Auto);
    store.update(|settings| {
        settings
            .recent_libraries
            .retain(|entry| entry.path() != path);
        settings.recent_libraries.insert(
            0,
            RecentLibrary {
                name: library.name.clone(),
                appearance: library.appearance.clone(),
                path: path.as_os_str().as_bytes().to_vec(),
            },
        );
        settings.recent_libraries.truncate(LIMIT);
    });
    store.flush()?;
    Ok(store.get().recent_libraries.clone())
}
