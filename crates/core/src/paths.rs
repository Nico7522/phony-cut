//! Well-known application folders (logs, settings) and bundled third-party files.

use std::path::{Path, PathBuf};

use directories::BaseDirs;

use crate::APP_NAME;

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("could not determine the user's home directory")]
    NoHomeDir,
}

/// Per-user application data folder.
///
/// Windows: `%APPDATA%\PhonyCut`; macOS: `~/Library/Application Support/PhonyCut`.
pub fn app_data_dir() -> Result<PathBuf, PathsError> {
    let base = BaseDirs::new().ok_or(PathsError::NoHomeDir)?;
    Ok(base.data_dir().join(APP_NAME))
}

/// What [`migrate_legacy_app_data`] did, to log once logging runs.
#[derive(Debug, PartialEq, Eq)]
pub enum Migration {
    /// Nothing to do (new folder already there, or no old folder).
    None,
    /// The old folder was renamed to the new one.
    Moved,
    /// The old folder could not be renamed (a file in use?): settings copied.
    SettingsCopied,
}

/// Takes over the settings folder of the versions named PhoneCut (up to
/// 0.2.0). Call before logging starts (the log file lives in that folder).
pub fn migrate_legacy_app_data() -> Result<Migration, PathsError> {
    let base = BaseDirs::new().ok_or(PathsError::NoHomeDir)?;
    Ok(migrate_folder(
        &base.data_dir().join(crate::LEGACY_APP_NAME),
        &base.data_dir().join(APP_NAME),
    ))
}

fn migrate_folder(old: &Path, new: &Path) -> Migration {
    if new.exists() || !old.is_dir() {
        return Migration::None;
    }
    if std::fs::rename(old, new).is_ok() {
        return Migration::Moved;
    }
    let settings = "settings.json";
    if std::fs::create_dir_all(new).is_ok()
        && std::fs::copy(old.join(settings), new.join(settings)).is_ok()
    {
        Migration::SettingsCopied
    } else {
        Migration::None
    }
}

/// Folder where log files are written.
pub fn log_dir() -> Result<PathBuf, PathsError> {
    Ok(app_data_dir()?.join("logs"))
}

/// A bundled file was not found; lists every place that was tried.
#[derive(Debug, thiserror::Error)]
#[error("bundled file not found (searched: {searched:?})")]
pub struct VendorFileNotFound {
    pub searched: Vec<PathBuf>,
}

/// Finds a file shipped in `vendor/` (e.g. `adb/adb.exe`).
///
/// Order: the `env_override` variable (development), then `vendor/<relative>`
/// next to the executable or in its parent folders. This covers the installed
/// layout, `target/<profile>/` (app) and
/// `target/<profile>/build/<pkg>/<hash>/out/` (test binaries) in development.
pub fn find_vendor_file(
    relative: &Path,
    env_override: &str,
) -> Result<PathBuf, VendorFileNotFound> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os(env_override) {
        candidates.push(PathBuf::from(path));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(Path::parent)
    {
        candidates.extend(vendor_candidates(dir, relative));
    }
    match candidates.iter().find(|p| p.is_file()) {
        Some(path) => Ok(path.clone()),
        None => Err(VendorFileNotFound {
            searched: candidates,
        }),
    }
}

fn vendor_candidates(exe_dir: &Path, relative: &Path) -> Vec<PathBuf> {
    bundled_candidates(exe_dir, "vendor", relative)
}

fn bundled_candidates(exe_dir: &Path, top: &str, relative: &Path) -> Vec<PathBuf> {
    exe_dir
        .ancestors()
        .take(8)
        .map(|dir| dir.join(top).join(relative))
        .collect()
}

/// Finds a file shipped in `assets/` (images…), same search as
/// [`find_vendor_file`]. `None` when missing: assets are optional extras.
pub fn find_asset_file(relative: &Path) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    bundled_candidates(dir, "assets", relative)
        .into_iter()
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_dir_is_inside_app_data_dir() {
        let data = app_data_dir().expect("home dir available in tests");
        let logs = log_dir().expect("home dir available in tests");
        assert!(logs.starts_with(&data));
        assert!(data.ends_with(APP_NAME));
    }

    #[test]
    fn legacy_folder_is_taken_over_once() {
        let root = std::env::temp_dir().join(format!("phonycut-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (old, new) = (root.join("PhoneCut"), root.join("PhonyCut"));
        assert_eq!(migrate_folder(&old, &new), Migration::None, "no old folder");
        std::fs::create_dir_all(old.join("logs")).unwrap();
        std::fs::write(old.join("settings.json"), "{}").unwrap();
        assert_eq!(migrate_folder(&old, &new), Migration::Moved);
        assert!(new.join("settings.json").is_file() && new.join("logs").is_dir());
        assert!(!old.exists());
        // Later starts: the new folder wins, nothing moves again.
        std::fs::create_dir_all(&old).unwrap();
        assert_eq!(migrate_folder(&old, &new), Migration::None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn vendor_candidates_walk_up_from_exe_dir() {
        let exe_dir = Path::new("repo").join("target").join("debug");
        let rel = Path::new("adb").join("adb.exe");
        let candidates = vendor_candidates(&exe_dir, &rel);
        assert_eq!(candidates[0], exe_dir.join("vendor").join(&rel));
        assert!(candidates.contains(&Path::new("repo").join("vendor").join(&rel)));
    }

    #[test]
    fn bundled_files_are_found_from_test_binary() {
        // Test binaries live under target/, inside the repository.
        let adb = Path::new("adb").join(crate::platform::ADB_EXE);
        assert!(find_vendor_file(&adb, "PHONYCUT_TEST_UNSET").is_ok());
        let server = Path::new("scrcpy").join("scrcpy-server");
        assert!(find_vendor_file(&server, "PHONYCUT_TEST_UNSET").is_ok());
    }
}
