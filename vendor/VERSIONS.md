# Third-party files bundled in `vendor/`

| Path | Version | Source | SHA-256 | License |
| --- | --- | --- | --- | --- |
| `scrcpy/scrcpy-server` | **v5.0.1** (released 2026-10-08) | https://github.com/Genymobile/scrcpy/releases/download/v5.0.1/scrcpy-server-v5.0.1 | `764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536` (matches upstream `SHA256SUMS.txt`) | Apache 2.0 — `licenses/scrcpy-LICENSE-Apache-2.0.txt` |
| `adb/adb.exe`, `adb/AdbWinApi.dll`, `adb/AdbWinUsbApi.dll` | Android SDK Platform-Tools **37.0.1** (adb 1.0.41, build 15733141) | https://dl.google.com/android/repository/platform-tools-latest-windows.zip (zip SHA-256 `45f4d63113e895ebde0c90f194099a4676b6ac653bd28d54314a9e022bbc1a99`) | — | See `licenses/platform-tools-NOTICE.txt` |
| `ffmpeg/` (**not in git**, run `scripts/fetch-ffmpeg.ps1`) | FFmpeg **n9.0.2-57-gd838f9f9ae**, win64 **LGPL shared** (BtbN `autobuild-2026-11-17-12-40`) | https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-11-17-12-40/ffmpeg-n9.0.2-57-gd838f9f9ae-win64-lgpl-shared-9.0.zip | `8388f846a8b56a9316ad8f24d9eedf186097a59555816b7b222de392611b0279` (zip, matches upstream `checksums.sha256`) | LGPL 2.1+ — `licenses/ffmpeg-LICENSE-LGPL.txt` (copied by the script) |

## Rules

- The scrcpy server version passed on the command line **must equal** the version above
  (`5.0.1`). The protocol is version-specific: read `doc/develop.md` at tag `v5.0.1`
  before touching the protocol code.
- Updating any file here: update this table (version, URL, hash) and the license file in
  the same commit.
- Platform-tools: redistribution checked in `docs/decisions/0013-adb-redistribution.md`
  (ship only `adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll`, unmodified, with the zip's
  `NOTICE.txt`).
- FFmpeg: only `lgpl-shared` builds (the fetch script refuses anything else). BtbN
  prunes old autobuilds; when the pinned URL disappears, pick a newer autobuild of the
  same FFmpeg major and update the script, this table and the hash together.
