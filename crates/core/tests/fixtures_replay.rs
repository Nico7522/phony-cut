//! Replays raw streams recorded from real phones (see
//! `examples/capture_stream.rs`) through the parser and the decoder.
//!
//! Every `tests/fixtures/*.bin` file is checked. A capture may end in the
//! middle of a packet (it is cut after N seconds): that is accepted.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use phonecut_core::decode::H264Decoder;
use phonecut_core::scrcpy::protocol::{self, ProtocolError, StreamItem, VideoCodec};

#[test]
fn recorded_streams_decode() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let Ok(entries) = fs::read_dir(&dir) else {
        return; // no fixtures recorded yet
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_some_and(|e| e == "bin") {
            check_capture(&path);
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
