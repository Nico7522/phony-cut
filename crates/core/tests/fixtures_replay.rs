//! Replays raw streams recorded from real phones (see
//! `examples/capture_stream.rs`) through the parser and the decoder.
//!
//! Every `tests/fixtures/*.bin` video file is checked; `*.audio.bin` files are
//! the audio socket recorded with it (`capture_stream --audio`). A capture may
//! end in the middle of a packet (it is cut after N seconds): that is accepted.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use phonycut_core::audio::AacDecoder;
use phonycut_core::decode::H264Decoder;
use phonycut_core::scrcpy::protocol::{self, AudioCodec, ProtocolError, StreamItem, VideoCodec};

/// AAC frame: 1024 samples at 48 kHz.
const AAC_FRAME_US: u64 = 21_333;

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new(); // no fixtures recorded yet
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .collect()
}

fn is_audio(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".audio.bin")
}

#[test]
fn recorded_streams_decode() {
    for path in fixtures().iter().filter(|p| !is_audio(p)) {
        check_capture(path);
    }
}

#[test]
fn recorded_audio_matches_its_video() {
    let audio: Vec<_> = fixtures().into_iter().filter(|p| is_audio(p)).collect();
    for path in audio {
        let name = path.display();
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut r = Cursor::new(bytes);
        assert_eq!(
            protocol::read_audio_codec(&mut r).unwrap(),
            Some(AudioCodec::Aac),
            "{name}"
        );
        // First packet: AudioSpecificConfig. 0x1190 = AAC-LC, 48 kHz, stereo.
        let config = protocol::read_audio_packet(&mut r).unwrap();
        assert!(config.config, "{name}: config first");
        assert_eq!(config.data, [0x11, 0x90], "{name}");

        let mut pts = Vec::new();
        loop {
            match protocol::read_audio_packet(&mut r) {
                Ok(p) => {
                    assert!(!p.config, "{name}: single config packet");
                    pts.push(p.pts_us.unwrap());
                }
                Err(ProtocolError::Closed) => break,
                Err(err) => panic!("{name}: {err}"),
            }
        }
        assert!(pts.len() > 10, "{name}: too few audio frames");
        for pair in pts.windows(2) {
            let step = pair[1] - pair[0];
            assert!(
                (AAC_FRAME_US..AAC_FRAME_US + 1_000).contains(&step),
                "{name}: unexpected audio step {step} µs"
            );
        }

        // Audio and video use the same device clock: the first PTS are close.
        let video_path = path.to_string_lossy().replace(".audio.bin", ".bin");
        let first_video = first_video_pts(Path::new(&video_path));
        assert!(
            pts[0].abs_diff(first_video) < 1_000_000,
            "{name}: audio and video clocks differ"
        );
    }
}

#[test]
fn recorded_audio_decodes() {
    let audio: Vec<_> = fixtures().into_iter().filter(|p| is_audio(p)).collect();
    for path in audio {
        let name = path.display();
        let mut r = Cursor::new(fs::read(&path).unwrap());
        protocol::read_audio_codec(&mut r).unwrap();
        let config = protocol::read_audio_packet(&mut r).unwrap();
        // Resample to a typical PC rate, mono, to check the conversion too.
        let mut decoder = AacDecoder::new(&config.data, 44_100, 1).unwrap();
        let mut samples = Vec::new();
        let mut frames = 0usize;
        while let Ok(packet) = protocol::read_audio_packet(&mut r) {
            decoder.decode(&packet.data, &mut samples).unwrap();
            frames += 1;
        }
        let expected = frames * 1024 * 44_100 / 48_000;
        assert!(
            samples.len().abs_diff(expected) < 3_000,
            "{name}: {} samples, expected about {expected}",
            samples.len()
        );
        assert!(
            samples.iter().all(|s| s.is_finite() && s.abs() <= 1.5),
            "{name}: samples out of range"
        );
        let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(
            peak > 0.01,
            "{name}: music was playing, got silence (peak {peak}, {} samples, {frames} frames)",
            samples.len()
        );
    }
}

fn first_video_pts(path: &Path) -> u64 {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut r = Cursor::new(bytes);
    protocol::read_device_name(&mut r).unwrap();
    protocol::read_codec(&mut r).unwrap();
    loop {
        match protocol::read_item(&mut r).unwrap() {
            StreamItem::Packet(p) if p.pts_us.is_some() => return p.pts_us.unwrap_or(0),
            _ => {}
        }
    }
}

fn check_capture(path: &Path) {
    let name = path.display();
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut r = Cursor::new(bytes);

    let device = protocol::read_device_name(&mut r).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(!device.is_empty(), "{name}: empty device name");
    assert_eq!(
        protocol::read_codec(&mut r).unwrap(),
        VideoCodec::H264,
        "{name}"
    );
    let session = protocol::read_first_session(&mut r).unwrap();

    let mut decoder = H264Decoder::new().unwrap();
    let mut size = (session.width, session.height);
    let mut frames = 0;
    let mut last_pts = None;
    loop {
        match protocol::read_item(&mut r) {
            Ok(StreamItem::Session(s)) => {
                decoder = H264Decoder::new().unwrap();
                size = (s.width, s.height);
            }
            Ok(StreamItem::Packet(p)) => {
                if let Some(pts) = p.pts_us {
                    if let Some(last) = last_pts {
                        assert!(pts >= last, "{name}: PTS went backwards");
                    }
                    last_pts = Some(pts);
                }
                if let Some(frame) = decoder.decode(&p).unwrap() {
                    assert_eq!((frame.width, frame.height), size, "{name}");
                    frames += 1;
                }
            }
            Err(ProtocolError::Closed) => break,
            Err(err) => panic!("{name}: {err}"),
        }
    }
    assert!(frames > 0, "{name}: no frame decoded");
}
