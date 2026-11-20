//! Shared helpers for integration tests: real H.264 streams generated with
//! FFmpeg's libopenh264 encoder, framed like the scrcpy v5.0.1 server does.
#![allow(dead_code)] // each test binary uses a subset

use ffmpeg::format::Pixel;
use ffmpeg::util::frame;
use ffmpeg_next as ffmpeg;

use phonycut_core::scrcpy::protocol::encode;

/// Frames per generated capture session.
pub const FRAMES: usize = 12;
/// Device time between generated frames (30 fps).
pub const FRAME_US: u64 = 33_333;

/// Encodes `FRAMES` frames of a moving gradient; returns Annex-B packets
/// with their key-frame flag.
pub fn encode_h264(width: u32, height: u32) -> Vec<(Vec<u8>, bool)> {
    encode_h264_with(width, height, FRAMES, 30)
}

/// Writes a test recording with our own MKV writer: `seconds` of 30 fps
/// video with a key frame every second (like the real stream). Returns the
/// duration in ms.
pub fn write_test_recording(path: &std::path::Path, seconds: usize) -> u64 {
    write_test_recording_with_tail(path, seconds, 0)
}

/// Like [`write_test_recording`], followed by `tail_ms` of static screen
/// (no frame, like the real server when nothing changes).
pub fn write_test_recording_with_tail(path: &std::path::Path, seconds: usize, tail_ms: i64) -> u64 {
    use phonycut_core::record::MkvWriter;
    let packets = encode_h264_with(160, 96, seconds * 30, 30);
    let (config, first) = split_config(&packets[0].0);
    let mut writer = MkvWriter::create(path, 160, 96, &config, None).expect("create mkv");
    for (i, (au, key)) in packets.iter().enumerate() {
        let data = if i == 0 { first.clone() } else { au.clone() };
        let time_us = i as i64 * FRAME_US as i64;
        writer.write(data, time_us, *key).expect("write");
    }
    let end_us = (packets.len() as i64) * FRAME_US as i64 + tail_ms * 1000;
    writer.finish(end_us).expect("finish");
    (end_us / 1000) as u64
}

/// Like [`encode_h264`] with a frame count and key frame interval.
pub fn encode_h264_with(width: u32, height: u32, frames: usize, gop: u32) -> Vec<(Vec<u8>, bool)> {
    ffmpeg::init().expect("ffmpeg init");
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Error);
    let codec = ffmpeg::encoder::find_by_name("libopenh264").expect("libopenh264 encoder");
    let mut ctx = ffmpeg::codec::Context::new_with_codec(codec)
        .encoder()
        .video()
        .expect("video encoder");
    ctx.set_width(width);
    ctx.set_height(height);
    ctx.set_format(Pixel::YUV420P);
    ctx.set_time_base((1, 30));
    ctx.set_bit_rate(400_000);
    ctx.set_gop(gop);
    let mut encoder = ctx.open_as(codec).expect("open encoder");

    let mut packets = Vec::new();
    let mut packet = ffmpeg::Packet::empty();
    let mut drain = |encoder: &mut ffmpeg::encoder::Video, packets: &mut Vec<(Vec<u8>, bool)>| {
        while encoder.receive_packet(&mut packet).is_ok() {
            let data = packet.data().unwrap_or_default().to_vec();
            packets.push((data, packet.is_key()));
        }
    };

    for i in 0..frames {
        let mut picture = frame::Video::new(Pixel::YUV420P, width, height);
        for plane in 0..3 {
            let stride = picture.stride(plane);
            let data = picture.data_mut(plane);
            for (y, row) in data.chunks_mut(stride).enumerate() {
                for (x, px) in row.iter_mut().enumerate() {
                    *px = ((x + y + i * 4) % 256) as u8;
                }
            }
        }
        picture.set_pts(Some(i as i64));
        encoder.send_frame(&picture).expect("send frame");
        drain(&mut encoder, &mut packets);
    }
    encoder.send_eof().expect("eof");
    drain(&mut encoder, &mut packets);
    packets
}

