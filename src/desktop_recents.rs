use assetcraft::{Appearance, Library, local_file};
use craft::settings::{Location, MergeRule, Settings, SettingsError, SettingsStore};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const LIMIT: usize = 12;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentLibrary {
    pub name: String,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(with = "recent_path")]
    path: PathBuf,
}

impl RecentLibrary {
    pub fn path(&self) -> &Path {
        &self.path
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
                path: path.to_path_buf(),
            },
        );
        settings.recent_libraries.truncate(LIMIT);
    });
    store.flush()?;
    Ok(store.get().recent_libraries.clone())
}

mod recent_path {
    use super::{Path, PathBuf, local_file};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        local_file::encode_path(path).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        local_file::decode_path(bytes)
            .ok_or_else(|| serde::de::Error::custom("Invalid library path"))
    }
}
