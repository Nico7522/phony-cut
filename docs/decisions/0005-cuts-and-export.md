# 0005 — Cut markers, "Mes clips" and export

Date: 2026-11-17 — Status: accepted

## Markers and segments

- While recording, "Marquer un cut" (button or **M**) asks the recorder thread, which
  owns the file timeline, to convert "now" into file time. The marker is appended to
  `<video stem>.markers.json` immediately (temp file + rename: crash-safe).
- Sidecar format (`version: 1`): `markers` (cut points, ms) and `segments` (`null` until
  the user edits them). Default segments = pieces between markers, `Clip 1…n`; markers
  closer than 500 ms to each other or to the ends are ignored. Minimum segment: 500 ms.
- Edits (in/out, name, selection, "Couper ici", reset) are saved 500 ms after the last
  change, when switching video or tab, and when the window closes.

## Player

`player.rs`, own thread, frames paced on the wall clock and published to a dedicated
`FrameSlot`. Seeking is frame-accurate: container seek to the key frame at or before
the target, then decode forward without converting intermediate frames. Consecutive
seeks (timeline drag) are coalesced. The live preview keeps running in the background
but its frames are not uploaded while "Mes clips" is shown.

## Export (`.mp4`, `+faststart`)

- **Fast** (default): stream copy. Starts on the key frame at or before the in point
  (≤ ~1 s earlier with `i-frame-interval=1`), ends exactly before the out point (MediaCodec
  emits no B-frames). Instant, no quality loss.
- **Precise**: decode + re-encode the **whole clip** with `libopenh264` (BSD, included in
  the LGPL FFmpeg build; x264 is GPL and excluded). Edge-only re-encoding ("smart render")
  was rejected for v1: the re-encoded head would carry SPS/PPS different from the copied
  part, which MP4 (single `avcC`) and some players (Windows Media Foundation) handle
  badly. Bit rate = source × 1.25 (10 Mbit/s if unknown).
- A rotation inside a precise clip: frames are fitted without distortion, with black
  bars, in the size of the clip's first frame. Fast export keeps the size changes as-is.
- **Durations**: MKV blocks carry no duration, so both exporters write packets one late
  and give the last one the time until the out point (or the end of the video). Without
  this, a clip ending on a static screen lost its tail (bug found on a real recording).
- Output: `<video stem> - clips/NN - <name>.mp4` next to the recording, names made safe
  for Windows. Progress, cancel (partial file removed), one export at a time.

## Verification

`tests/export_play.rs`: fast/precise boundaries and durations, static-screen tail, three
clips from two markers in both modes, cancellation, player load + frame-accurate seek +
end. Manual: export from real recordings checked with `inspect_recording`.
