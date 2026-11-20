//! Recording check: scrcpy streams (one recorded from a real phone, one
//! generated with a rotation and a device clock reset) are written to MKV with
//! our writer, then read back with FFmpeg: container, duration, frame count and
//! full decode must be right.

mod common;

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ffmpeg_next as ffmpeg;

use phonecut_core::record::{MkvWriter, Recorder, RecorderSetup, Timeline};
use phonecut_core::scrcpy::protocol::{self, ProtocolError, StreamItem};

/// What a recording should contain.
struct Expected {
    frames: usize,
    duration_us: i64,
    sizes: Vec<(u32, u32)>,
}

/// Writes every frame of a scrcpy stream (after the dummy byte), simulating
/// arrival times from the device clock, then a static screen of `tail`.
fn record(stream: Vec<u8>, path: &Path, tail: Duration) -> Expected {
    let mut r = Cursor::new(stream);
    protocol::read_device_name(&mut r).unwrap();
    protocol::read_codec(&mut r).unwrap();
    let first = protocol::read_first_session(&mut r).unwrap();

    let t0 = Instant::now();
    let mut clock = Duration::ZERO; // simulated arrival clock
    let mut last_pts: Option<u64> = None;
    let mut size = (first.width, first.height);
    let mut sizes = vec![size];
    let mut config = None;
    let mut inline_config: Option<Vec<u8>> = None;
    let mut writer: Option<(MkvWriter, Timeline)> = None;
    let mut frames = 0;

    loop {
        let packet = match protocol::read_item(&mut r) {
            Ok(StreamItem::Session(s)) => {
                size = (s.width, s.height);
                sizes.push(size);
                continue;
            }
            Ok(StreamItem::Packet(p)) => p,
            Err(ProtocolError::Closed) => break,
            Err(err) => panic!("{err}"),
        };
        if packet.config {
            if writer.is_some() {
                inline_config = Some(packet.data.clone());
            }
            config = Some(packet.data);
            continue;
        }
        let pts = packet.pts_us.unwrap();
        // Arrival follows the device clock, except across a clock reset
        // (rotation) where some real time passes.
        clock += match last_pts {
            Some(last) if pts >= last => Duration::from_micros(pts - last),
            Some(_) => Duration::from_millis(200),
            None => Duration::ZERO,
        };
        last_pts = Some(pts);
        let arrival = t0 + clock;

        if writer.is_none() {
            if !packet.key_frame {
                continue;
            }
            let w =
                MkvWriter::create(path, size.0, size.1, config.as_ref().unwrap(), None).unwrap();
            writer = Some((w, Timeline::new(pts, arrival)));
        }
        let (w, timeline) = writer.as_mut().unwrap();
        let mut data = inline_config.take().unwrap_or_default();
        data.extend_from_slice(&packet.data);
        w.write(data, timeline.map(pts, arrival), packet.key_frame)
            .unwrap();
        frames += 1;
    }

    let (w, timeline) = writer.expect("at least one key frame");
    let end = timeline.time_at(t0 + clock + tail);
    w.finish(end).unwrap();
    Expected {
        frames,
        duration_us: end,
        sizes,
    }
}

/// Reads the file back with FFmpeg and checks it against `expected`.
fn verify(path: &Path, expected: &Expected) {
    let mut input = ffmpeg::format::input(path).unwrap();
    assert!(input.format().name().contains("matroska"));

    // Container duration (AV_TIME_BASE = µs), stored with ms precision.
    let duration = input.duration();
    assert!(
        (duration - expected.duration_us).abs() <= 2_000,
        "duration {duration} µs, expected {} µs",
        expected.duration_us
    );

    let stream = input.streams().best(ffmpeg::media::Type::Video).unwrap();
    let index = stream.index();
    let params = stream.parameters();
    let mut decoder = ffmpeg::codec::Context::from_parameters(params)
        .unwrap()
        .decoder()
        .video()
        .unwrap();

    let mut packets = 0;
    let mut decoded = 0;
    let mut seen_sizes = Vec::new();
    let mut frame = ffmpeg::util::frame::Video::empty();
    let mut last_pts = i64::MIN;
    let mut receive = |decoder: &mut ffmpeg::decoder::Video, decoded: &mut usize| {
        while decoder.receive_frame(&mut frame).is_ok() {
            *decoded += 1;
            let size = (frame.width(), frame.height());
            if seen_sizes.last() != Some(&size) {
                seen_sizes.push(size);
            }
        }
    };
    for (s, packet) in input.packets() {
        if s.index() != index {
            continue;
        }
        let pts = packet.pts().unwrap();
        assert!(pts > last_pts, "timestamps strictly increase");
        last_pts = pts;
        packets += 1;
        decoder.send_packet(&packet).unwrap();
        receive(&mut decoder, &mut decoded);
    }
    decoder.send_eof().unwrap();
    receive(&mut decoder, &mut decoded);

    assert_eq!(packets, expected.frames, "packets in file");
    assert_eq!(decoded, expected.frames, "decoded frames");
    let mut expected_sizes = expected.sizes.clone();
    expected_sizes.dedup();
    assert_eq!(seen_sizes, expected_sizes, "video sizes (rotation)");
}

fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phonecut-rec-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn generated_stream_with_rotation_and_static_end() {
    let path = temp_file("rotation.mkv");
    let expected = record(common::rotating_stream(), &path, Duration::from_secs(2));
    // 2 x 12 frames; the static screen at the end counts in the duration.
    assert_eq!(expected.frames, 2 * common::FRAMES);
    assert!(expected.duration_us > 2_000_000);
    verify(&path, &expected);
    let _ = fs::remove_file(&path);
}

