//! Clip export and player checks on a test recording (3 s, 30 fps, one key
//! frame per second), written with our own MKV writer.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use ffmpeg_next as ffmpeg;

use phonycut_core::api::{
    Event, ExportFormat, ExportMode, ExportRequest, ExportState, ExportStyle, FrameSlot, GifSize,
    PlayerEvent,
};
use phonycut_core::export::{clips_folder, export_fast, export_precise, run_export};
use phonycut_core::markers::{default_segments, Marker, Segment};
use phonycut_core::player::{Control, Player, PlayerSetup};

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
    let dir = std::env::temp_dir().join(format!("phonycut-{name}-{}", std::process::id()));
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
    export_precise(
        &video,
        &segment(1_500, 2_500),
        &out,
        ExportStyle::default(),
        &mut |f| {
            last = f;
            true
        },
    )
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
    export_precise(
        &video,
        &segment(2_500, 4_000),
        &precise,
        ExportStyle::default(),
        &mut |_| true,
    )
    .unwrap();
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
            style: ExportStyle::default(),
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

/// Social network formats: the clip has the format's size whatever the
/// recording's, with a blurred background and a phone frame.
#[test]
fn social_formats_have_their_size() {
    let dir = workdir("formats");
    let video = dir.join("rec.mkv");
    common::write_test_recording(&video, 2);
    for (format, size) in [
        (ExportFormat::Vertical, (1080, 1920)),
        (ExportFormat::Square, (1080, 1080)),
    ] {
        let request = ExportRequest {
            video: video.clone(),
            segments: vec![segment(0, 2_000)],
            // A format is always re-encoded, even when "fast" is chosen.
            mode: ExportMode::Fast,
            style: ExportStyle {
                format,
                blur_background: true,
                phone_frame: true,
                ..ExportStyle::default()
            },
        };
        let state = run_export(&request, &AtomicBool::new(false), &mut |_| {});
        let ExportState::Finished { files, folder } = state else {
            panic!("{format:?}: {state:?}");
        };
        let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.contains(format.tag().unwrap()), "{name}");
        let actual = {
            let input = ffmpeg::format::input(&files[0]).unwrap();
            let stream = input.streams().best(ffmpeg::media::Type::Video).unwrap();
            let params = ffmpeg::codec::Context::from_parameters(stream.parameters())
                .unwrap()
                .decoder()
                .video()
                .unwrap();
            (params.width(), params.height())
        };
        assert_eq!(actual, size, "{format:?}");
        assert!(inspect(&files[0]).frames >= 50, "{format:?}");
        if let Ok(keep) = std::env::var("PHONYCUT_KEEP_EXPORT") {
            fs::copy(&files[0], std::path::Path::new(&keep).join(&name)).unwrap();
        }
        fs::remove_dir_all(&folder).unwrap();
    }
    fs::remove_dir_all(&dir).unwrap();
}

