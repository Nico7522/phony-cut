# 0002 — adb lifecycle and device watching

Date: 2026-11-17 — Status: accepted

## Locating adb

`PHONECUT_ADB` env var (dev override), else `vendor/adb/adb.exe` next to the
executable or in up to three parent folders (installed layout and `target/debug`).

## Watching devices

A `core-service` thread polls `adb devices -l` every second and sends events only on
change. Polling was chosen over `adb track-devices` for simplicity and robustness
(survives adb server restarts without reconnect logic). Manufacturer/model come from
`getprop` once per authorized device and are cached while it stays connected.

Every adb call has a timeout and the child is killed when it expires; output pipes are
read with a grace period because the daemon spawned by `start-server` can inherit them.
On Windows, adb is spawned with `CREATE_NO_WINDOW` (no console flash).

## Stopping the adb server on exit

The adb server is shared by every adb client on the PC (port 5037).

- If **we** started it (`start-server` printed "daemon started successfully"), we run
  `adb kill-server` on exit: a lingering `adb.exe` would lock our install folder
  (uninstall/update) and confuse non-technical users.
- If it was **already running** (another tool), we leave it alone.

## Other adb versions

If adb reports "server version … doesn't match this client", another tool's adb is
running: we emit `Event::AdbConflict` and the UI asks the user to close that tool.
A process-level check (listing other `adb.exe`) may be added later if needed.
