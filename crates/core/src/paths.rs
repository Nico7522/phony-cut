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
/// Windows: `%APPDATA%\PhoneCut`; macOS: `~/Library/Application Support/PhoneCut`.
pub fn app_data_dir() -> Result<PathBuf, PathsError> {
    let base = BaseDirs::new().ok_or(PathsError::NoHomeDir)?;
    Ok(base.data_dir().join(APP_NAME))
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
    exe_dir
        .ancestors()
        .take(8)
        .map(|dir| dir.join("vendor").join(relative))
        .collect()
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
        assert!(find_vendor_file(&adb, "PHONECUT_TEST_UNSET").is_ok());
        let server = Path::new("scrcpy").join("scrcpy-server");
        assert!(find_vendor_file(&server, "PHONECUT_TEST_UNSET").is_ok());
    }
}
