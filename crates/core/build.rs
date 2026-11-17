//! Copies the FFmpeg DLLs into `target/<profile>/`, next to the app
//! executable, so it can be launched directly. (`cargo run`/`cargo test` also
//! get them through the runner in `.cargo/config.toml`.)

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let ffmpeg_dir = env::var_os("FFMPEG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("../../vendor/ffmpeg"));
    let bin_dir = ffmpeg_dir.join("bin");
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    println!("cargo:rerun-if-changed={}", bin_dir.display());

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out (classic layout) or
    // target/<profile>/build/<pkg>/<hash>/out (newer layout): the profile
    // folder is the parent of the nearest `build` folder.
    let Some(profile_dir) = env::var_os("OUT_DIR").map(PathBuf::from).and_then(|out| {
        out.ancestors()
            .find(|dir| dir.file_name().is_some_and(|n| n == "build"))
            .and_then(Path::parent)
            .map(Path::to_path_buf)
    }) else {
        println!("cargo:warning=cannot determine target directory; FFmpeg DLLs not copied");
        return;
    };

    let Ok(entries) = fs::read_dir(&bin_dir) else {
        println!(
            "cargo:warning=FFmpeg DLLs not found in {} (run scripts/fetch-ffmpeg.ps1)",
            bin_dir.display()
        );
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
        {
            if let Some(name) = path.file_name() {
                if let Err(err) = fs::copy(&path, profile_dir.join(name)) {
                    println!("cargo:warning=could not copy {}: {err}", path.display());
                }
            }
        }
    }
}
