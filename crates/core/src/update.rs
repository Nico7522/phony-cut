//! Is a newer PhonyCut published? Asks GitHub for the latest release with the
//! system `curl` (shipped with Windows 10/11 and macOS: no HTTP library).
//!
//! Only `api.github.com/repos/<repo>/releases/latest` is contacted, with a
//! generic user agent and nothing about the user.

use std::process::{Command, Stdio};
use std::time::Duration;

use crate::platform;

/// Development: pretend to be this version (e.g. `0.1.0`) to see the update
/// banner; the once-a-day limit is ignored too.
pub const PRETEND_VERSION_ENV: &str = "PHONYCUT_PRETEND_VERSION";

/// Version compared with the latest release.
pub fn current_version() -> String {
    std::env::var(PRETEND_VERSION_ENV).unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned())
}

/// The public repository the releases come from.
pub const RELEASES_API: &str = "https://api.github.com/repos/Nico7522/phony-cut/releases/latest";
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("cannot run curl: {0}")]
    Curl(#[from] std::io::Error),
    #[error("GitHub did not answer ({0})")]
    Http(String),
    #[error("unexpected answer: {0}")]
    Answer(String),
}

/// The latest published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// e.g. `0.4.0` (tag without the `v`).
    pub version: String,
}

/// Asks GitHub for the latest release (blocking, up to 15 s).
pub fn latest_release() -> Result<Release, UpdateError> {
    let mut cmd = Command::new("curl");
    cmd.args([
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--max-time",
        &TIMEOUT.as_secs().to_string(),
        "--header",
        "Accept: application/vnd.github+json",
        "--user-agent",
        "PhonyCut-update-check",
        RELEASES_API,
    ])
    .stdin(Stdio::null());
    platform::hide_console_window(&mut cmd);
    let output = cmd.output()?;
    if !output.status.success() {
        return Err(UpdateError::Http(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    parse_release(&String::from_utf8_lossy(&output.stdout))
}

fn parse_release(json: &str) -> Result<Release, UpdateError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| UpdateError::Answer(e.to_string()))?;
    let tag = value
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| UpdateError::Answer("no tag_name".into()))?;
    Ok(Release {
        version: tag.trim_start_matches('v').to_owned(),
    })
}

/// `major.minor.patch` as numbers; anything else (pre-release suffix…) is
/// ignored after the numbers.
fn numbers(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim_start_matches('v').split('.').map(|p| {
        p.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse::<u64>()
            .ok()
    });
    Some((
        parts.next()??,
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
    ))
}

/// True when `latest` is a newer version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (numbers(latest), numbers(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(is_newer("0.4.0", "0.3.0"));
        assert!(is_newer("v0.10.0", "0.9.9"), "10 > 9, not text order");
        assert!(is_newer("1.0.0", "0.99.0"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.1", "0.3.0"));
        assert!(!is_newer("garbage", "0.3.0"));
    }

    #[test]
    fn release_answer_is_read() {
        let json =
            r#"{"tag_name":"v0.3.0","name":"PhonyCut v0.3.0","html_url":"https://github.com/x"}"#;
        assert_eq!(parse_release(json).unwrap().version, "0.3.0");
        assert!(parse_release(r#"{"message":"Not Found"}"#).is_err());
    }
}
