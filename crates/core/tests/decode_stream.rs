//! End-to-end check of the video path without a phone: a real H.264 stream is
//! produced with FFmpeg's libopenh264 encoder, framed exactly like the scrcpy
//! v5.0.1 server does (separate config packet, 12-byte headers, session packet
//! on rotation), then parsed and decoded by our code.

mod common;

use std::io::Cursor;

use common::{rotating_stream, FRAMES};
use phonecut_core::decode::H264Decoder;
use phonecut_core::scrcpy::protocol::{self, StreamItem};

#[test]
fn decodes_scrcpy_framed_stream_with_rotation() {
    let mut r = Cursor::new(rotating_stream());

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
