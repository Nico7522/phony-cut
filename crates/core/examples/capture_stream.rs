//! Records the raw scrcpy sockets of a real phone, to build test fixtures.
//!
//! Usage (phone plugged in, authorized and unlocked):
//!   cargo run -p phonycut-core --example capture_stream -- <out.bin> [seconds] [serial] [--audio | --playback]
//!
//! `<out.bin>` contains everything after the dummy byte of the video socket:
//! device name (64 bytes), codec id, then session/media packets. With
//! `--audio`, the audio socket is also saved next to it as `<out>.audio.bin`
//! (codec id then media packets). `--playback` captures with the `playback`
//! audio source (Android 13+) instead of the whole device output. Put short captures (a few seconds, low
//! resolution) in `crates/core/tests/fixtures/`; `tests/fixtures_replay.rs`
//! replays every video `.bin` file found there.

use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use phonycut_core::adb::Adb;
use phonycut_core::api::DeviceState;
use phonycut_core::scrcpy::server::{AudioSource, RunningServer, ServerOptions};

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    tracing_subscriber::fmt().with_target(true).init(); // shows the server log
    let mut flag = |name: &str| match args.iter().position(|a| a == name) {
        Some(index) => {
            args.remove(index);
            true
        }
        None => false,
    };
    let playback = flag("--playback");
    let audio = flag("--audio") || playback;
    let audio_source = if playback {
        AudioSource::Playback
    } else {
        AudioSource::Output
    };
    let mut args = args.into_iter();
    let out_path = PathBuf::from(args.next().context("missing output file")?);
    let seconds: u64 = args.next().map_or(Ok(3), |s| s.parse())?;
    let serial_arg = args.next();

    let adb = Adb::locate()?;
    adb.start_server()?;
    let serial = match serial_arg {
        Some(serial) => serial,
        None => {
            let (devices, _) = adb.devices()?;
            let ready: Vec<_> = devices
                .into_iter()
                .filter(|d| d.state == DeviceState::Ready)
                .collect();
            match ready.as_slice() {
                [only] => only.serial.clone(),
                [] => bail!("no authorized device"),
                _ => bail!("several devices: pass the serial as third argument"),
            }
        }
    };

    // Small video keeps fixtures small.
    let options = ServerOptions {
        max_size: 480,
        video_bit_rate: 1_000_000,
        control: false, // the fixtures are the raw media sockets
        audio,
        audio_source,
        ..ServerOptions::default()
    };
    let mut server = RunningServer::start(&adb, &serial, &options)?;
    let connection = server.connect(&AtomicBool::new(false), audio, false)?;
    let deadline = Instant::now() + Duration::from_secs(seconds);

    let audio_thread = match connection.audio {
        Some(socket) => {
            let path = audio_path(&out_path);
            Some(thread::spawn(move || save(socket, &path, deadline)))
        }
        None => None,
    };
    let video_bytes = save(connection.video, &out_path, deadline)?;
    println!(
        "captured {video_bytes} video bytes from {serial} into {}",
        out_path.display()
    );
    if let Some(handle) = audio_thread {
        let audio_bytes = handle
            .join()
            .map_err(|_| anyhow::anyhow!("audio thread panicked"))??;
        println!(
            "captured {audio_bytes} audio bytes into {}",
            audio_path(&out_path).display()
        );
    }
    Ok(())
}

/// `clip.bin` → `clip.audio.bin`.
fn audio_path(video: &Path) -> PathBuf {
    let stem = video
        .file_stem()
        .map_or_else(Default::default, |s| s.to_os_string());
    let mut name = stem;
    name.push(".audio.bin");
    video.with_file_name(name)
}

/// Copies the socket into `path` until `deadline`; returns the byte count.
fn save(mut socket: TcpStream, path: &Path, deadline: Instant) -> anyhow::Result<usize> {
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    let mut file = File::create(path)?;
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0usize;
    while Instant::now() < deadline {
        match socket.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                file.write_all(&buf[..n])?;
                total += n;
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(total)
}
