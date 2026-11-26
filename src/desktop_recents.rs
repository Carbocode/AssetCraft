use assetcraft::{Appearance, Library, local_file};
use craft::settings::{Location, MergeRule, Settings, SettingsError, SettingsStore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    recent_appearances: BTreeMap<String, Appearance>,
}

fn path_key(path: &Path) -> String {
    format!("{:?}", local_file::encode_path(path))
}

impl DesktopSettings {
    fn remember(&mut self, path: PathBuf, library: &Library) {
        self.recent_appearances
            .insert(path_key(&path), library.appearance.clone());
        self.recent_libraries.retain(|entry| entry.path() != path);
        self.recent_libraries.insert(
            0,
            RecentLibrary {
                name: library.name.clone(),
                appearance: library.appearance.clone(),
                path,
            },
        );
        self.recent_libraries.truncate(LIMIT);
        self.recent_appearances.retain(|key, _| {
            self.recent_libraries
                .iter()
                .any(|entry| path_key(entry.path()) == *key)
        });
    }

    fn resolved_recents(&self) -> Vec<RecentLibrary> {
        self.recent_libraries
            .iter()
            .take(LIMIT)
            .cloned()
            .map(|mut entry| {
                if let Some(appearance) = self.recent_appearances.get(&path_key(entry.path())) {
                    entry.appearance = appearance.clone();
                }
                entry
            })
            .collect()
    }
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
            ["recent_appearances", _] => MergeRule::Whole,
            _ => MergeRule::ByKey,
        }
    }
}

pub fn load() -> Vec<RecentLibrary> {
    let (store, _) = SettingsStore::<DesktopSettings>::open(Location::Auto);
    store.get().resolved_recents()
}

pub fn remember(path: &Path, library: &Library) -> Result<Vec<RecentLibrary>, SettingsError> {
    let path = path.canonicalize().or_else(|_| std::path::absolute(path))?;
    let (mut store, _) = SettingsStore::<DesktopSettings>::open(Location::Auto);
    store.update(|settings| settings.remember(path, library));
    store.flush()?;
    Ok(store.get().resolved_recents())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn appearance_changes_merge_with_new_recents() {
        let dir = std::env::temp_dir().join(format!("assetcraft-recents-{}", Uuid::new_v4()));
        let first = dir.join("first.sqlite");
        let second = dir.join("second.sqlite");
        let mut library = Library {
            id: Uuid::new_v4(),
            name: "First".to_owned(),
            created_at_unix_ms: 0,
            appearance: Appearance::default(),
        };

        let (mut current, _) = SettingsStore::<DesktopSettings>::open_at(&dir);
        current.update(|settings| settings.remember(first.clone(), &library));
        current.flush().unwrap();
        let (mut other, _) = SettingsStore::<DesktopSettings>::open_at(&dir);
        other.update(|settings| settings.remember(second.clone(), &library));
        other.flush().unwrap();

        library.appearance.icon = "star".to_owned();
        current.update(|settings| settings.remember(first.clone(), &library));
        current.flush().unwrap();
        let (reopened, _) = SettingsStore::<DesktopSettings>::open_at(&dir);
        let recents = reopened.get().resolved_recents();
        drop((reopened, other, current));
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(recents.len(), 2);
        assert_eq!(recents[0].path(), second);
        assert_eq!(recents[1].path(), first);
        assert_eq!(recents[1].appearance, library.appearance);
    }
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
