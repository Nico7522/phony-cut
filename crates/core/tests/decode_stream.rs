//! End-to-end check of the video path without a phone: a real H.264 stream is
//! produced with FFmpeg's libopenh264 encoder, framed exactly like the scrcpy
//! v5.0.1 server does (separate config packet, 12-byte headers, session packet
//! on rotation), then parsed and decoded by our code.

use std::io::Cursor;

use ffmpeg::format::Pixel;
use ffmpeg::util::frame;
use ffmpeg_next as ffmpeg;

use phonecut_core::decode::H264Decoder;
use phonecut_core::scrcpy::protocol::{self, encode, StreamItem};

const FRAMES: usize = 12;

/// Encodes `FRAMES` frames of a moving gradient; returns Annex-B packets
/// with their key-frame flag.
fn encode_h264(width: u32, height: u32) -> Vec<(Vec<u8>, bool)> {
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
    ctx.set_gop(30);
    let mut encoder = ctx.open_as(codec).expect("open encoder");

    let mut packets = Vec::new();
    let mut packet = ffmpeg::Packet::empty();
    let mut drain = |encoder: &mut ffmpeg::encoder::Video, packets: &mut Vec<(Vec<u8>, bool)>| {
        while encoder.receive_packet(&mut packet).is_ok() {
            let data = packet.data().unwrap_or_default().to_vec();
            packets.push((data, packet.is_key()));
        }
    };

    for i in 0..FRAMES {
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
fn split_config(au: &[u8]) -> (Vec<u8>, Vec<u8>) {
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

/// Frames a capture session the way the scrcpy server sends it.
fn scrcpy_session(out: &mut Vec<u8>, width: u32, height: u32) {
    out.extend(encode::session(width, height, false));
    for (i, (au, key)) in encode_h264(width, height).into_iter().enumerate() {
        let pts = i as u64 * 33_333;
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

#[test]
fn decodes_scrcpy_framed_stream_with_rotation() {
    let mut bytes = vec![0u8];
    bytes.extend(encode::device_name("Test phone"));
    bytes.extend(0x6832_3634u32.to_be_bytes());
    scrcpy_session(&mut bytes, 64, 128); // portrait
    scrcpy_session(&mut bytes, 128, 64); // rotated: new session packet
    let mut r = Cursor::new(bytes);

    protocol::read_dummy_byte(&mut r).unwrap();
    assert_eq!(protocol::read_device_name(&mut r).unwrap(), "Test phone");
    protocol::read_codec(&mut r).unwrap();
    let first = protocol::read_first_session(&mut r).unwrap();
    assert_eq!((first.width, first.height), (64, 128));

    let mut decoder = H264Decoder::new().unwrap();
    let mut size = (first.width, first.height);
    let mut decoded = Vec::new();
    loop {
        match protocol::read_item(&mut r) {
            Ok(StreamItem::Session(s)) => {
                decoder = H264Decoder::new().unwrap();
                size = (s.width, s.height);
            }
            Ok(StreamItem::Packet(p)) => {
                if let Some(frame) = decoder.decode(&p).unwrap() {
                    assert_eq!((frame.width, frame.height), size);
                    assert_eq!(frame.rgba.len(), (size.0 * size.1 * 4) as usize);
                    decoded.push((frame.width, frame.pts_us));
                }
            }
            Err(protocol::ProtocolError::Closed) => break,
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    // Low-delay decoding: (almost) every packet yields its frame immediately.
    let portrait = decoded.iter().filter(|(w, _)| *w == 64).count();
    let landscape = decoded.iter().filter(|(w, _)| *w == 128).count();
    assert!(portrait >= FRAMES - 1, "portrait frames: {portrait}");
    assert!(landscape >= FRAMES - 1, "landscape frames: {landscape}");
    assert_eq!(decoded[0].1, Some(0), "PTS is propagated");
}
