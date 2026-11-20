# 0004 — Recording to MKV without re-encoding

Date: 2026-11-17 — Status: accepted

## Data path

The mirror session thread hands a copy of every packet (config included) to the
`recorder` thread through a channel, then decodes as before. Muxing never blocks the
preview. The recorder is held in the session's shared state, so the service can start /
stop it at any time, even when the screen is static and no packet arrives.

## File

- Matroska via FFmpeg (`matroska` muxer), one H.264 track, packets copied as-is.
  The SPS/PPS config packet becomes the track extradata; FFmpeg converts Annex-B to
  the length-prefixed form MKV expects.
- Starts on the next key frame (≤ 1 s, `i-frame-interval=1`). Before that the UI shows
  "Démarrage de l'enregistrement…". Stopped before any key frame → file deleted,
  `RecordingProblem::NoVideo`.
- Name: `PhonyCut YYYY-MM-DD HH-MM-SS.mkv` in local time (Windows `GetLocalTime`, no
  date crate), `(2)`, `(3)`… if it exists. Folder from settings
  (`%APPDATA%\PhonyCut\settings.json`), default `Videos\PhonyCut`, created on demand.
- Folder choice: native dialog via `rfd` (approved dependency), run on its own thread
  so the UI never blocks; the answer comes back through a channel.

## Timestamps

`Timeline` rebases device PTS to 0 at the first recorded frame. While device deltas
agree with arrival deltas (±1 s) the device clock is used (no network jitter in the
file); otherwise (encoder restart on rotation, clock reset) arrival time is used, so
timestamps stay strictly increasing.

Packets are written one late so each has a real duration; the last one lasts until the
stop instant. Without this, a static screen at the end (the server sends no frames)
would be missing from the file duration.

## Rotation

Same file, same track: the new SPS/PPS are put in front of the next frame. FFmpeg-based
players (VLC) handle the size change; Windows' player to be confirmed on real files.

## Crash robustness

`cluster_time_limit=1000` + `flush_packets=1`: a crash loses about 1 s plus the index
(cues) and the final duration; the file still plays. A clean stop, or the phone being
unplugged, finalizes the file normally.

## Verification

`tests/record_mkv.rs` writes MKV files from a generated stream (rotation + clock
reset + static end) and from the real phone capture, then reads them back with FFmpeg
(container, duration ±2 ms, frame count, strictly increasing timestamps, full decode,
sizes). `examples/inspect_recording.rs` prints the same facts for any recording.
