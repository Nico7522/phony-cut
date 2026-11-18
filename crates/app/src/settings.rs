//! User settings, stored as JSON in the app data folder.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use phonecut_core::api::VideoQuality;

use crate::texts::Language;
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where recordings are saved.
    pub output_dir: PathBuf,
    pub quality: QualityPreset,
    pub fps: Fps,
    /// The connection assistant was completed once (not shown at startup).
    pub onboarding_done: bool,
    /// Interface language.
    pub language: LanguageChoice,
}

/// The user's language choice; `Auto` follows Windows (French or English).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LanguageChoice {
    #[default]
    Auto,
    French,
    English,
}

impl LanguageChoice {
    pub fn resolve(self) -> Language {
        match self {
            Self::French => Language::French,
            Self::English => Language::English,
            Self::Auto if phonecut_core::platform::system_prefers_french() => Language::French,
            Self::Auto => Language::English,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_dir: default_output_dir(),
            quality: QualityPreset::default(),
            fps: Fps::default(),
            onboarding_done: false,
            language: LanguageChoice::default(),
        }
    }
}

/// Video quality, in words a non-developer understands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityPreset {
    /// 1280 px, 4 Mbit/s: small files, slower PCs.
    Light,
    /// 1920 px, 8 Mbit/s.
    #[default]
    Standard,
    /// Native resolution, 16 Mbit/s.
    High,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fps {
    Thirty,
    #[default]
    Sixty,
}

impl Settings {
    /// What to ask the phone for.
    pub fn video_quality(&self) -> VideoQuality {
        let (max_size, bit_rate) = match self.quality {
            QualityPreset::Light => (1280, 4_000_000),
            QualityPreset::Standard => (1920, 8_000_000),
            QualityPreset::High => (0, 16_000_000),
        };
        VideoQuality {
            max_size,
            bit_rate,
            max_fps: match self.fps {
                Fps::Thirty => 30,
                Fps::Sixty => 60,
            },
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
            // Windows tools often add a UTF-8 BOM, which JSON parsers reject.
            Ok(text) => {
                serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_else(|err| {
                    tracing::warn!(%err, "invalid settings file, using defaults");
                    Self::default()
                })
            }
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
    fn presets_map_to_stream_settings() {
        let mut s = Settings::default();
        assert_eq!(
            s.video_quality(),
            VideoQuality::default(),
            "standard = core default"
        );
        s.quality = QualityPreset::High;
        s.fps = Fps::Thirty;
        let q = s.video_quality();
        assert_eq!((q.max_size, q.bit_rate, q.max_fps), (0, 16_000_000, 30));
    }

    #[test]
    fn language_choice_is_stored() {
        let s: Settings = serde_json::from_str(r#"{ "language": "English" }"#).unwrap();
        assert_eq!(s.language, LanguageChoice::English);
        assert_eq!(s.language.resolve(), Language::English);
        assert_eq!(LanguageChoice::French.resolve(), Language::French);
        assert_eq!(Settings::default().language, LanguageChoice::Auto);
    }

    #[test]
    fn phase_3_settings_file_still_loads() {
        let old = r#"{ "output_dir": "D:\\Clips" }"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert_eq!(s.output_dir, PathBuf::from(r"D:\Clips"));
        assert_eq!(s.quality, QualityPreset::Standard);
        assert!(!s.onboarding_done);
    }

    #[test]
    fn json_round_trip_and_missing_fields() {
        let settings = Settings {
            output_dir: PathBuf::from(r"D:\Clips"),
            quality: QualityPreset::Light,
            fps: Fps::Thirty,
            onboarding_done: true,
            language: LanguageChoice::English,
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