/// Splits an Annex-B access unit into (SPS/PPS, rest), like MediaCodec which
/// emits the codec config as its own buffer.
pub fn split_config(au: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= au.len() {
        if au[i..].starts_with(&[0, 0, 1]) {
            let start = if i > 0 && au[i - 1] == 0 { i - 1 } else { i };
            starts.push((start, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let (mut config, mut rest) = (Vec::new(), Vec::new());
    for (n, &(start, payload)) in starts.iter().enumerate() {
        let end = starts.get(n + 1).map_or(au.len(), |&(next, _)| next);
        let nal_type = au[payload] & 0x1f;
        let target = if nal_type == 7 || nal_type == 8 {
            &mut config
        } else {
            &mut rest
        };
        target.extend_from_slice(&au[start..end]);
    }
    (config, rest)
}

/// Appends a capture session framed the way the scrcpy server sends it.
/// Device PTS restart at 0 for each session, like an encoder reset.
pub fn scrcpy_session(out: &mut Vec<u8>, width: u32, height: u32) {
    out.extend(encode::session(width, height, false));
    for (i, (au, key)) in encode_h264(width, height).into_iter().enumerate() {
        let pts = i as u64 * FRAME_US;
        if i == 0 {
            let (config, frame) = split_config(&au);
            assert!(!config.is_empty(), "first access unit must carry SPS/PPS");
            out.extend(encode::packet(0, true, false, &config));
            out.extend(encode::packet(pts, false, key, &frame));
        } else {
            out.extend(encode::packet(pts, false, key, &au));
        }
    }
}

/// Full socket content (after the dummy byte) with two sessions: portrait,
/// then rotated to landscape.
pub fn rotating_stream() -> Vec<u8> {
    let mut bytes = encode::device_name("Test phone");
    bytes.extend(0x6832_3634u32.to_be_bytes());
    scrcpy_session(&mut bytes, 64, 128);
    scrcpy_session(&mut bytes, 128, 64);
    bytes
}

/// Packets of a recorded scrcpy socket (`capture_stream`), after its header.
pub fn capture_packets(
    bytes: Vec<u8>,
    audio: bool,
) -> Vec<phonycut_core::scrcpy::protocol::MediaPacket> {
    use phonycut_core::scrcpy::protocol::{self, ProtocolError, StreamItem};
    let mut r = std::io::Cursor::new(bytes);
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

/// Writes a recording with sound from the real capture
/// `xiaomi-poco-android16-sound` (about 7.7 s, 214x480, AAC). File time 0 is
/// its first key frame. Returns the duration in ms.
pub fn write_recording_with_sound(path: &std::path::Path) -> u64 {
    use phonycut_core::record::MkvWriter;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let video = capture_packets(
        std::fs::read(dir.join("xiaomi-poco-android16-sound.bin")).unwrap(),
        false,
    );
    let audio = capture_packets(
        std::fs::read(dir.join("xiaomi-poco-android16-sound.audio.bin")).unwrap(),
        true,
    );
    let config = video.iter().find(|p| p.config).unwrap().data.clone();
    let first_key = video.iter().find(|p| p.key_frame).unwrap().pts_us.unwrap();
    let mut writer = MkvWriter::create(path, 214, 480, &config, Some(&audio[0].data)).unwrap();
    let time = |pts: u64| pts as i64 - first_key as i64;
    let mut frames = video
        .iter()
        .filter(|p| !p.config && p.pts_us.unwrap() >= first_key)
        .peekable();
    let mut sound = audio
        .iter()
        .skip(1)
        .filter(|p| p.pts_us.unwrap() >= first_key)
        .peekable();
    let mut end = 0;
    loop {
        match (frames.peek(), sound.peek()) {
            (Some(v), Some(a)) if a.pts_us < v.pts_us => {
                let a = sound.next().unwrap();
                writer
                    .write_audio(&a.data, time(a.pts_us.unwrap()))
                    .unwrap();
            }
            (Some(_), _) => {
                let v = frames.next().unwrap();
                end = time(v.pts_us.unwrap());
                writer.write(v.data.clone(), end, v.key_frame).unwrap();
            }
            (None, Some(_)) => {
                let a = sound.next().unwrap();
                writer
                    .write_audio(&a.data, time(a.pts_us.unwrap()))
                    .unwrap();
            }
            (None, None) => break,
        }
    }
    let end_us = end + 33_333;
    writer.finish(end_us).unwrap();
    (end_us / 1000) as u64
}
