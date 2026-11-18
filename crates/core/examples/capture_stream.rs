//! Records the raw scrcpy video socket of a real phone, to build test fixtures.
//!
//! Usage (phone plugged in and authorized):
//!   cargo run -p phonecut-core --example capture_stream -- <out.bin> [seconds] [serial]
//!
//! The file contains everything after the dummy byte: device name (64 bytes),
//! codec id, then session/media packets. Put short captures (a few seconds, low
//! resolution) in `crates/core/tests/fixtures/`; `tests/fixtures_replay.rs`
//! replays every `.bin` file found there.

use std::fs::File;
use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use phonecut_core::adb::Adb;
use phonecut_core::api::DeviceState;
use phonecut_core::scrcpy::server::{RunningServer, ServerOptions};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out_path = args.next().context("missing output file")?;
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
        control: false, // video only: the fixture is the raw video socket
        ..ServerOptions::default()
    };
    let mut server = RunningServer::start(&adb, &serial, &options)?;
    let mut socket = server.connect(&AtomicBool::new(false), false)?.video;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;

    let mut file = File::create(&out_path)?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
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
    println!("captured {total} bytes from {serial} into {out_path}");
    Ok(())
}
