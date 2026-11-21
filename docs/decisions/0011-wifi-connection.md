# 0011 — Connecting a phone over Wi-Fi

Date: 2026-11-20 — Status: accepted, done

## Choices (product owner)

- Android 11+ "Wireless debugging" only (no `adb tcpip` for older phones: it has to be
  redone after every reboot).
- Pairing with a **QR code** shown by PhonyCut, the 6-digit code as a fallback (new
  dependency `qrcode`, MIT/Apache, drawn by egui: no image feature).
- **Automatic reconnection** to paired phones.

## What adb does (checked with adb 37.0.1 and a Xiaomi, Android 16)

- `adb mdns check`: mDNS discovery is built in (`mdns_backend: LIBADBMDNS`).
- `adb pair <ip:port> <code>` → `Successfully paired to 192.168.1.45:38799
  [guid=adb-IR7XZT6DZPT44L99-0T2B7U]`. Wrong code → `Failed: Wrong password or connection
  was dropped.`
- Right after, without any `adb connect`, the device list has
  `adb-IR7XZT6DZPT44L99-0T2B7U._adb-tls-connect._tcp device …`: the server connects by
  itself to paired phones it sees on the network (`ADB_MDNS_AUTO_CONNECT`, default
  `adb-tls-connect`, from `adb --help`). It does so again on later starts.
- The connect port (39435) differs from the pairing port (38799); adb finds it.
- Over Wi-Fi the scrcpy server, the sound and the control work unchanged: 30–42 fps on a
  moving video at the Standard quality (8 Mbit/s), no decoding error. No special quality
  for Wi-Fi.

## Pairing flow (`pairing.rs`)

- QR code: `WIFI:T:ADB;S:<name>;P:<password>;;` (Android Studio's format), name
  `phonycut-<8 random>`, password 12 random letters/digits (OS-seeded). After the scan, the
  phone offers `_adb-tls-pairing._tcp` under `<name>`; we poll `adb mdns services` and run
  `adb pair`. Verified on the phone: found and paired in under a second, preview over
  Wi-Fi half a second later.
- Code: the user only types the 6 digits; the phone offers `_adb-tls-pairing._tcp` under
  its own name (`adb-<serial>-<random>`), whose address we use.
- QR code valid 3 minutes, code search 20 s; clear messages (wrong code, phone not found:
  same Wi-Fi? pairing window open?).

## Device list

- A phone plugged in and on Wi-Fi is listed once, by USB (faster, charges). Its Wi-Fi
  serial contains the hardware serial (`adb-<serial>-<random>._adb-tls-connect._tcp`).
  Unplugging hands over to Wi-Fi (the mirror restarts on the Wi-Fi serial).
- Messages for a phone not answering over Wi-Fi talk about the Wi-Fi, not the cable.

## "Switch to Wi-Fi" for every Android version (`adb tcpip`)

Added later at the owner's request (any Android version, like scrcpy's `--tcpip`).

- With the phone plugged in: read its Wi-Fi address (`ip route`, `src` of `dev wlan…`),
  `adb tcpip 5555` (adbd restarts: the USB preview drops for a moment and is restarted),
  then `adb connect <ip>:5555`. The phone listens until it restarts.
- `adb connect` exits with 0 in every case; its answer is read (adb 37.0.1, real phone):
  `connected to …` / `already connected to …`, `failed to authenticate to …` (the phone
  asks to allow the PC: it connects once accepted), `cannot connect to …: … (10060)`.
- Addresses that worked are remembered (`%APPDATA%\PhonyCut\wifi-phones.json`, 5 at most)
  and connected to again every 10 s while missing (Wi-Fi drop, PhonyCut restart). After a
  phone restart the cable is needed once; the home screen says so.
- Not encrypted, but a PC still has to be allowed on the phone ("Allow debugging").
- One entry per phone: an `ip:port` connection is matched to its phone with
  `ro.serialno`; USB first, then the first Wi-Fi connection (wireless debugging and tcpip
  can both be up).