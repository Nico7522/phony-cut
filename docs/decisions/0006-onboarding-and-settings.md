# 0006 — Connection assistant, main screen and settings

Date: 2026-11-17 — Status: accepted

## Connection assistant

- Four steps: developer options, USB debugging, cable, authorization. Shown at first
  launch (`settings.onboarding_done`), reopened from "Aide à la connexion" (home screen
  when no phone is ready) or from the settings.
- Validation from adb state (`auto_step`): steps 1–2 cannot be detected (without USB
  debugging the phone is invisible to the PC), so the user moves through them with
  "Suivant"; as soon as the phone appears the assistant jumps to "Autorisation"
  (unauthorized) or "C'est prêt" (ready); unplugging goes back to "Câble".
- Every step says that menus vary by brand, that the pictures show a Xiaomi, and to
  search the web with the phone model if needed (product owner's request). A brand
  selector gives the usual menu path for Xiaomi/Redmi/POCO, Samsung, Pixel,
  Oppo/OnePlus/realme and a generic one.
- "Ça ne marche pas ?" per step: charge-only cables, rear ports / no hub, "Transfert de
  fichiers" USB mode, debugging disabled by updates, manufacturer USB driver, revoking
  authorizations, other Android tools.
- Pictures: `assets/tutorial/1..6.png` (or `.jpg`), decoded with FFmpeg off the UI
  thread (no image crate). Missing pictures are simply not shown. `assets/` must be
  shipped next to the executable (phase 6).

## Main screen

Three big buttons under the preview: Enregistrer / Arrêter, Marquer un cut, Mes clips.

## Settings

- Quality presets instead of raw numbers: Légère (1280 px, 4 Mbit/s), Standard (1920 px,
  8 Mbit/s, default), Haute (native, 16 Mbit/s); 30 or 60 fps. Mapped to
  `api::VideoQuality`, sent with `Command::StartMirror`.
- Applied immediately by restarting the preview; during a recording, applied when it
  ends (the running file keeps one resolution/bit rate).
- Output folder and assistant are also reachable from here.
