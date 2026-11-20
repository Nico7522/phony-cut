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

pub(crate) fn init_ffmpeg() {
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
    out_width: u32,
    out_height: u32,
}

/// Output size fitting `width`×`height` with its long side at most
/// `max_side`, keeping the aspect ratio (even sizes, never upscaled).
pub fn fit_size(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= max_side || long == 0 {
        return (width, height);
    }
    let scale =
        |v: u32| ((u64::from(v) * u64::from(max_side) / u64::from(long)) as u32 & !1).max(2);
    (scale(width), scale(height))
}

/// Converts decoded frames (any pixel format and size) to packed RGBA,
/// rebuilding its scaler when the input changes (rotation).
#[derive(Default)]
pub struct RgbaConverter {
    scaler: Option<(ScalerKey, scaling::Context)>,
    converted: Option<frame::Video>,
}

impl RgbaConverter {
    /// `pts_us`: presentation time to attach to the result.
    pub fn convert(
        &mut self,
        decoded: &frame::Video,
        pts_us: Option<u64>,
    ) -> Result<VideoFrame, DecodeError> {
        self.convert_fit(decoded, pts_us, u32::MAX)
    }

    /// Like [`Self::convert`], scaled down so the long side is at most
    /// `max_side` (a preview never needs more pixels than the screen).
    pub fn convert_fit(
        &mut self,
        decoded: &frame::Video,
        pts_us: Option<u64>,
        max_side: u32,
    ) -> Result<VideoFrame, DecodeError> {
        let (out_width, out_height) = fit_size(decoded.width(), decoded.height(), max_side);
        let key = ScalerKey {
            format: decoded.format(),
            width: decoded.width(),
            height: decoded.height(),
            out_width,
            out_height,
        };
        let scaler = match &mut self.scaler {
            Some((current, scaler)) if *current == key => scaler,
            slot => {
                let scaler = scaling::Context::get(
                    key.format,
                    key.width,
                    key.height,
                    Pixel::RGBA,
                    key.out_width,
                    key.out_height,
                    scaling::Flags::BILINEAR,
                )?;
                // A new size needs a new output picture.
                self.converted = None;
                &mut slot.insert((key, scaler)).1
            }
        };
        let converted = self.converted.get_or_insert_with(frame::Video::empty);
        scaler.run(decoded, converted)?;

        let (width, height) = (key.out_width as usize, key.out_height as usize);
        let row_len = width * 4;
        let stride = converted.stride(0);
        let data = converted.data(0);
        let mut rgba = Vec::with_capacity(row_len * height);
        for row in data.chunks(stride).take(height) {
            rgba.extend_from_slice(&row[..row_len]);
        }

        Ok(VideoFrame {
            width: key.out_width,
            height: key.out_height,
            rgba,
            pts_us,
        })
    }
}

/// A decoded picture and its device time (µs).
pub struct DecodedFrame {
    pub frame: frame::Video,
    pub pts_us: Option<u64>,
}

/// New reference to the same picture buffers (no pixel copy).
fn frame_ref(source: &frame::Video) -> Result<frame::Video, DecodeError> {
    let mut frame = frame::Video::empty();
    // SAFETY: both frames are valid; av_frame_ref adds references to the
    // source's buffers, released when `frame` is dropped.
    let ret = unsafe { ffmpeg::ffi::av_frame_ref(frame.as_mut_ptr(), source.as_ptr()) };
    if ret < 0 {
        return Err(ffmpeg::Error::from(ret).into());
    }
    Ok(frame)
}

/// Receives every available frame from `decoder`, calling `on_frame` for
/// each; stops when the decoder needs more input.
pub fn drain_frames(
    decoder: &mut ffmpeg::decoder::Video,
    frame: &mut frame::Video,
    mut on_frame: impl FnMut(&frame::Video) -> Result<(), DecodeError>,
) -> Result<(), DecodeError> {
    loop {
        match decoder.receive_frame(frame) {
            Ok(()) => on_frame(frame)?,
            Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN => {
                return Ok(())
            }
            Err(ffmpeg::Error::Eof) => return Ok(()),
            Err(err) => return Err(err.into()),
        }
    }
}

pub struct H264Decoder {
    decoder: ffmpeg::decoder::Video,
    converter: RgbaConverter,
    /// SPS/PPS waiting to be prepended to the next frame packet (scrcpy sends
    /// them as a separate "config" packet).
    pending_config: Option<Vec<u8>>,
    decoded: frame::Video,
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
            converter: RgbaConverter::default(),
            pending_config: None,
            decoded: frame::Video::empty(),
        })
    }

    /// Feeds one packet; returns the newest decoded frame, if one came out.
    pub fn decode(&mut self, packet: &MediaPacket) -> Result<Option<VideoFrame>, DecodeError> {
        match self.decode_raw(packet)? {
            Some(decoded) => Ok(Some(
                self.converter.convert(&decoded.frame, decoded.pts_us)?,
            )),
            None => Ok(None),
        }
    }

    /// Feeds one packet; returns the newest decoded picture without converting
    /// it (it shares the decoder's buffers: no copy).
    pub fn decode_raw(
        &mut self,
        packet: &MediaPacket,
    ) -> Result<Option<DecodedFrame>, DecodeError> {
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
        drain_frames(&mut self.decoder, &mut self.decoded, |decoded| {
            latest = Some(DecodedFrame {
                frame: frame_ref(decoded)?,
                pts_us: decoded.pts().and_then(|p| u64::try_from(p).ok()),
            });
            Ok(())
        })?;
        Ok(latest)
    }
}
