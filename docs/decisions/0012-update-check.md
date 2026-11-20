# 0012 — Telling the user a new version is out

Date: 2026-11-20 — Status: accepted, done

## Choices (product owner)

- A **discreet banner** ("PhonyCut 0.4.0 is available. Download · Later") linking to the
  website's download page; the user installs it as before. Never a blocking pop-up, no
  automatic download.
- The request goes through the **system `curl`** (shipped with Windows 10/11 and macOS):
  no HTTP/TLS library in PhonyCut, same principle as adb.
- **At most once a day**, at launch, with a setting (Settings → Updates, on by default).

## How

- `GET https://api.github.com/repos/Nico7522/phony-cut/releases/latest` with a generic user
  agent (`PhonyCut-update-check`); only `tag_name` is read. Nothing about the user, the
  phone or the videos is sent; the website FAQ says so.
- Versions compare as numbers (`0.10.0` > `0.9.9`). Offline or GitHub down: nothing is
  shown, the log says why.
- `PHONYCUT_PRETEND_VERSION=0.1.0` (development) makes PhonyCut compare as an older
  version and ignore the daily limit, to see the banner.
