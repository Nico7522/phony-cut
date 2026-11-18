# 0009 — Using the phone with the mouse and keyboard

Date: 2026-11-18 — Status: accepted

## Protocol (scrcpy v5.0.1)

- The server runs with control enabled (default) and `clipboard_autosync=false`.
- Second socket on the same forward port, opened right after the dummy byte of the video
  socket. The server sends the device name only once every socket is accepted, so the
  control socket must be opened before reading anything else (otherwise both sides wait).
- Messages (`scrcpy/control.rs`): inject keycode, inject text (≤ 300 bytes, cut on UTF-8
  boundaries, long texts split), inject touch (pointer id = mouse, -1), inject scroll
  (ticks in [-16, 16] → i16 fixed point), back-or-screen-on. Serialization is checked
  against the byte vectors of scrcpy's own `test_control_msg_serialize.c`.
- A writer thread sends queued messages; a reader thread drains the device messages (an
  unread socket could block the server). Both stop with the session.
- Positions carry the frame size; the server ignores events made for an outdated size
  (rotation race), so no extra synchronization is needed.

## Mapping (same conventions as the official client)

- Left button on the picture: finger (down / move / up, drags continue outside the
  picture until release). Right button: Back (`BACK_OR_SCREEN_ON`, also wakes the
  screen). Middle button: Home. Wheel: scroll (egui's horizontal delta is the content
  movement, so it is used as-is for "scroll left = positive").
- Keyboard only while the picture has the focus (click on it; click elsewhere to give it
  back; a blue frame shows it). Printable characters go as text (PC layout and accents,
  AltGr), editing/navigation keys as Android keycodes, Ctrl/Alt+letter as shortcuts.
  Ctrl+V pastes the PC clipboard into the phone (as text); Ctrl+C/X act on the phone.
  While the picture has the focus, M types "m" on the phone instead of marking a cut.
- Buttons above the picture: Back, Home, Recent apps, Volume −/+.

## Not done

- Phone → PC clipboard (device messages are drained, not parsed).
- Multi-touch gestures (pinch), physical keyboard emulation (UHID), hover events.
