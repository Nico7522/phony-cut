//! H.264 → RGBA frames, with FFmpeg (LGPL shared build).
//!
//! Tuned for latency: single-threaded decoding (frame threading would delay
//! output by several frames) and `LOW_DELAY`.

use std::sync::Once;

use ffmpeg::codec::{self, Flags};
use ffmpeg::format::Pixel;
use ffmpeg::software::scaling;
use ffmpeg::util::frame;
use ffmpeg_next as ffmpeg;

use crate::api::VideoFrame;
use crate::scrcpy::protocol::MediaPacket;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("FFmpeg has no H.264 decoder")]
    NoDecoder,
    #[error("FFmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg::Error),
}

static FFMPEG_INIT: Once = Once::new();

fn init_ffmpeg() {
    FFMPEG_INIT.call_once(|| {
        if let Err(err) = ffmpeg::init() {
            tracing::error!(%err, "FFmpeg initialisation failed");
        }
        // FFmpeg logs to stderr by default; keep only real errors.
        ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Error);
    });
}

/// Key of the current scaler: it must be rebuilt when the input changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScalerKey {
    format: Pixel,
    width: u32,
    height: u32,
}

pub struct H264Decoder {
    decoder: ffmpeg::decoder::Video,
    scaler: Option<(ScalerKey, scaling::Context)>,
    /// SPS/PPS waiting to be prepended to the next frame packet (scrcpy sends
    /// them as a separate "config" packet).
    pending_config: Option<Vec<u8>>,
    decoded: frame::Video,
    converted: frame::Video,
}

impl H264Decoder {
    pub fn new() -> Result<Self, DecodeError> {
        init_ffmpeg();
        let codec = ffmpeg::decoder::find(codec::Id::H264).ok_or(DecodeError::NoDecoder)?;
        let mut context = codec::Context::new_with_codec(codec);
        context.set_flags(Flags::LOW_DELAY);
        let decoder = context.decoder().video()?;
        Ok(Self {
            decoder,
            scaler: None,
            pending_config: None,
            decoded: frame::Video::empty(),
            converted: frame::Video::empty(),
        })
    }

    /// Feeds one packet; returns the newest decoded frame, if one came out.
    pub fn decode(&mut self, packet: &MediaPacket) -> Result<Option<VideoFrame>, DecodeError> {
        if packet.config {
            self.pending_config = Some(packet.data.clone());
            return Ok(None);
        }

        let mut av_packet = match self.pending_config.take() {
            Some(mut merged) => {
                merged.extend_from_slice(&packet.data);
                ffmpeg::Packet::copy(&merged)
            }
            None => ffmpeg::Packet::copy(&packet.data),
        };
        let pts = packet.pts_us.and_then(|p| i64::try_from(p).ok());
        av_packet.set_pts(pts);
        av_packet.set_dts(pts);
        if packet.key_frame {
            av_packet.set_flags(codec::packet::Flags::KEY);
        }

        self.decoder.send_packet(&av_packet)?;

        let mut latest = None;
        loop {
            match self.decoder.receive_frame(&mut self.decoded) {
                Ok(()) => latest = Some(self.convert()?),
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN => {
                    break
                }
                Err(ffmpeg::Error::Eof) => break,
                Err(err) => return Err(err.into()),
            }
        }
        Ok(latest)
    }

    fn convert(&mut self) -> Result<VideoFrame, DecodeError> {
        let key = ScalerKey {
            format: self.decoded.format(),
            width: self.decoded.width(),
            height: self.decoded.height(),
        };
        let scaler = match &mut self.scaler {
            Some((current, scaler)) if *current == key => scaler,
            slot => {
                let scaler = scaling::Context::get(
                    key.format,
                    key.width,
                    key.height,
                    Pixel::RGBA,
                    key.width,
                    key.height,
                    scaling::Flags::BILINEAR,
                )?;
                &mut slot.insert((key, scaler)).1
            }
        };
        scaler.run(&self.decoded, &mut self.converted)?;

        let (width, height) = (key.width as usize, key.height as usize);
        let row_len = width * 4;
        let stride = self.converted.stride(0);
        let data = self.converted.data(0);
        let mut rgba = Vec::with_capacity(row_len * height);
        for row in data.chunks(stride).take(height) {
            rgba.extend_from_slice(&row[..row_len]);
        }

        Ok(VideoFrame {
            width: key.width,
            height: key.height,
            rgba,
            pts_us: self.decoded.pts().and_then(|p| u64::try_from(p).ok()),
        })
    }
}
