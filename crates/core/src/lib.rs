//! PhoneCut core: device access, scrcpy protocol, recording and export.
//!
//! This crate must never depend on any UI toolkit. The UI talks to it through
//! channels (commands in, events out), see [`api`] and [`service::Core`].

pub mod adb;
pub mod api;
pub mod decode;
pub mod paths;
pub mod platform;
pub mod record;
pub mod scrcpy;
pub mod service;

/// Application name, used for folders and window titles.
pub const APP_NAME: &str = "PhoneCut";
