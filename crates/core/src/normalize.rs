//! Makes a recording with a rotation playable everywhere.
//!
//! When the phone rotates, its encoder restarts at the new size and the
//! recording carries the new SPS inside the track. FFmpeg decodes that, but
//! common players (Windows Media Player, VLC) keep the size of the file header
//! and show garbage after the rotation. A Matroska track cannot change its
//! codec description, so the whole video is re-encoded into one fixed frame:
//! a **square** as large as the longest side, so portrait and landscape
//! pictures both keep their real size (black bars around). With a portrait
//! frame, landscape parts would shrink to less than half (product choice,
//! ADR 0010). The sound is copied as-is.
//!
//! The result replaces the original file only once complete: if anything
//! fails, the original (readable by PhoneCut and FFmpeg) is kept.

use std::fs;
use std::path::{Path, PathBuf};

use ffmpeg::util::mathematics::Rescale;
use ffmpeg::{codec, format, media, Dictionary, Rational};
use ffmpeg_next as ffmpeg;

use crate::decode::{drain_frames, init_ffmpeg, DecodeError};
use crate::export::{EncodeSink, ExportError, SinkError};

const MILLIS: Rational = Rational(1, 1000);
/// Bit rate when the recording does not tell its own.
const DEFAULT_BIT_RATE: usize = 10_000_000;

/// Temporary output next to the recording. Not a `.mkv`, so it never shows up
/// in "My clips" if PhoneCut is closed during the work.
pub fn temp_path(recording: &Path) -> PathBuf {
    let mut name = recording.file_name().unwrap_or_default().to_os_string();
    name.push(".rotation-fix");
    recording.with_file_name(name)
}

/// Re-encodes `recording` in place (see the module doc). `progress` gets
/// 0.0..=1.0.
pub fn fix_rotation(recording: &Path, progress: &mut dyn FnMut(f32)) -> Result<(), ExportError> {
    let temp = temp_path(recording);
    let result = rewrite(recording, &temp, progress);
    match result {
        Ok(()) => fs::rename(&temp, recording).map_err(|err| {
            tracing::error!(%err, "cannot replace the recording with the fixed one");
            let _ = fs::remove_file(&temp);
            ExportError::Write(ffmpeg::Error::Other {
                errno: err.raw_os_error().unwrap_or(0),
            })
        }),
        Err(err) => {
            let _ = fs::remove_file(&temp);
            Err(err)
        }
    }
}

fn rewrite(
    input_path: &Path,
    output: &Path,
    progress: &mut dyn FnMut(f32),
) -> Result<(), ExportError> {
    init_ffmpeg();
    let mut input = format::input(input_path).map_err(ExportError::Read)?;
    let (video_index, video_tb, video_params) = {
        let stream = input
            .streams()
            .best(media::Type::Video)
            .ok_or(ExportError::Read(ffmpeg::Error::StreamNotFound))?;
        (stream.index(), stream.time_base(), stream.parameters())
    };
    let audio = input
        .streams()
        .best(media::Type::Audio)
        .map(|s| (s.index(), s.time_base(), s.parameters()));
    let mut decoder = codec::Context::from_parameters(video_params)
        .and_then(|c| c.decoder().video())
        .map_err(ExportError::Read)?;
    let bit_rate = match input.bit_rate() {
        rate if rate > 0 => usize::try_from(rate).unwrap_or(DEFAULT_BIT_RATE) * 5 / 4,
        _ => DEFAULT_BIT_RATE,
    };
    let duration_us = input.duration().max(1);

    let mut out = format::output_as(output, "matroska").map_err(ExportError::Write)?;
    let mut job = Rewrite {
        video_tb,
        audio: audio.map(|(index, tb, params)| AudioCopy {
            input_index: index,
            input_tb: tb,
            params,
            output_index: None,
        }),
        bit_rate,
        sink: None,
        waiting_audio: Vec::new(),
        duration_us,
    };
    let mut decoded = ffmpeg::util::frame::Video::empty();
    let mut packet = ffmpeg::Packet::empty();
    loop {
        match packet.read(&mut input) {
            Ok(()) => {
                if job
                    .audio
                    .as_ref()
                    .is_some_and(|a| a.input_index == packet.stream())
                {
                    job.on_audio(&packet, &mut out)?;
                    continue;
                }
                if packet.stream() != video_index {
                    continue;
                }
                if let Err(err) = decoder.send_packet(&packet) {
                    tracing::debug!(%err, "packet skipped");
                    continue;
                }
            }
            Err(ffmpeg::Error::Eof) => {
                decoder.send_eof().map_err(ExportError::Read)?;
                drain_frames(&mut decoder, &mut decoded, |frame| {
                    job.on_frame(frame, &mut out, progress)
                })
                .map_err(sink_failure)?;
                break;
            }
            Err(err) => return Err(ExportError::Read(err)),
        }
        drain_frames(&mut decoder, &mut decoded, |frame| {
            job.on_frame(frame, &mut out, progress)
        })
        .map_err(sink_failure)?;
    }
    let Some(mut sink) = job.sink else {
        return Err(ExportError::Read(ffmpeg::Error::InvalidData));
    };
    sink.finish(&mut out, duration_us / 1000)
        .map_err(|e| match e {
            SinkError::NoEncoder => ExportError::NoEncoder,
            SinkError::Ffmpeg(e) => ExportError::Write(e),
        })?;
    out.write_trailer().map_err(ExportError::Write)?;
    progress(1.0);
    Ok(())
}

