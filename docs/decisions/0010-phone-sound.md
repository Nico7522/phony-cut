# 0010 — Capturing the phone's sound

Date: 2026-11-20 — Status: accepted (steps 1–3 done; export, "My clips" playback and the
Sound setting follow)

## Choices (product owner)

- Sound **only on the PC** while mirroring: the phone goes silent (no `audio_dup`).
- **AAC** from the phone: copied as-is into the recordings and clips, plays everywhere in
  an MP4.
- Played on the PC with **`cpal`** (WASAPI on Windows, portable to macOS).
- A **Sound** setting will allow turning capture / listening off.

## Protocol (scrcpy v5.0.1, checked in the sources and on a real phone)

- `audio=true audio_codec=aac audio_source=<source>`; sockets in the order video, audio,
  control (dummy byte only on the first).
- Audio socket: codec id (`0` = the phone cannot share its sound, video continues; `1` =
  error), then the same 12-byte media headers as video, no session packets. First packet:
  AudioSpecificConfig (`11 90` = AAC-LC, 48 kHz, stereo), then raw AAC frames of 1024
  samples (21.333 ms).
- **Audio and video PTS share the same device clock** (measured: a few tens of ms apart),
  so no resynchronization is needed.

## Audio source

- `output` (REMOTE_SUBMIX, scrcpy default) returned **pure silence** on a Xiaomi POCO X7
  Pro (Android 16) for both a music app and YouTube; `playback` (AudioPlaybackCapture)
  works.
- So the source follows the Android version (`ro.build.version.sdk`): `playback` on
  Android 13+ (API 33), `output` on Android 11–12, no sound before Android 11.
- Side effects: apps may opt out of playback capture (their sound is missing); when the
  capture stops, some players (YouTube) pause, as when headphones are unplugged.

## Live playback

- FFmpeg's native AAC decoder + swresample to the device's format; `cpal` stream on its
  own thread (streams are not `Send` everywhere).
- Jitter buffer: start after 60 ms; when more than 200 ms piles up, drop down to 80 ms
  (keeps the sound in sync with the picture); silence and re-buffering when it runs dry.
- The audio socket is always read, even when the sound is not played (an unread socket
  could block the server).

## Recording

- Second Matroska track (AAC, extradata = AudioSpecificConfig), packets copied.
- File time: the sound's offset from the first recorded video frame (same clock), then its
  own continuity (the video encoder clock may restart on rotation, the audio one does not).
  Sound before the first recorded image is dropped.
- Both tracks go through FFmpeg's interleaving queue (`max_interleave_delta` 1 s, so a
  crash still loses about 1 s at most).
