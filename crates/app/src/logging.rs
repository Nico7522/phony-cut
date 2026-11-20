//! Logging setup: human-readable logs to stderr and to a file in the app data folder.

use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Context;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

const LOG_FILE_NAME: &str = "phonycut.log";

/// Our crates at `info`; the graphics stack (wgpu, egui backends) only reports problems.
const DEFAULT_FILTER: &str = "info,wgpu_core=warn,wgpu_hal=error,naga=warn,egui_wgpu=warn";

/// Installs the global tracing subscriber and returns the log file path.
///
/// The level can be overridden with the `RUST_LOG` environment variable.
pub fn init() -> anyhow::Result<PathBuf> {
    let dir = phonycut_core::paths::log_dir()?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("cannot create log folder {}", dir.display()))?;
    let path = dir.join(LOG_FILE_NAME);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("cannot open log file {}", path.display()))?;

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(fmt::layer().with_ansi(false).with_writer(Mutex::new(file)))
        .try_init()
        .context("logging already initialised")?;

    Ok(path)
}
