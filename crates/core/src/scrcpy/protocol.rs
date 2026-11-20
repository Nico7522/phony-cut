//! Wire format of the scrcpy **v5.0.1** video and audio sockets (forward tunnel).
//!
//! Audio socket (scrcpy `AudioEncoder`/`Streamer`): codec id `u32` (`0` = the
//! device could not capture sound and streams video only, `1` = config
//! error), then media packets with the same 12-byte header as below, never
//! session packets. The first packet is the codec config (AAC: csd-0).
//!
//! Verified against scrcpy v5.0.1 `doc/develop.md`, `Streamer.java`,
//! `DesktopConnection.java` and `app/src/demuxer.c`. The protocol is internal to
//! scrcpy and changes between versions: re-check everything when bumping
//! [`super::SCRCPY_VERSION`].
//!
//! Sequence on the (single) video socket:
//! 1. dummy byte `0` (forward tunnel only),
//! 2. device name: 64 bytes, UTF-8, NUL-padded,
//! 3. codec id: `u32` big-endian (`0` = stream disabled, `1` = config error),
//! 4. then a sequence of 12-byte headers:
//!    - MSB set → *session* packet: `[flags u32][width u32][height u32]`,
//!      sent at start and on each capture reset (rotation, fold…);
//!    - MSB clear → *media* packet: `[pts+flags u64][size u32]` then `size`
//!      bytes of payload. Bit 62 = config (SPS/PPS), bit 61 = key frame,
//!      bits 0..61 = PTS in microseconds.

use std::io::{self, Read};

pub const DEVICE_NAME_LEN: usize = 64;
pub const HEADER_LEN: usize = 12;

const FLAG_SESSION: u64 = 1 << 63;
const FLAG_CONFIG: u64 = 1 << 62;
const FLAG_KEY_FRAME: u64 = 1 << 61;
const PTS_MASK: u64 = FLAG_KEY_FRAME - 1;

/// Upper bound for one packet, to fail fast on a corrupted stream
/// (a 4K key frame is far below this).
pub const MAX_PACKET_SIZE: u32 = 64 * 1024 * 1024;

/// Video codec announced by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    Av1,
    Vp8,
    Vp9,
}

impl VideoCodec {
    pub fn from_id(id: u32) -> Option<Self> {
        match id {
            0x6832_3634 => Some(Self::H264),
            0x6832_3635 => Some(Self::H265),
            0x0061_7631 => Some(Self::Av1),
            0x0076_7038 => Some(Self::Vp8),
            0x0076_7039 => Some(Self::Vp9),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("connection closed")]
    Closed,
    #[error("i/o error: {0}")]
    Io(#[source] io::Error),
    #[error("the device disabled the video stream")]
    StreamDisabled,
    #[error("the device reported a configuration error")]
    ConfigError,
    #[error("unsupported codec id {0:#010x}")]
    UnsupportedCodec(u32),
    #[error("expected a session packet first")]
    MissingSession,
    #[error("unexpected session packet on the audio socket")]
    UnexpectedSession,
    #[error("invalid video size {width}x{height}")]
    InvalidSize { width: u32, height: u32 },
    #[error("invalid packet size {0}")]
    InvalidPacketSize(u32),
    #[error("unexpected dummy byte {0}")]
    BadDummyByte(u8),
}

impl From<io::Error> for ProtocolError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe => Self::Closed,
            _ => Self::Io(err),
        }
    }
}

/// A capture session: video size until the next session packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub width: u32,
    pub height: u32,
    /// Size changed because the client asked for it (flex displays only).
    pub client_resized: bool,
}

/// One encoded packet as produced by the device's `MediaCodec`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaPacket {
    /// Presentation time in microseconds; `None` for config packets.
    pub pts_us: Option<u64>,
    /// Codec configuration (SPS/PPS for H.264), not a frame.
    pub config: bool,
    pub key_frame: bool,
    pub data: Vec<u8>,
}

