//! Clip export and player checks on a test recording (3 s, 30 fps, one key
//! frame per second), written with our own MKV writer.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use ffmpeg_next as ffmpeg;

use phonecut_core::api::{Event, ExportMode, ExportRequest, ExportState, FrameSlot, PlayerEvent};
use phonecut_core::export::{clips_folder, export_fast, export_precise, run_export};
use phonecut_core::markers::{default_segments, Marker, Segment};
use phonecut_core::player::{Control, Player, PlayerSetup};

/// Facts about an exported file, read back with FFmpeg.
struct Clip {
    format: String,
    duration_ms: i64,
    frames: usize,
    first_is_key: bool,
}

fn inspect(path: &Path) -> Clip {
    let mut input = ffmpeg::format::input(path).unwrap();
    let format = input.format().name().to_owned();
    let duration_ms = input.duration() / 1000;
    let stream = input.streams().best(ffmpeg::media::Type::Video).unwrap();
    let index = stream.index();
    let mut decoder = ffmpeg::codec::Context::from_parameters(stream.parameters())
        .unwrap()
        .decoder()
        .video()
        .unwrap();
    let mut frame = ffmpeg::util::frame::Video::empty();
    let (mut frames, mut first_is_key) = (0, None);
    for (s, packet) in input.packets() {
        if s.index() != index {
            continue;
        }
        first_is_key.get_or_insert(packet.is_key());
        decoder.send_packet(&packet).unwrap();
        while decoder.receive_frame(&mut frame).is_ok() {
            frames += 1;
        }
    }
    decoder.send_eof().unwrap();
    while decoder.receive_frame(&mut frame).is_ok() {
        frames += 1;
    }
    Clip {
        format,
        duration_ms,
        frames,
        first_is_key: first_is_key.unwrap_or(false),
    }
}

fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phonecut-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn segment(start_ms: u64, end_ms: u64) -> Segment {
    Segment {
        name: "test".into(),
        start_ms,
        end_ms,
        export: true,
    }
}