/// GIF: a looping animation at 15 frames per second, at most the chosen
/// width (never wider than the recording), without sound.
#[test]
fn gif_export_loops_at_15_fps() {
    let dir = workdir("gif");
    let video = dir.join("rec.mkv");
    common::write_recording_with_sound(&video);
    let request = ExportRequest {
        video: video.clone(),
        segments: vec![segment(1_000, 3_000)],
        mode: ExportMode::Fast,
        style: ExportStyle {
            format: ExportFormat::Gif,
            gif_size: GifSize::Small,
            ..ExportStyle::default()
        },
    };
    let state = run_export(&request, &AtomicBool::new(false), &mut |_| {});
    let ExportState::Finished { files, folder } = state else {
        panic!("{state:?}");
    };
    let gif = &files[0];
    assert_eq!(gif.extension().unwrap(), "gif");
    if let Ok(keep) = std::env::var("PHONYCUT_KEEP_EXPORT") {
        fs::copy(gif, std::path::Path::new(&keep).join("look.gif")).unwrap();
    }
    let (size, frames, has_audio) = {
        let mut input = ffmpeg::format::input(gif).unwrap();
        assert_eq!(input.format().name(), "gif");
        let has_audio = input.streams().best(ffmpeg::media::Type::Audio).is_some();
        let stream = input.streams().best(ffmpeg::media::Type::Video).unwrap();
        let index = stream.index();
        let mut decoder = ffmpeg::codec::Context::from_parameters(stream.parameters())
            .unwrap()
            .decoder()
            .video()
            .unwrap();
        let mut frame = ffmpeg::util::frame::Video::empty();
        let mut frames = 0;
        for (s, packet) in input.packets() {
            if s.index() == index {
                decoder.send_packet(&packet).unwrap();
                while decoder.receive_frame(&mut frame).is_ok() {
                    frames += 1;
                }
            }
        }
        ((decoder.width(), decoder.height()), frames, has_audio)
    };
    // The recording is 214x480: smaller than 320, so kept (even) at 214.
    assert_eq!(size.0, 214);
    assert!(!has_audio);
    // 2 s at 15 fps.
    assert!((28..=32).contains(&frames), "{frames} frames");
    fs::remove_dir_all(&folder).unwrap();
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
        style: ExportStyle::default(),
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

/// Sound in a clip: AAC copied, starting with the picture (fast: on the key
/// frame before the in point; precise: on the in point) and lasting as long.
fn check_sound(path: &Path, clip_ms: i64) {
    let first = sound_span(path, clip_ms).0;
    assert!(first <= 25, "sound starts with the clip, at {first} ms");
}

/// First and last sound packet times (ms); checks it decodes and lasts
/// until `clip_ms`.
fn sound_span(path: &Path, clip_ms: i64) -> (i64, i64) {
    let mut input = ffmpeg::format::input(path).unwrap();
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .expect("the clip has sound");
    let index = stream.index();
    let tb = stream.time_base();
    let mut decoder = ffmpeg::codec::Context::from_parameters(stream.parameters())
        .unwrap()
        .decoder()
        .audio()
        .unwrap();
    assert_eq!(decoder.id(), ffmpeg::codec::Id::AAC);
    let to_ms = |pts: i64| pts * 1000 * i64::from(tb.0) / i64::from(tb.1);
    let (mut first, mut last, mut samples) = (None, 0, 0);
    let mut frame = ffmpeg::util::frame::Audio::empty();
    for (s, packet) in input.packets() {
        if s.index() != index {
            continue;
        }
        let ms = to_ms(packet.pts().unwrap());
        first.get_or_insert(ms);
        last = ms;
        decoder.send_packet(&packet).unwrap();
        while decoder.receive_frame(&mut frame).is_ok() {
            samples += frame.samples();
        }
    }
    let first = first.unwrap();
    assert!(
        (last + 21 - clip_ms).abs() <= 40,
        "sound ends with the clip: last at {last} ms, clip {clip_ms} ms"
    );
    assert!(samples > 0, "sound decodes");
    (first, last)
}

#[test]
fn clips_keep_the_sound() {
    let dir = workdir("sound");
    let video = dir.join("rec.mkv");
    common::write_recording_with_sound(&video);

    let fast = dir.join("fast.mp4");
    export_fast(&video, &segment(2_500, 5_000), &fast, &mut |_| true).unwrap();
    check_sound(&fast, inspect(&fast).duration_ms);

    let precise = dir.join("precise.mp4");
    export_precise(
        &video,
        &segment(2_500, 5_000),
        &precise,
        ExportStyle::default(),
        &mut |_| true,
    )
    .unwrap();
    let clip = inspect(&precise);
    assert!(
        (clip.duration_ms - 2_500).abs() <= 40,
        "precise clip {} ms",
        clip.duration_ms
    );
    check_sound(&precise, clip.duration_ms);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rotation_fix_keeps_the_sound() {
    let dir = workdir("fix-sound");
    let video = dir.join("rec.mkv");
    let duration_ms = i64::try_from(common::write_recording_with_sound(&video)).unwrap();
    let before = sound_span(&video, duration_ms);
    phonycut_core::normalize::fix_rotation(&video, &mut |_| {}).unwrap();
    assert_eq!(
        sound_span(&video, duration_ms),
        before,
        "sound copied at the same times"
    );
    let _ = fs::remove_dir_all(&dir);
}
