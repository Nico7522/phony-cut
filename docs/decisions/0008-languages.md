# 0008 — French and English interface

Date: 2026-11-18 — Status: accepted

## Choice

Hand-written translations in `crates/app/src/texts.rs`, no i18n crate (Fluent,
rust-i18n…): two languages, a few hundred strings, all already centralized.

- Each text is a function returning `tr(fr, en)`, or a `match language()` when it
  formats values (counts, sizes). Screens never contain literal text.
- The current language is a global (`AtomicU8`): set at startup from the settings and
  when the user changes it; the next frame is drawn in the new language, no restart.
- Formatting follows the language: `12,4 Mo` / `12.4 MB`, `28,0 s` / `28.0 s`.
- Core types carry no wording (unchanged rule), so the core needed no change except
  `platform::system_prefers_french()` (Windows `GetUserDefaultUILanguage`).

## Settings

`language: Auto | French | English` in `settings.json`, default `Auto`: French when the
Windows display language is French, English otherwise. Loading tolerates a UTF-8 BOM
(added by many Windows tools), which made the whole file unreadable before.

## Tutorial pictures

The screenshots show a French Xiaomi. In English the app first looks in
`assets/tutorial/en/` (same file names), then falls back to the French ones; the
English notice says the pictures show a phone set to French.

## Installer

English and French (`ShowLanguageDialog=auto`: picked from the Windows language).
