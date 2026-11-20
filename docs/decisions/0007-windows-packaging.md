# 0007 — Windows packaging

Date: 2026-11-18 — Status: accepted

## Installer: Inno Setup 6 (not cargo-dist)

Chosen with the product owner. The app is not a single executable: it ships FFmpeg
DLLs, adb, scrcpy-server, tutorial pictures and licenses. Inno Setup lists them in a
short script (`installer/phonycut.iss`), installs **per user without administrator
rights** (`PrivilegesRequired=lowest` → `%LOCALAPPDATA%\Programs\PhonyCut`, no UAC
prompt), and is preinstalled on GitHub's Windows runners. cargo-dist targets single
binaries; bundling these files in its WiX template meant maintaining generated XML.

## Layout

```
PhonyCut.exe + FFmpeg DLLs        (DLLs must sit next to the exe)
vendor\adb\, vendor\scrcpy\       (found by paths::find_vendor_file)
vendor\licenses\, vendor\VERSIONS.md
assets\tutorial\, assets\icon\    (found by paths::find_asset_file)
```

## Details

- `AppId` must never change (upgrades). `AppMutex=PhonyCutAppMutex` matches
  `platform::SINGLE_INSTANCE_MUTEX`: the app refuses a second instance (two previews on
  one phone conflict) and the installer asks to close a running PhonyCut.
- adb server stopped before upgrading (`PrepareToInstall`) and on uninstall, otherwise
  `adb.exe` stays locked.
- Uninstall removes `%APPDATA%\PhonyCut` (settings, logs); recordings are never touched.
- Icon from the product owner's logo `assets/icon/PhyCut_logo_software.svg` (a PNG
  wrapped in SVG). `scripts/make-icon.ps1` renders it with headless Microsoft Edge (no
  SVG library in the project), then `examples/make_icon.rs` crops it, puts it on a
  white rounded tile with a light grey border (the black drawing alone vanished on dark
  taskbars; approved by the product owner) and downsamples (area average, premultiplied alpha) to
  `assets/icon/phonycut.ico` (16–256 px) and `assets/icon/phonycut-256.rgba` (window
  icon, embedded with `include_bytes!`). Generated files are committed; building does not
  need Edge. The `.ico` is embedded with `embed-resource` (approved) together with
  version info generated from Cargo's version.
- Release profile: thin LTO, stripped. `windows_subsystem = "windows"` (no console).
- Licenses: scrcpy (Apache 2.0), FFmpeg (LGPL, dynamic), platform-tools NOTICE, and a
  generated list of Rust crates (`scripts/third-party-licenses.ps1`). "À propos" in the
  settings shows the credits and opens the folder.

## Release pipeline

`.github/workflows/release.yml`, on `v*` tags: fmt, clippy, tests, then
`scripts/build-installer.ps1` (also usable locally), which refuses a tag that does not
match `Cargo.toml`, and publishes `PhonyCut-Setup-x64.exe` + `SHA256SUMS.txt` (no version in the name since the website links to `releases/latest/download/PhonyCut-Setup-x64.exe`; the version is in the release title and the installer itself).
(The hash was first written to `PhonyCut-Setup-<version>.exe.sha256`: with file
extensions hidden, Windows shows it as "PhonyCut-Setup-<version>.exe", and a tester
opened it instead of the installer.)

## Not done in v1 / open points

- **Code signing**: none; SmartScreen warning documented in `docs/installation.md` and
  the release notes.
- **Platform-tools redistribution**: adb is shipped (product owner's choice); its license
  terms must still be checked before a public release.
- **H.264 patents**: the bundled FFmpeg includes an openh264 built from source (not
  Cisco's binary, so Cisco's patent coverage does not apply). To review before a wide
  distribution, as for the H.264 decoding itself.
- Rust crate licenses are listed, not reproduced in full (`cargo-about` could generate
  the full texts later).
