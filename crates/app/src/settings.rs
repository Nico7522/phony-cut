//! User settings, stored as JSON in the app data folder.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where recordings are saved.
    pub output_dir: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_dir: default_output_dir(),
        }
    }
}

/// `Videos\PhoneCut` (the user's Videos folder, wherever it is).
pub fn default_output_dir() -> PathBuf {
    let videos = directories::UserDirs::new().and_then(|dirs| {
        dirs.video_dir()
            .map(Path::to_path_buf)
            .or_else(|| Some(dirs.home_dir().join("Videos")))
    });
    videos
        .unwrap_or_else(|| PathBuf::from("."))
        .join(phonecut_core::APP_NAME)
}

impl Settings {
    /// Loads the settings; missing or unreadable file → defaults.
    pub fn load() -> Self {
        let Ok(path) = settings_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                tracing::warn!(%err, "invalid settings file, using defaults");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = settings_path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))
    }
}

fn settings_path() -> anyhow::Result<PathBuf> {
    Ok(phonecut_core::paths::app_data_dir()?.join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_folder_is_phonecut_in_videos() {
        assert!(default_output_dir().ends_with(phonecut_core::APP_NAME));
    }

    #[test]
    fn json_round_trip_and_missing_fields() {
        let settings = Settings {
            output_dir: PathBuf::from(r"D:\Clips"),
        };
        let text = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), settings);
        // Older/partial files fall back to defaults field by field.
        assert_eq!(
            serde_json::from_str::<Settings>("{}").unwrap(),
            Settings::default()
        );
    }
}