#[test]
fn fast_export_starts_on_the_previous_key_frame() {
    let dir = workdir("fast");
    let video = dir.join("rec.mkv");
    common::write_test_recording(&video, 3);
    let out = dir.join("clip.mp4");

    export_fast(&video, &segment(1_500, 2_500), &out, &mut |_| true).unwrap();
    let clip = inspect(&out);
    assert!(clip.format.contains("mp4"), "{}", clip.format);
    assert!(clip.first_is_key);
    // Key frame at 1.0 s → clip covers 1.0..2.5 s = 45 frames.
    assert_eq!(clip.frames, 45);
    assert!(
        (clip.duration_ms - 1_500).abs() <= 40,
        "{} ms",
        clip.duration_ms
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn precise_export_starts_on_the_in_point() {
    let dir = workdir("precise");
    let video = dir.join("rec.mkv");
    common::write_test_recording(&video, 3);
    let out = dir.join("clip.mp4");

    let mut last = 0.0;
    export_precise(&video, &segment(1_500, 2_500), &out, &mut |f| {
        last = f;
        true
    })
    .unwrap();
    assert_eq!(last, 1.0, "progress reaches 100 %");
    let clip = inspect(&out);
    assert!(clip.format.contains("mp4"));
    assert!(clip.first_is_key);
    // Exactly 1.5..2.5 s = 30 frames (±1 for frame boundaries).
    assert!((29..=31).contains(&clip.frames), "{} frames", clip.frames);
    assert!(
        (clip.duration_ms - 1_000).abs() <= 50,
        "{} ms",
        clip.duration_ms
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn clip_ending_on_a_static_screen_keeps_its_duration() {
    // 3 s of video then 1 s without any frame (static screen): 4 s file.
    let dir = workdir("tail");
    let video = dir.join("rec.mkv");
    let duration_ms = common::write_test_recording_with_tail(&video, 3, 1_000);
    assert!(duration_ms.abs_diff(4_000) <= 1, "{duration_ms} ms");

    let fast = dir.join("fast.mp4");
    export_fast(&video, &segment(2_500, 4_000), &fast, &mut |_| true).unwrap();
    // From the key frame at 2.0 s to the end (4.0 s).
    let clip = inspect(&fast);
    assert!(
        (clip.duration_ms - 2_000).abs() <= 40,
        "fast: {} ms",
        clip.duration_ms
    );

    let precise = dir.join("precise.mp4");
    export_precise(&video, &segment(2_500, 4_000), &precise, &mut |_| true).unwrap();
    let clip = inspect(&precise);
    assert!(
        (clip.duration_ms - 1_500).abs() <= 40,
        "precise: {} ms",
        clip.duration_ms
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn three_clips_from_two_markers() {
    let dir = workdir("three");
    let video = dir.join("rec.mkv");
    let duration_ms = common::write_test_recording(&video, 3);
    let segments = default_segments(
        &[Marker { time_ms: 1_000 }, Marker { time_ms: 2_000 }],
        duration_ms,
    );
    assert_eq!(segments.len(), 3);

    for mode in [ExportMode::Fast, ExportMode::Precise] {
        let request = ExportRequest {
            video: video.clone(),
            segments: segments.clone(),
            mode,
        };
        let mut progress = Vec::new();
        let state = run_export(&request, &AtomicBool::new(false), &mut |s| progress.push(s));
        let ExportState::Finished { files, folder } = state else {
            panic!("{mode:?}: {state:?}");
        };
        assert_eq!(folder, clips_folder(&video));
        assert_eq!(files.len(), 3, "{mode:?}");
        for file in &files {
            let clip = inspect(file);
            assert!(
                clip.frames >= 29,
                "{mode:?} {}: {} frames",
                file.display(),
                clip.frames
            );
        }
        assert!(!progress.is_empty());
        fs::remove_dir_all(&folder).unwrap();
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn cancelled_export_leaves_no_file() {
    let dir = workdir("cancel");
    let video = dir.join("rec.mkv");
    common::write_test_recording(&video, 2);
    let request = ExportRequest {
        video: video.clone(),
        segments: vec![segment(0, 2_000)],
        mode: ExportMode::Precise,
    };
    let state = run_export(&request, &AtomicBool::new(true), &mut |_| {});
    assert_eq!(state, ExportState::Cancelled);
    let left = fs::read_dir(clips_folder(&video)).unwrap().count();
    assert_eq!(left, 0, "partial clip removed");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn player_loads_and_seeks_frame_accurately() {
    let dir = workdir("player");
    let video = dir.join("rec.mkv");
    common::write_test_recording(&video, 3);
    let (tx, rx) = crossbeam_channel::unbounded();
    let frames = Arc::new(FrameSlot::default());
    let player = Player::open(PlayerSetup {
        path: video.clone(),
        frames: Arc::clone(&frames),
        events: tx,
        notify: Arc::new(|| {}),
    })
    .unwrap();

    let next_player_event = || loop {
        match rx
            .recv_timeout(Duration::from_secs(5))
            .expect("player event")
        {
            Event::Player(e) => return e,
            _ => continue,
        }
    };
    let PlayerEvent::Loaded(loaded) = next_player_event() else {
        panic!("expected Loaded");
    };
    assert_eq!((loaded.width, loaded.height), (160, 96));
    assert!((loaded.duration.as_millis() as i64 - 3_000).abs() <= 40);
    assert!(matches!(
        next_player_event(),
        PlayerEvent::Position { time, playing: false } if time.is_zero()
    ));
    assert!(frames.take().is_some(), "first frame shown");

    // 2.0 s is not a key frame boundary + 0.5 s: needs decoding forward.
    player.send(Control::Seek(Duration::from_millis(2_500)));
    let PlayerEvent::Position { time, .. } = next_player_event() else {
        panic!("expected Position");
    };
    assert!((time.as_millis() as i64 - 2_500).abs() <= 34, "{time:?}");
    let frame = frames.take().expect("frame after seek");
    assert!((frame.pts_us.unwrap() as i64 - 2_500_000).abs() <= 34_000);

    // Playing to the end reports Ended.
    player.send(Control::Play);
    let ended = (0..100).any(|_| matches!(next_player_event(), PlayerEvent::Ended));
    assert!(ended);
    drop(player);
    fs::remove_dir_all(&dir).unwrap();
}
