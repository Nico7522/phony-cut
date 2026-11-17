//! Platform-specific helpers. All OS-specific code lives in this module.

use std::io;
use std::path::Path;
use std::process::Command;

/// File name of the adb executable on this platform.
pub const ADB_EXE: &str = if cfg!(windows) { "adb.exe" } else { "adb" };

/// Prevents a console window from flashing when a GUI app spawns a
/// command-line program (Windows only; no-op elsewhere).
pub fn hide_console_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // https://learn.microsoft.com/windows/win32/procthread/process-creation-flags
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Opens a folder in the system file manager.
pub fn open_in_file_manager(folder: &Path) -> io::Result<()> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(program).arg(folder).spawn().map(|_| ())
}

/// Local wall-clock date and time (for file names shown to the user).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// Current local date and time.
#[cfg(windows)]
pub fn local_now() -> LocalDateTime {
    /// `SYSTEMTIME` from `minwinbase.h`.
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(system_time: *mut SystemTime);
    }

    let mut st = SystemTime::default();
    // SAFETY: GetLocalTime only writes a SYSTEMTIME into the valid, properly
    // aligned and #[repr(C)] struct we pass; it cannot fail.
    unsafe { GetLocalTime(&mut st) };
    LocalDateTime {
        year: st.year,
        month: st.month as u8,
        day: st.day as u8,
        hour: st.hour as u8,
        minute: st.minute as u8,
        second: st.second as u8,
    }
}

/// Current date and time (UTC until the macOS port adds local time).
#[cfg(not(windows))]
pub fn local_now() -> LocalDateTime {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    utc_from_unix(secs)
}

/// Converts a Unix timestamp to a UTC civil date (Howard Hinnant's algorithm).
#[cfg_attr(windows, allow(dead_code))]
fn utc_from_unix(secs: u64) -> LocalDateTime {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
    LocalDateTime {
        year,
        month,
        day,
        hour: (rem / 3600) as u8,
        minute: (rem % 3600 / 60) as u8,
        second: (rem % 60) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_now_is_plausible() {
        let now = local_now();
        assert!(now.year >= 2024);
        assert!((1..=12).contains(&now.month));
        assert!((1..=31).contains(&now.day));
        assert!(now.hour < 24 && now.minute < 60 && now.second < 61);
    }

    #[test]
    fn utc_conversion() {
        // 2026-11-17 15:39:41 UTC
        assert_eq!(
            utc_from_unix(1_794_929_981),
            LocalDateTime {
                year: 2026,
                month: 11,
                day: 17,
                hour: 15,
                minute: 39,
                second: 41
            }
        );
        assert_eq!(utc_from_unix(0).year, 1970);
    }
}
