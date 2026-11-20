//! Windows: embeds the icon and the version information into PhonyCut.exe.
//! The resource script is generated so the version always comes from Cargo.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let icon = manifest_dir.join("../../assets/icon/phonycut.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(out_dir) = env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };

    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut numbers: Vec<u16> = version
        .split(['.', '-', '+'])
        .take(3)
        .map(|n| n.parse().unwrap_or(0))
        .collect();
    numbers.resize(4, 0);
    let comma = numbers
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let icon_path = icon
        .canonicalize()
        .unwrap_or(icon)
        .display()
        .to_string()
        .trim_start_matches(r"\\?\")
        .replace('\\', "\\\\");

    // Language 0x040C = French (France), code page 1200 = Unicode.
    let rc = format!(
        r#"1 ICON "{icon_path}"
1 VERSIONINFO
FILEVERSION {comma}
PRODUCTVERSION {comma}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040C04B0"
    BEGIN
      VALUE "CompanyName", "PhonyCut"
      VALUE "FileDescription", "PhonyCut"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "phonycut"
      VALUE "OriginalFilename", "PhonyCut.exe"
      VALUE "ProductName", "PhonyCut"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x40C, 1200
  END
END
"#
    );
    let rc_path = out_dir.join("phonycut.rc");
    if let Err(err) = fs::write(&rc_path, rc) {
        println!("cargo:warning=cannot write resource script: {err}");
        return;
    }
    if let Err(err) = embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=icon and version info not embedded: {err}");
    }
}
