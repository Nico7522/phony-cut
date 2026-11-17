//! Still images (tutorial pictures) decoded with FFmpeg, so the app needs no
//! extra image library. PNG and JPEG are supported by the bundled build.

use std::path::Path;

use ffmpeg_next as ffmpeg;

use crate::api::VideoFrame;
use crate::decode::{drain_frames, init_ffmpeg, DecodeError, RgbaConverter};

/// Decodes the first frame of an image file to RGBA.
pub fn load_rgba(path: &Path) -> Result<VideoFrame, DecodeError> {
    init_ffmpeg();
    let mut input = ffmpeg::format::input(path)?;
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or(ffmpeg::Error::StreamNotFound)?;
    let index = stream.index();
    let mut decoder = ffmpeg::codec::Context::from_parameters(stream.parameters())?
        .decoder()
        .video()?;

    let mut converter = RgbaConverter::default();
    let mut frame = ffmpeg::util::frame::Video::empty();
    let mut image = None;
    let mut keep = |decoded: &ffmpeg::util::frame::Video| {
        if image.is_none() {
            image = Some(converter.convert(decoded, None)?);
        }
        Ok(())
    };
    for (s, packet) in input.packets() {
        if s.index() == index {
            decoder.send_packet(&packet)?;
            drain_frames(&mut decoder, &mut frame, &mut keep)?;
        }
    }
    decoder.send_eof()?;
    drain_frames(&mut decoder, &mut frame, &mut keep)?;
    image.ok_or(DecodeError::Ffmpeg(ffmpeg::Error::InvalidData))
}
