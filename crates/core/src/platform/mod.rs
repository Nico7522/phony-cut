//! Platform-specific helpers. All OS-specific code lives in this module.

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