#[test]
fn real_phone_streams() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for fixture in entries.flatten().map(|e| e.path()) {
        // Video sockets only: `*.audio.bin` is the audio socket of a capture.
        if fixture.extension().is_none_or(|e| e != "bin")
            || fixture.to_string_lossy().ends_with(".audio.bin")
        {
            continue;
        }
        let name = fixture.file_stem().unwrap().to_string_lossy().into_owned();
        let path = temp_file(&format!("{name}.mkv"));
        // Captures may end mid-packet: read_item reports Closed, accepted.
        let expected = record(
            fs::read(&fixture).unwrap(),
            &path,
            Duration::from_millis(500),
        );
        verify(&path, &expected);
        let _ = fs::remove_file(&path);
    }
}

/// Packets of a capture socket, after its header (video: name + codec;
/// audio: codec), with their device PTS.
fn capture_packets(bytes: Vec<u8>, audio: bool) -> Vec<protocol::MediaPacket> {
    let mut r = Cursor::new(bytes);
    if audio {
        protocol::read_audio_codec(&mut r).unwrap();
    } else {
        protocol::read_device_name(&mut r).unwrap();
        protocol::read_codec(&mut r).unwrap();
    }
    let mut packets = Vec::new();
    loop {
        match protocol::read_item(&mut r) {
            Ok(StreamItem::Packet(p)) => packets.push(p),
            Ok(StreamItem::Session(_)) => {}
            Err(ProtocolError::Closed) => return packets,
            Err(err) => panic!("{err}"),
        }
    }
}

/// The real recorder, fed with a capture made with sound: the file has an AAC
/// track that starts with the picture, lasts as long and decodes.
#[test]
fn recording_with_sound() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let video = capture_packets(
        fs::read(dir.join("xiaomi-poco-android16-sound.bin")).unwrap(),
        false,
    );
    let audio = capture_packets(
        fs::read(dir.join("xiaomi-poco-android16-sound.audio.bin")).unwrap(),
        true,
    );
    let video_config = video.iter().find(|p| p.config).unwrap().data.clone();
    let audio_config = audio[0].data.clone();
    assert!(audio[0].config);

    let path = temp_file("with-sound.mkv");
    let (events, _rx) = crossbeam_channel::unbounded();
    let recorder = Recorder::start(RecorderSetup {
        path: path.clone(),
        width: 214,
        height: 480,
        config: Some(video_config),
        audio_config: Some(audio_config),
        events,
        notify: std::sync::Arc::new(|| {}),
    })
    .unwrap();
    // Deliver both sockets in device-time order, as they arrive live.
    let mut v = video.iter().filter(|p| !p.config).peekable();
    let mut a = audio.iter().skip(1).peekable();
    loop {
        match (v.peek(), a.peek()) {
            (Some(vp), Some(ap)) if ap.pts_us < vp.pts_us => recorder.push_audio(a.next().unwrap()),
            (Some(_), _) => recorder.push(v.next().unwrap()),
            (None, Some(_)) => recorder.push_audio(a.next().unwrap()),
            (None, None) => break,
        }
    }
    recorder.stop(phonecut_core::api::RecordingEnd::Requested);

    let first_key = video.iter().find(|p| p.key_frame).unwrap().pts_us.unwrap();
    let expected_audio: Vec<u64> = audio
        .iter()
        .filter_map(|p| p.pts_us)
        .filter(|&pts| pts >= first_key)
        .collect();

    let mut input = ffmpeg::format::input(&path).unwrap();
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .expect("sound track");
    let index = stream.index();
    let time_base = stream.time_base();
    let mut decoder = ffmpeg::codec::Context::from_parameters(stream.parameters())
        .unwrap()
        .decoder()
        .audio()
        .unwrap();
    assert_eq!(decoder.id(), ffmpeg::codec::Id::AAC);
    assert_eq!(decoder.rate(), 48_000);
    assert_eq!(decoder.channels(), 2);

    let mut pts_ms = Vec::new();
    let mut samples = 0;
    let mut frame = ffmpeg::util::frame::Audio::empty();
    for (s, packet) in input.packets() {
        if s.index() != index {
            continue;
        }
        let pts = packet.pts().unwrap();
        pts_ms.push(pts * 1000 * i64::from(time_base.0) / i64::from(time_base.1));
        decoder.send_packet(&packet).unwrap();
        while decoder.receive_frame(&mut frame).is_ok() {
            samples += frame.samples();
        }
    }
    assert_eq!(
        pts_ms.len(),
        expected_audio.len(),
        "every sound frame after the first image"
    );
    let first_offset_ms = i64::try_from((expected_audio[0] - first_key) / 1000).unwrap();
    assert!(
        (pts_ms[0] - first_offset_ms).abs() <= 1,
        "sound starts {} ms after the picture, expected {first_offset_ms} ms",
        pts_ms[0]
    );
    let span_ms =
        i64::try_from((expected_audio[expected_audio.len() - 1] - expected_audio[0]) / 1000)
            .unwrap();
    assert!(
        (pts_ms[pts_ms.len() - 1] - pts_ms[0] - span_ms).abs() <= 2,
        "sound keeps the device timing"
    );
    assert!(
        samples >= (pts_ms.len() - 2) * 1024,
        "sound decodes ({samples} samples)"
    );
    let _ = fs::remove_file(&path);
}
