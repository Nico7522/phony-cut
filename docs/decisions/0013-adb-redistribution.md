# 0013 — Redistribution of adb (Android SDK Platform-Tools)

- Status: accepted
- Date: 2026-11-22

## Context

PhonyCut ships three files of Google's prebuilt Android SDK Platform-Tools for Windows
(`vendor/adb/`): `adb.exe`, `AdbWinApi.dll` and `AdbWinUsbApi.dll`. The CLAUDE.md and
`vendor/VERSIONS.md` asked to check the redistribution terms before the first public
release. That check was not done before 0.1.0; it is done here, before 1.0.

## What the texts say

The platform-tools download page makes you accept the **Android Software Development Kit
License Agreement** (version of April 28, 2026; same wording as
<https://developer.android.com/studio/terms> for the clauses below):

- **3.4**: "Except to the extent required by applicable third party licenses, you may not
  copy (except for backup purposes), modify, adapt, redistribute, decompile, reverse
  engineer, disassemble, or create derivative works of the SDK or any part of the SDK."
- **3.5**: "Use, reproduction and distribution of components of the SDK licensed under an
  open source software license are governed solely by the terms of that open source
  software license and not the License Agreement."
- **3.8**: proprietary rights notices must not be removed or altered.

The `NOTICE.txt` Google ships **inside** the platform-tools zip (kept as
`vendor/licenses/platform-tools-NOTICE.txt`) lists the licenses of its content: the
Apache License 2.0 (Android Open Source Project), plus BSD, MIT, ISC, Unicode, LLVM and
similar permissive licenses for libraries, and GPL/LGPL only for the filesystem tools
(`mke2fs`, `make_f2fs`), which PhonyCut does **not** ship.

The three files PhonyCut ships are built from AOSP sources under the Apache License 2.0:

- `adb.exe`: <https://android.googlesource.com/platform/packages/modules/adb/>
- `AdbWinApi.dll`, `AdbWinUsbApi.dll`:
  <https://android.googlesource.com/platform/development/+/refs/heads/main/host/windows/usb/>

scrcpy (Apache 2.0, Genymobile) ships the same three files in its Windows release
("`adb.exe` is included in the release", scrcpy FAQ), as does the WinGet package.

## Decision

Keep shipping the three prebuilt files, as open-source components covered by section 3.5:
their distribution is governed by the Apache License 2.0, not by the SDK License
Agreement's restriction (3.4).

What Apache 2.0 (section 4) asks, and how PhonyCut meets it:

| Obligation | How |
| --- | --- |
| Give a copy of the license | Apache 2.0 text in `vendor/licenses/platform-tools-NOTICE.txt` |
| Keep the NOTICE file | the full `NOTICE.txt` of the zip, shipped by the installer (`vendor\licenses`) |
| Keep copyright notices | the files are shipped unmodified; `THIRD_PARTY_LICENSES.md` names the AOSP |
| State changes | none: the files are not modified (SHA-256 of the zip in `VERSIONS.md`) |

Rules:

- Never ship other platform-tools files (`fastboot`, `mke2fs`, `make_f2fs`…): some are GPL
  and none is needed.
- Never modify the binaries; update them only from Google's official zip, recording the
  version and hash in `vendor/VERSIONS.md`, with its `NOTICE.txt`.
- No use of Google or Android logos; "Android" is only used to say which phones work.

## Alternatives considered

- **Build adb from the AOSP sources** in CI: removes any doubt about the prebuilt
  binaries, but the AOSP build is heavy (Soong, several GB) and would need maintenance at
  every adb update. Kept as a fallback if Google ever objects.
- **Download adb on the user's PC at install time**, from Google: the user would accept
  Google's terms themselves, but it needs the network, a download that can break, and a
  licence screen most users do not understand. Worse for the target audience.

## Residual risk

The SDK License Agreement does not list which of its parts are open source; the reading
above relies on the NOTICE shipped with the zip and on the AOSP sources. The risk is low
(same practice as scrcpy and WinGet for years), and the fallback is known.
This is not legal advice; the owner may ask a lawyer before 1.0 if wanted.