struct AudioCopy {
    input_index: usize,
    input_tb: Rational,
    params: codec::Parameters,
    /// Set once the output streams exist (first decoded picture).
    output_index: Option<usize>,
}

struct Rewrite {
    video_tb: Rational,
    audio: Option<AudioCopy>,
    bit_rate: usize,
    /// Created on the first picture: a square frame from its size.
    sink: Option<EncodeSink>,
    /// Sound read before the file header could be written.
    waiting_audio: Vec<ffmpeg::Packet>,
    duration_us: i64,
}

impl Rewrite {
    fn on_frame(
        &mut self,
        frame: &ffmpeg::util::frame::Video,
        out: &mut format::context::Output,
        progress: &mut dyn FnMut(f32),
    ) -> Result<(), DecodeError> {
        let pts = frame.timestamp().or(frame.pts()).unwrap_or(0);
        let time_ms = pts.rescale(self.video_tb, MILLIS).max(0);
        if self.sink.is_none() {
            self.start(out, frame).map_err(sink_error)?;
        }
        if let Some(sink) = &mut self.sink {
            sink.encode(out, frame, time_ms).map_err(sink_error)?;
        }
        progress((time_ms * 1000) as f32 / self.duration_us as f32);
        Ok(())
    }

    /// Adds the streams (video at the size of `first`, copied sound) and
    /// writes the file header.
    fn start(
        &mut self,
        out: &mut format::context::Output,
        first: &ffmpeg::util::frame::Video,
    ) -> Result<(), SinkError> {
        self.sink = Some(EncodeSink::new(
            out,
            square_frame(first.width(), first.height()),
            self.bit_rate,
        )?);
        if let Some(audio) = &mut self.audio {
            let mut stream = out.add_stream(Option::<ffmpeg::Codec>::None)?;
            stream.set_parameters(audio.params.clone());
            // SAFETY: the stream was just created; a zero tag lets the
            // Matroska muxer pick its own for AAC.
            unsafe {
                (*(*stream.as_mut_ptr()).codecpar).codec_tag = 0;
            }
            audio.output_index = Some(stream.index());
        }
        let mut options = Dictionary::new();
        options.set("max_interleave_delta", "1000000");
        out.write_header_with(options)?;
        for packet in std::mem::take(&mut self.waiting_audio) {
            self.write_audio(packet, out)?;
        }
        Ok(())
    }

    fn on_audio(
        &mut self,
        packet: &ffmpeg::Packet,
        out: &mut format::context::Output,
    ) -> Result<(), ExportError> {
        if self.sink.is_none() {
            self.waiting_audio.push(packet.clone());
            return Ok(());
        }
        self.write_audio(packet.clone(), out).map_err(|e| match e {
            SinkError::NoEncoder => ExportError::NoEncoder,
            SinkError::Ffmpeg(e) => ExportError::Write(e),
        })
    }

    fn write_audio(
        &self,
        mut packet: ffmpeg::Packet,
        out: &mut format::context::Output,
    ) -> Result<(), SinkError> {
        let Some(audio) = &self.audio else {
            return Ok(());
        };
        let Some(index) = audio.output_index else {
            return Ok(());
        };
        let out_tb = out
            .stream(index)
            .map(|s| s.time_base())
            .ok_or(ffmpeg::Error::StreamNotFound)?;
        packet.set_stream(index);
        packet.rescale_ts(audio.input_tb, out_tb);
        packet.set_position(-1);
        packet.write_interleaved(out)?;
        Ok(())
    }
}

/// Square frame holding both orientations of a `width`×`height` picture at
/// their real size (even side, as YUV 4:2:0 requires).
pub fn square_frame(width: u32, height: u32) -> (u32, u32) {
    let side = (width.max(height) + 1) & !1;
    (side, side)
}

fn sink_error(err: SinkError) -> DecodeError {
    match err {
        SinkError::NoEncoder => DecodeError::NoDecoder,
        SinkError::Ffmpeg(e) => DecodeError::Ffmpeg(e),
    }
}

fn sink_failure(err: DecodeError) -> ExportError {
    match err {
        DecodeError::NoDecoder => ExportError::NoEncoder,
        DecodeError::Ffmpeg(e) => ExportError::Write(e),
    }
}
