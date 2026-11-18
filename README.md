# PhoneCut

Show your Android phone's screen on your Windows PC, record it, mark cuts while you
record, then export the clips. You can also use the phone with the mouse and keyboard.
In French and English.

**Website, download and tutorial: https://nico7522.github.io/phony-cut/**

- Windows 10/11 (x64), an Android phone (5.0 or later) with USB debugging enabled and a
  USB cable that transfers data.
- [Download the latest version](https://github.com/Nico7522/phony-cut/releases/latest/download/PhoneCut-Setup-x64.exe)
  (installs for the current user, no administrator rights needed).

## Building from source

See [CLAUDE.md](CLAUDE.md) (in French): tools, commands and project structure.

## License

PhoneCut is licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

at your option.

PhoneCut uses the Android server of [scrcpy](https://github.com/Genymobile/scrcpy)
(Apache 2.0), FFmpeg (LGPL, dynamically linked) and Android platform-tools (adb). Their
versions and licenses are listed in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md),
and the license texts are in [vendor/licenses](vendor/licenses). PhoneCut is not
affiliated with Genymobile or Google.