/// Item read from the video stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamItem {
    Session(Session),
    Packet(MediaPacket),
}

/// Reads the dummy byte sent first on a forward tunnel.
pub fn read_dummy_byte(r: &mut impl Read) -> Result<(), ProtocolError> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    match b[0] {
        0 => Ok(()),
        other => Err(ProtocolError::BadDummyByte(other)),
    }
}

/// Reads the 64-byte, NUL-padded device name.
pub fn read_device_name(r: &mut impl Read) -> Result<String, ProtocolError> {
    let mut buf = [0u8; DEVICE_NAME_LEN];
    r.read_exact(&mut buf)?;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(DEVICE_NAME_LEN);
    Ok(String::from_utf8_lossy(&buf[..end]).into_owned())
}

/// Reads the codec id that starts the video stream.
pub fn read_codec(r: &mut impl Read) -> Result<VideoCodec, ProtocolError> {
    let id = read_u32(r)?;
    match id {
        0 => Err(ProtocolError::StreamDisabled),
        1 => Err(ProtocolError::ConfigError),
        id => VideoCodec::from_id(id).ok_or(ProtocolError::UnsupportedCodec(id)),
    }
}

/// Audio codec announced on the audio socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Opus,
    Aac,
    Flac,
    Raw,
}

impl AudioCodec {
    pub fn from_id(id: u32) -> Option<Self> {
        match id {
            0x6f70_7573 => Some(Self::Opus),
            0x0061_6163 => Some(Self::Aac),
            0x666c_6163 => Some(Self::Flac),
            0x0072_6177 => Some(Self::Raw),
            _ => None,
        }
    }
}

/// Reads the codec id that starts the audio socket. `Ok(None)`: the device
/// could not capture sound (Android < 11, capture refused…) and keeps
/// streaming video only (scrcpy `writeDisableStream(false)`).
pub fn read_audio_codec(r: &mut impl Read) -> Result<Option<AudioCodec>, ProtocolError> {
    let id = read_u32(r)?;
    match id {
        0 => Ok(None),
        1 => Err(ProtocolError::ConfigError),
        id => AudioCodec::from_id(id)
            .map(Some)
            .ok_or(ProtocolError::UnsupportedCodec(id)),
    }
}

/// Reads the next audio packet. The audio socket carries the same 12-byte
/// media headers as video but never session packets.
pub fn read_audio_packet(r: &mut impl Read) -> Result<MediaPacket, ProtocolError> {
    match read_item(r)? {
        StreamItem::Packet(packet) => Ok(packet),
        StreamItem::Session(_) => Err(ProtocolError::UnexpectedSession),
    }
}

/// Reads the next session or media packet.
pub fn read_item(r: &mut impl Read) -> Result<StreamItem, ProtocolError> {
    let mut header = [0u8; HEADER_LEN];
    r.read_exact(&mut header)?;
    let head = u64::from_be_bytes(header_part::<8>(&header, 0));

    if head & FLAG_SESSION != 0 {
        let width = u32::from_be_bytes(header_part::<4>(&header, 4));
        let height = u32::from_be_bytes(header_part::<4>(&header, 8));
        if width == 0 || height == 0 {
            return Err(ProtocolError::InvalidSize { width, height });
        }
        return Ok(StreamItem::Session(Session {
            width,
            height,
            client_resized: header[3] & 1 != 0,
        }));
    }

    let size = u32::from_be_bytes(header_part::<4>(&header, 8));
    if size == 0 || size > MAX_PACKET_SIZE {
        return Err(ProtocolError::InvalidPacketSize(size));
    }
    let mut data = vec![0u8; size as usize];
    r.read_exact(&mut data)?;

    let config = head & FLAG_CONFIG != 0;
    Ok(StreamItem::Packet(MediaPacket {
        pts_us: (!config).then_some(head & PTS_MASK),
        config,
        key_frame: head & FLAG_KEY_FRAME != 0,
        data,
    }))
}

