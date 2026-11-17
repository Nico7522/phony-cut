# 0003 — Live preview pipeline

Date: 2026-11-17 — Status: accepted

## Protocol

Implemented from scrcpy **v5.0.1** sources, not from memory (`doc/develop.md` at that
tag still says "documents scrcpy 4.0" but matches the code). Differences from what the
project brief assumed: video size comes in a 12-byte *session packet* (sent at start and
on every capture reset, e.g. rotation), and `scid` is hexadecimal.

Forward tunnel (`tunnel_forward=true`) with `adb forward tcp:0`: adb picks the free
port, no race on a port we chose ourselves.

## Threads

- `core-service`: adb polling and commands.
- `mirror-session`: server process, socket reading **and** decoding in one loop.
  A software H.264 decode at ≤1920 px takes a few ms, far below the frame interval, so a
  separate decode thread would add complexity for no latency gain. Revisit if the log's
  `mirror stats` show `avg_decode_ms` close to the frame interval.
- `scrcpy-server-log`: forwards the server's console output to our log.

## Latency choices

- Decoder: single thread (frame threading delays output by N frames) + `LOW_DELAY`.
  The end-to-end test checks that each packet yields its frame immediately.
- `FrameSlot` keeps only the latest frame; the UI never drains a backlog.
- `TCP_NODELAY` on the socket.
- RGBA conversion on the CPU (swscale) and a full texture upload per frame. Simple and
  fast enough at ≤1920 px; a GPU YUV shader is the next step if needed.
- **Debug builds must optimize dependencies** (`[profile.dev.package."*"] opt-level = 3`):
  egui's per-pixel `ColorImage` conversion took ~190 ms per 862×1920 frame unoptimized
  (preview at ~5 fps), ~2 ms optimized. `from_rgba_premultiplied` is used (frames are
  opaque, so it is equivalent and ~3× cheaper than `from_rgba_unmultiplied`).
- Logs: `mirror stats` (decode time, `dropped_before_display` = frames overwritten
  before the UI showed them) and `display stats` (GPU upload time) every 10 s.

## Rotation

On a session packet the decoder is recreated (the server restarts its encoder) and a new
`MirrorState::Running` with the new size is sent; the texture simply takes the size of
the next frame.

## Disconnection

When the stream ends, `adb get-state` is checked up to 3 times (400 ms apart): gone →
`MirrorEnd::Disconnected`, still there → `MirrorEnd::Failed`. The UI starts the mirror
automatically once per connection of a ready phone, and offers "Relancer l'aperçu" after
an interruption (no automatic retry loop).

## Defaults

`max_size=1920`, 8 Mbit/s, 60 fps max, key frame every second. Configurable in phase 5.

## FFmpeg distribution

`vendor/ffmpeg/` is not versioned (too large); `scripts/fetch-ffmpeg.ps1` downloads the
pinned BtbN LGPL-shared build and checks its SHA-256. `cargo run`/`cargo test` use a
runner script that adds `vendor\ffmpeg\bin` to PATH; `build.rs` also copies the DLLs to
`target/<profile>/` so the exe can be launched directly.
