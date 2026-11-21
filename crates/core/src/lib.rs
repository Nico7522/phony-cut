//! PhonyCut core: device access, scrcpy protocol, recording and export.
//!
//! This crate must never depend on any UI toolkit. The UI talks to it through
//! channels (commands in, events out), see [`api`] and [`service::Core`].

pub mod adb;
pub mod api;
pub mod audio;
pub mod audio_out;
pub mod decode;
pub mod export;
pub mod filming;
pub mod image;
pub mod markers;
pub mod normalize;
pub mod pairing;
pub mod paths;
pub mod platform;
pub mod player;
pub mod preview;
pub mod record;
pub mod replay;
pub mod scrcpy;
pub mod service;
pub mod tcpip;
pub mod update;

/// Application name, used for folders and window titles.
pub const APP_NAME: &str = "PhonyCut";

/// Name up to version 0.2.0: its folders (settings, videos) are taken over.
pub const LEGACY_APP_NAME: &str = "PhoneCut";