/// Reads the first item, which must be a session packet.
pub fn read_first_session(r: &mut impl Read) -> Result<Session, ProtocolError> {
    match read_item(r)? {
        StreamItem::Session(session) => Ok(session),
        StreamItem::Packet(_) => Err(ProtocolError::MissingSession),
    }
}

fn read_u32(r: &mut impl Read) -> Result<u32, ProtocolError> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}

fn header_part<const N: usize>(header: &[u8; HEADER_LEN], offset: usize) -> [u8; N] {
    let mut out = [0u8; N];
    out.copy_from_slice(&header[offset..offset + N]);
    out
}

/// Encoders mirroring the server side, to build test streams and fixtures.
pub mod encode {
    use super::*;

    pub fn session(width: u32, height: u32, client_resized: bool) -> Vec<u8> {
        let mut flags = (FLAG_SESSION >> 32) as u32;
        if client_resized {
            flags |= 1;
        }
        let mut out = flags.to_be_bytes().to_vec();
        out.extend(width.to_be_bytes());
        out.extend(height.to_be_bytes());
        out
    }

    pub fn packet(pts_us: u64, config: bool, key_frame: bool, data: &[u8]) -> Vec<u8> {
        let mut head = if config { FLAG_CONFIG } else { pts_us };
        if key_frame && !config {
            head |= FLAG_KEY_FRAME;
        }
        let mut out = head.to_be_bytes().to_vec();
        out.extend((data.len() as u32).to_be_bytes());
        out.extend(data);
        out
    }

