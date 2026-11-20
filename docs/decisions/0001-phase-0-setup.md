# 0001 — Phase 0 setup choices

Date: 2026-11-17 — Status: accepted

## UI toolkit

`eframe`/`egui` (not Tauri). The whole architecture (GPU texture for video, `core`
unaware of the UI, channels between both) assumes an immediate-mode Rust UI.

## Crate naming

The `crates/core` folder holds the package `phonycut-core` (lib `phonycut_core`):
a package cannot be named `core` because it would shadow Rust's built-in `core` crate.
The UI package is named `app` (so `cargo run -p app` works) and builds the
`phonycut` executable.

## Pinned scrcpy version

scrcpy server **v5.0.1** (latest stable at setup time). See `vendor/VERSIONS.md`.

## Toolchain

eframe 0.37 requires Rust ≥ 1.95. Developed on stable 1.100 (MSVC).

## Logging

`tracing-subscriber` with two layers: stderr and an append-only file
`%APPDATA%\PhonyCut\logs\phonycut.log` (no extra crate; `tracing-appender` could be
added later for rotation). Level overridable with `RUST_LOG`.