    pub fn device_name(name: &str) -> Vec<u8> {
        let mut out = vec![0u8; DEVICE_NAME_LEN];
        out[..name.len()].copy_from_slice(name.as_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::encode::*;
    use super::*;
    use std::io::Cursor;

    #[test]
    fn full_handshake_and_stream() {
        let mut bytes = vec![0u8]; // dummy byte
        bytes.extend(device_name("Pixel 7"));
        bytes.extend(0x6832_3634u32.to_be_bytes()); // "h264"
        bytes.extend(session(1080, 2400, false));
        bytes.extend(packet(0, true, false, &[0, 0, 0, 1, 0x67]));
        bytes.extend(packet(16_666, false, true, &[0, 0, 0, 1, 0x65, 1, 2]));
        bytes.extend(packet(33_333, false, false, &[0, 0, 0, 1, 0x41]));
        bytes.extend(session(2400, 1080, false)); // rotation
        let mut r = Cursor::new(bytes);

        read_dummy_byte(&mut r).unwrap();
        assert_eq!(read_device_name(&mut r).unwrap(), "Pixel 7");
        assert_eq!(read_codec(&mut r).unwrap(), VideoCodec::H264);
        assert_eq!(
            read_first_session(&mut r).unwrap(),
            Session {
                width: 1080,
                height: 2400,
                client_resized: false
            }
        );

        let StreamItem::Packet(config) = read_item(&mut r).unwrap() else {
            panic!("expected config packet");
        };
        assert!(config.config);
        assert_eq!(config.pts_us, None);
        assert!(!config.key_frame);

        let StreamItem::Packet(key) = read_item(&mut r).unwrap() else {
            panic!("expected key frame");
        };
        assert_eq!(key.pts_us, Some(16_666));
        assert!(key.key_frame);
        assert_eq!(key.data, vec![0, 0, 0, 1, 0x65, 1, 2]);

        let StreamItem::Packet(delta) = read_item(&mut r).unwrap() else {
            panic!("expected delta frame");
        };
        assert_eq!(delta.pts_us, Some(33_333));
        assert!(!delta.key_frame);

        assert_eq!(
            read_item(&mut r).unwrap(),
            StreamItem::Session(Session {
                width: 2400,
                height: 1080,
                client_resized: false
            })
        );
        assert!(matches!(read_item(&mut r), Err(ProtocolError::Closed)));
    }

    #[test]
    fn large_pts_keeps_flags_apart() {
        let pts = (1u64 << 60) + 12_345;
        let mut r = Cursor::new(packet(pts, false, true, &[9]));
        let StreamItem::Packet(p) = read_item(&mut r).unwrap() else {
            panic!("expected packet");
        };
        assert_eq!(p.pts_us, Some(pts));
        assert!(p.key_frame);
        assert!(!p.config);
    }

    #[test]
    fn client_resized_flag() {
        let mut r = Cursor::new(session(800, 600, true));
        assert!(read_first_session(&mut r).unwrap().client_resized);
    }

    #[test]
    fn device_name_uses_all_64_bytes_when_unterminated() {
        let mut r = Cursor::new(vec![b'a'; DEVICE_NAME_LEN]);
        assert_eq!(read_device_name(&mut r).unwrap().len(), DEVICE_NAME_LEN);
    }

    #[test]
    fn codec_special_values() {
        let read = |id: u32| read_codec(&mut Cursor::new(id.to_be_bytes()));
        assert!(matches!(read(0), Err(ProtocolError::StreamDisabled)));
        assert!(matches!(read(1), Err(ProtocolError::ConfigError)));
        assert!(matches!(
            read(0x6f70_7573), // "opus" is not a video codec
            Err(ProtocolError::UnsupportedCodec(0x6f70_7573))
        ));
        assert_eq!(read(0x6832_3635).unwrap(), VideoCodec::H265);
    }

    #[test]
    fn audio_codec_ids() {
        let read = |id: u32| read_audio_codec(&mut Cursor::new(id.to_be_bytes()));
        assert_eq!(read(0x0061_6163).unwrap(), Some(AudioCodec::Aac));
        assert_eq!(read(0x6f70_7573).unwrap(), Some(AudioCodec::Opus));
        assert_eq!(read(0).unwrap(), None, "device could not capture sound");
        assert!(matches!(read(1), Err(ProtocolError::ConfigError)));
        assert!(matches!(
            read(0x6832_3634), // "h264" is not an audio codec
            Err(ProtocolError::UnsupportedCodec(_))
        ));
    }

    #[test]
    fn audio_packets_have_no_sessions() {
        let mut stream = packet(0, true, false, &[0x11, 0x90]);
        stream.extend(packet(21_333, false, true, &[1, 2, 3]));
        stream.extend(session(10, 10, false));
        let mut r = Cursor::new(stream);
        let config = read_audio_packet(&mut r).unwrap();
        assert!(config.config);
        assert_eq!(config.pts_us, None);
        assert_eq!(config.data, [0x11, 0x90]);
        let frame = read_audio_packet(&mut r).unwrap();
        assert_eq!(frame.pts_us, Some(21_333));
        assert!(matches!(
            read_audio_packet(&mut r),
            Err(ProtocolError::UnexpectedSession)
        ));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(
            read_dummy_byte(&mut Cursor::new([7u8])),
            Err(ProtocolError::BadDummyByte(7))
        ));
        assert!(matches!(
            read_first_session(&mut Cursor::new(packet(1, false, false, &[1]))),
            Err(ProtocolError::MissingSession)
        ));
        assert!(matches!(
            read_item(&mut Cursor::new(session(0, 100, false))),
            Err(ProtocolError::InvalidSize { .. })
        ));
        let mut zero = 0u64.to_be_bytes().to_vec();
        zero.extend(0u32.to_be_bytes());
        assert!(matches!(
            read_item(&mut Cursor::new(zero)),
            Err(ProtocolError::InvalidPacketSize(0))
        ));
    }

    #[test]
    fn truncated_packet_is_closed() {
        let mut bytes = packet(5, false, false, &[1, 2, 3, 4]);
        bytes.truncate(bytes.len() - 2);
        assert!(matches!(
            read_item(&mut Cursor::new(bytes)),
            Err(ProtocolError::Closed)
        ));
    }
}
