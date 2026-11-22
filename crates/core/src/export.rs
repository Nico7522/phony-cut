//! Clip export to `.mp4` (H.264), one file per segment.
//!
//! - **Fast**: stream copy (no re-encoding). The clip starts on the key frame
//!   at or before the in point (≤ ~1 s earlier with one key frame per second)
//!   and ends exactly before the out point (no B-frames in the stream).
//! - **Precise**: decode + re-encode (libopenh264, BSD) from the exact in point.
//!   Slower; the whole clip is re-encoded (see docs/decisions/0005).
//!
//! - **Social network formats** (9:16, 16:9, 1:1, 4:5): re-encoded like
//!   Precise, the picture fitted in the format over a blurred copy of itself
//!   (or black), optionally inside a plain phone frame.
//!
//! MP4 is written with `+faststart` (index at the beginning) so the files play
//! everywhere, including browsers and phones.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::Sender;
use ffmpeg::util::mathematics::Rescale;
use ffmpeg::{codec, encoder, format, media, software::scaling, Dictionary, Rational};
use ffmpeg_next as ffmpeg;

use crate::api::{
    Event, ExportFormat, ExportMode, ExportProblem, ExportRequest, ExportState, ExportStyle,
    Notifier,
};
use crate::decode::{drain_frames, init_ffmpeg, DecodeError};
use crate::markers::{file_safe_name, Segment};
use crate::record::unique_path;

const MICROS: Rational = Rational(1, 1_000_000);
/// Encoder time base for precise export (variable frame rate, ms precision).
const ENCODER_TIME_BASE: Rational = Rational(1, 1000);
/// Fallback bit rate for precise export when the source one is unknown.
const DEFAULT_BIT_RATE: usize = 10_000_000;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("cannot read the recording: {0}")]
    Read(#[source] ffmpeg::Error),
    #[error("cannot write the clip: {0}")]
    Write(#[source] ffmpeg::Error),
    #[error("decoding failed: {0}")]
    Decode(#[from] DecodeError),
    #[error("no H.264 encoder available")]
    NoEncoder,
    #[error("cancelled")]
    Cancelled,
}

impl ExportError {
    fn problem(&self) -> ExportProblem {
        match self {
            Self::Read(_) | Self::Decode(_) => ExportProblem::CannotRead,
            Self::Write(_) => ExportProblem::CannotWrite,
            Self::NoEncoder => ExportProblem::NoEncoder,
            // Not reported as a problem (ExportState::Cancelled instead).
            Self::Cancelled => ExportProblem::CannotWrite,
        }
    }
}

/// `…/PhonyCut 2026-11-17 17-22-45.mkv` → `…/PhonyCut 2026-11-17 17-22-45 - clips/`
pub fn clips_folder(video: &Path) -> PathBuf {
    let stem = video
        .file_stem()
        .map_or_else(|| "video".to_owned(), |s| s.to_string_lossy().into_owned());
    video
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("{stem} - clips"))
}

/// `01 - Mon clip.mp4`, `01 - Mon clip (9x16).mp4`
pub fn clip_file_name(index: usize, segment: &Segment, format: ExportFormat) -> String {
    let name = file_safe_name(&segment.name);
    let ext = format.extension();
    match format.tag() {
        Some(tag) => format!("{:02} - {name} ({tag}).{ext}", index + 1),
        None => format!("{:02} - {name}.{ext}", index + 1),
    }
}

/// Reports progress (0..=1) and returns `false` to cancel.
pub type Progress<'a> = &'a mut dyn FnMut(f32) -> bool;

/// Copies the sound track (AAC from the phone) of a recording into a clip,
/// unchanged: the packets between the clip origin and its end, with their
/// times shifted so the clip starts at 0. Packets read before the output is
/// ready (origin unknown, header not written) wait in memory.
pub(crate) struct AudioCopy {
    input_index: usize,
    input_tb: Rational,
    params: codec::Parameters,
    end_us: i64,
    output_index: Option<usize>,
    header_written: bool,
    /// Recording time (µs) that becomes time 0 in the clip.
    origin_us: Option<i64>,
    waiting: Vec<ffmpeg::Packet>,
}

impl AudioCopy {
    /// The sound track of `input`, if it has one; packets from `end_us` on
    /// are dropped.
    pub(crate) fn find(input: &format::context::Input, end_us: i64) -> Option<Self> {
        let stream = input.streams().best(media::Type::Audio)?;
        Some(Self {
            input_index: stream.index(),
            input_tb: stream.time_base(),
            params: stream.parameters(),
            end_us,
            output_index: None,
            header_written: false,
            origin_us: None,
            waiting: Vec::new(),
        })
    }

    pub(crate) fn is_audio(&self, packet: &ffmpeg::Packet) -> bool {
        packet.stream() == self.input_index
    }

    /// Adds the sound stream to `out` (before its header is written).
    pub(crate) fn add_stream(
        &mut self,
        out: &mut format::context::Output,
    ) -> Result<(), ffmpeg::Error> {
        let mut stream = out.add_stream(Option::<ffmpeg::Codec>::None)?;
        stream.set_parameters(self.params.clone());
        // SAFETY: `stream` was just created and is owned by `out`; codecpar is
        // valid. Let the muxer pick its own tag for AAC.
        unsafe {
            (*(*stream.as_mut_ptr()).codecpar).codec_tag = 0;
        }
        self.output_index = Some(stream.index());
        Ok(())
    }

    pub(crate) fn header_written(
        &mut self,
        out: &mut format::context::Output,
    ) -> Result<(), ffmpeg::Error> {
        self.header_written = true;
        self.flush(out)
    }

    pub(crate) fn set_origin(
        &mut self,
        origin_us: i64,
        out: &mut format::context::Output,
    ) -> Result<(), ffmpeg::Error> {
        self.origin_us = Some(origin_us);
        self.flush(out)
    }

    /// Writes a sound packet now, or keeps it until the output is ready.
    pub(crate) fn on_packet(
        &mut self,
        packet: &ffmpeg::Packet,
        out: &mut format::context::Output,
    ) -> Result<(), ffmpeg::Error> {
        if self.header_written && self.origin_us.is_some() {
            self.write(packet.clone(), out)
        } else {
            self.waiting.push(packet.clone());
            Ok(())
        }
    }

    fn flush(&mut self, out: &mut format::context::Output) -> Result<(), ffmpeg::Error> {
        if !self.header_written || self.origin_us.is_none() {
            return Ok(());
        }
        for packet in std::mem::take(&mut self.waiting) {
            self.write(packet, out)?;
        }
        Ok(())
    }

    fn write(
        &self,
        mut packet: ffmpeg::Packet,
        out: &mut format::context::Output,
    ) -> Result<(), ffmpeg::Error> {
        let (Some(index), Some(origin_us), Some(pts)) =
            (self.output_index, self.origin_us, packet.pts())
        else {
            return Ok(());
        };
        let time_us = pts.rescale(self.input_tb, MICROS);
        if time_us < origin_us || time_us >= self.end_us {
            return Ok(());
        }
        let out_tb = out
            .stream(index)
            .map(|s| s.time_base())
            .ok_or(ffmpeg::Error::StreamNotFound)?;
        let shifted = (time_us - origin_us).rescale(MICROS, out_tb);
        let duration = packet.duration().rescale(self.input_tb, out_tb);
        packet.set_pts(Some(shifted));
        packet.set_dts(Some(shifted));
        packet.set_duration(duration);
        packet.set_position(-1);
        packet.set_stream(index);
        packet.write_interleaved(out)
    }
}

/// Exports one segment with stream copy.
pub fn export_fast(
    video: &Path,
    segment: &Segment,
    output: &Path,
    progress: Progress<'_>,
) -> Result<(), ExportError> {
    init_ffmpeg();
    let mut input = format::input(video).map_err(ExportError::Read)?;
    let (index, in_tb, params) = {
        let stream = input
            .streams()
            .best(media::Type::Video)
            .ok_or(ExportError::Read(ffmpeg::Error::StreamNotFound))?;
        (stream.index(), stream.time_base(), stream.parameters())
    };

    let mut out = format::output_as(output, "mp4").map_err(ExportError::Write)?;
    {
        let mut stream = out
            .add_stream(Option::<ffmpeg::Codec>::None)
            .map_err(ExportError::Write)?;
        stream.set_parameters(params);
        // SAFETY: `stream` was just created and is owned by `out`; codecpar is
        // valid. The Matroska codec tag means nothing to MP4: let the muxer
        // pick the right one.
        unsafe {
            (*(*stream.as_mut_ptr()).codecpar).codec_tag = 0;
        }
    }
    let (start_us, end_us) = segment_us(segment);
    let mut audio = AudioCopy::find(&input, end_us);
    if let Some(audio) = &mut audio {
        audio.add_stream(&mut out).map_err(ExportError::Write)?;
    }
    write_mp4_header(&mut out)?;
    if let Some(audio) = &mut audio {
        audio.header_written(&mut out).map_err(ExportError::Write)?;
    }
    let out_tb = out
        .stream(0)
        .map(|s| s.time_base())
        .ok_or(ExportError::Write(ffmpeg::Error::StreamNotFound))?;

    // The last frame of the clip lasts until the out point (or the end of
    // the video): MKV blocks carry no duration, so it must be set here.
    let clip_end_us = end_us.min(input.duration().max(0));
    input
        .seek(start_us, ..start_us)
        .map_err(ExportError::Read)?;

    let mut origin: Option<i64> = None;
    // Written one packet late, so each one gets its real duration.
    let mut pending: Option<(ffmpeg::Packet, i64)> = None;
    let write = |mut packet: ffmpeg::Packet,
                 time_us: i64,
                 until_us: i64,
                 out: &mut format::context::Output| {
        packet.set_duration((until_us - time_us).max(0).rescale(MICROS, in_tb));
        packet.rescale_ts(in_tb, out_tb);
        packet.write_interleaved(out).map_err(ExportError::Write)
    };
    let mut packet = ffmpeg::Packet::empty();
    loop {
        match packet.read(&mut input) {
            Ok(()) => {}
            Err(ffmpeg::Error::Eof) => break,
            Err(err) => return Err(ExportError::Read(err)),
        }
        if let Some(audio) = audio.as_mut().filter(|a| a.is_audio(&packet)) {
            audio
                .on_packet(&packet, &mut out)
                .map_err(ExportError::Write)?;
            continue;
        }
        if packet.stream() != index {
            continue;
        }
        let Some(pts) = packet.pts() else { continue };
        let time_us = pts.rescale(in_tb, MICROS);
        if time_us >= end_us {
            break;
        }
        // A clip must start on a key frame (the seek lands on one; skip
        // anything before it, just in case). The sound starts there too.
        let origin_pts = match origin {
            Some(o) => o,
            None if packet.is_key() => {
                if let Some(audio) = &mut audio {
                    audio
                        .set_origin(time_us, &mut out)
                        .map_err(ExportError::Write)?;
                }
                *origin.insert(pts)
            }
            None => continue,
        };
        let span = (end_us - start_us).max(1) as f32;
        if !progress(((time_us - start_us).max(0) as f32 / span).min(1.0)) {
            return Err(ExportError::Cancelled);
        }
        let shifted = pts - origin_pts;
        packet.set_pts(Some(shifted));
        packet.set_dts(Some(shifted));
        packet.set_position(-1);
        packet.set_stream(0);
        if let Some((previous, previous_us)) = pending.take() {
            write(previous, previous_us, time_us, &mut out)?;
        }
        pending = Some((packet.clone(), time_us));
    }
    let Some((last, last_us)) = pending else {
        return Err(ExportError::Read(ffmpeg::Error::InvalidData));
    };
    write(last, last_us, clip_end_us.max(last_us), &mut out)?;
    out.write_trailer().map_err(ExportError::Write)?;
    progress(1.0);
    Ok(())
}

/// Exports one segment by re-encoding it, starting exactly on the in point.
pub fn export_precise(
    video: &Path,
    segment: &Segment,
    output: &Path,
    style: ExportStyle,
    progress: Progress<'_>,
) -> Result<(), ExportError> {
    init_ffmpeg();
    let mut input = format::input(video).map_err(ExportError::Read)?;
    let (index, in_tb, params) = {
        let stream = input
            .streams()
            .best(media::Type::Video)
            .ok_or(ExportError::Read(ffmpeg::Error::StreamNotFound))?;
        (stream.index(), stream.time_base(), stream.parameters())
    };
    let mut decoder = codec::Context::from_parameters(params)
        .and_then(|c| c.decoder().video())
        .map_err(ExportError::Read)?;
    let mut bit_rate = match input.bit_rate() {
        rate if rate > 0 => usize::try_from(rate).unwrap_or(DEFAULT_BIT_RATE) * 5 / 4,
        _ => DEFAULT_BIT_RATE,
    };
    let target = style.format.size();
    if let Some((w, h)) = target {
        // About 4 bits per pixel and second: 8 Mb/s for 1080×1920.
        let wanted = usize::try_from(u64::from(w) * u64::from(h) * 4).unwrap_or(DEFAULT_BIT_RATE);
        bit_rate = bit_rate.max(wanted).min(20_000_000);
    }
    let layout = Layout {
        blur: target.is_some() && style.blur_background,
        frame: target.is_some() && style.phone_frame,
    };

    let (start_us, end_us) = segment_us(segment);
    let clip_end_ms = (end_us.min(input.duration().max(0)) - start_us).max(0) / 1000;
    input
        .seek(start_us, ..start_us)
        .map_err(ExportError::Read)?;

    let mut out = format::output_as(output, "mp4").map_err(ExportError::Write)?;
    // The sound starts exactly at the in point, like the re-encoded picture.
    let mut audio = AudioCopy::find(&input, end_us);
    if let Some(audio) = &mut audio {
        audio
            .set_origin(start_us, &mut out)
            .map_err(ExportError::Write)?;
    }
    let mut clip = PreciseClip {
        in_tb,
        start_us,
        end_us,
        bit_rate,
        target,
        layout,
        sink: None,
        audio,
        finished: false,
        cancelled: false,
    };
    let mut decoded = ffmpeg::util::frame::Video::empty();
    let mut packet = ffmpeg::Packet::empty();
    while !clip.finished {
        match packet.read(&mut input) {
            Ok(()) => {
                if let Some(audio) = clip.audio.as_mut().filter(|a| a.is_audio(&packet)) {
                    audio
                        .on_packet(&packet, &mut out)
                        .map_err(ExportError::Write)?;
                    continue;
                }
                if packet.stream() != index {
                    continue;
                }
                if let Err(err) = decoder.send_packet(&packet) {
                    tracing::debug!(%err, "packet skipped");
                    continue;
                }
            }
            Err(ffmpeg::Error::Eof) => {
                decoder.send_eof().map_err(ExportError::Read)?;
                clip.finished = true;
            }
            Err(err) => return Err(ExportError::Read(err)),
        }
        drain_frames(&mut decoder, &mut decoded, |frame| {
            clip.on_frame(frame, &mut out, progress)
        })
        .map_err(unwrap_sink_error)?;
    }
    if clip.cancelled {
        return Err(ExportError::Cancelled);
    }
    let Some(mut sink) = clip.sink else {
        return Err(ExportError::Read(ffmpeg::Error::InvalidData));
    };
    sink.finish(&mut out, clip_end_ms).map_err(|e| match e {
        SinkError::NoEncoder => ExportError::NoEncoder,
        SinkError::Ffmpeg(e) => ExportError::Write(e),
    })?;
    out.write_trailer().map_err(ExportError::Write)?;
    progress(1.0);
    Ok(())
}

/// State of a precise export while frames are decoded.
struct PreciseClip {
    in_tb: Rational,
    start_us: i64,
    end_us: i64,
    bit_rate: usize,
    /// Social network format size (`None`: the first frame's size).
    target: Option<(u32, u32)>,
    layout: Layout,
    /// Created on the first frame of the clip (its size).
    sink: Option<EncodeSink>,
    /// The recording's sound, copied.
    audio: Option<AudioCopy>,
    finished: bool,
    cancelled: bool,
}

impl PreciseClip {
    fn on_frame(
        &mut self,
        frame: &ffmpeg::util::frame::Video,
        out: &mut format::context::Output,
        progress: Progress<'_>,
    ) -> Result<(), DecodeError> {
        if self.finished {
            return Ok(());
        }
        let pts = frame.timestamp().or(frame.pts()).unwrap_or(0);
        let time_us = pts.rescale(self.in_tb, MICROS);
        if time_us < self.start_us {
            return Ok(()); // between the key frame and the in point
        }
        if time_us >= self.end_us {
            self.finished = true;
            return Ok(());
        }
        let span = (self.end_us - self.start_us).max(1) as f32;
        if !progress(((time_us - self.start_us) as f32 / span).min(1.0)) {
            self.cancelled = true;
            self.finished = true;
            return Ok(());
        }
        if self.sink.is_none() {
            let size = self.target.unwrap_or((frame.width(), frame.height()));
            let sink = EncodeSink::new(out, size, self.bit_rate).map_err(sink_error)?;
            self.sink = Some(sink.with_layout(self.layout));
            if let Some(audio) = &mut self.audio {
                audio.add_stream(out)?;
            }
            write_mp4_header(out).map_err(|e| match e {
                ExportError::Write(e) => DecodeError::Ffmpeg(e),
                _ => DecodeError::Ffmpeg(ffmpeg::Error::Bug),
            })?;
            if let Some(audio) = &mut self.audio {
                audio.header_written(out)?;
            }
        }
        if let Some(sink) = &mut self.sink {
            sink.encode(out, frame, (time_us - self.start_us) / 1000)
                .map_err(sink_error)?;
        }
        Ok(())
    }
}

fn segment_us(segment: &Segment) -> (i64, i64) {
    let ms = |v: u64| i64::try_from(v).unwrap_or(i64::MAX / 1000) * 1000;
    (ms(segment.start_ms), ms(segment.end_ms))
}

fn write_mp4_header(out: &mut format::context::Output) -> Result<(), ExportError> {
    let mut options = Dictionary::new();
    options.set("movflags", "+faststart");
    out.write_header_with(options).map_err(ExportError::Write)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SinkError {
    #[error("no H.264 encoder")]
    NoEncoder,
    #[error("{0}")]
    Ffmpeg(#[from] ffmpeg::Error),
}

// The frame callback can only return DecodeError: carry encoder errors
// through it and turn them back into export errors afterwards.
fn sink_error(err: SinkError) -> DecodeError {
    match err {
        SinkError::NoEncoder => DecodeError::NoDecoder,
        SinkError::Ffmpeg(e) => DecodeError::Ffmpeg(e),
    }
}

fn unwrap_sink_error(err: DecodeError) -> ExportError {
    match err {
        DecodeError::NoDecoder => ExportError::NoEncoder,
        DecodeError::Ffmpeg(e) => ExportError::Write(e),
    }
}

/// H.264 encoder + output stream of a fixed size. Adding the
/// stream does not write the file header: the caller adds its other streams
/// first, then writes the header.
pub(crate) struct EncodeSink {
    encoder: encoder::Video,
    /// Converts frames to YUV420P at their fitted size inside the clip.
    scaler: Option<(ScalerInput, scaling::Context)>,
    scaled: ffmpeg::util::frame::Video,
    /// Clip-sized frame handed to the encoder (black bars around `scaled`
    /// when a rotation changes the aspect ratio inside the clip).
    canvas: ffmpeg::util::frame::Video,
    /// Index of the encoded stream in the output.
    index: usize,
    width: u32,
    height: u32,
    /// Encoded packet waiting for the next one (to know its duration).
    pending: Option<ffmpeg::Packet>,
    layout: Layout,
    /// Blurred background, rebuilt when the input size changes.
    background: Option<Background>,
}

/// What surrounds the picture in a social network format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Layout {
    /// Blurred, darkened copy of the picture behind it (else black).
    pub blur: bool,
    /// Plain dark phone frame around the picture.
    pub frame: bool,
}

/// Where the picture and its frame go in the clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placement {
    /// Picture size and position.
    size: (u32, u32),
    at: (u32, u32),
    /// Frame thickness and outer corner radius (0 without a frame).
    border: u32,
    radius: u32,
}

/// Fits a picture of `src` in `clip`, leaving room for the frame.
fn place(src: (u32, u32), clip: (u32, u32), frame: bool) -> Placement {
    if !frame {
        let (size, at) = fit_inside(src, clip);
        return Placement {
            size,
            at,
            border: 0,
            radius: 0,
        };
    }
    let short = clip.0.min(clip.1);
    let margin = (short * 6 / 100) & !1;
    let border = ((short * 18 / 1000).max(4)) & !1;
    let room = |side: u32| side.saturating_sub(2 * (margin + border)).max(2);
    let ((w, h), _) = fit_inside(src, (room(clip.0), room(clip.1)));
    let at = (((clip.0 - w) / 2) & !1, ((clip.1 - h) / 2) & !1);
    Placement {
        size: (w, h),
        at,
        border,
        radius: w.min(h) * 6 / 100 + border,
    }
}

/// The picture scaled to cover a tiny frame, blurred, then scaled back up to
/// the clip size: a soft, darkened background.
struct Background {
    input: ScalerInput,
    cover: scaling::Context,
    covered: ffmpeg::util::frame::Video,
    small: ffmpeg::util::frame::Video,
    /// Offset of the centered crop in `covered`.
    crop: (u32, u32),
    up: scaling::Context,
    frame: ffmpeg::util::frame::Video,
}

impl Background {
    fn new(input: ScalerInput, clip: (u32, u32)) -> Result<Self, SinkError> {
        let even = |v: u32| (v & !1).max(2);
        let small = (even(clip.0 / 24), even(clip.1 / 24));
        let scale = (f64::from(small.0) / f64::from(input.width.max(1)))
            .max(f64::from(small.1) / f64::from(input.height.max(1)));
        let cover_size = (
            even((f64::from(input.width) * scale).ceil() as u32 + 1).max(small.0),
            even((f64::from(input.height) * scale).ceil() as u32 + 1).max(small.1),
        );
        let cover = scaling::Context::get(
            input.format,
            input.width,
            input.height,
            format::Pixel::YUV420P,
            cover_size.0,
            cover_size.1,
            scaling::Flags::AREA,
        )?;
        let up = scaling::Context::get(
            format::Pixel::YUV420P,
            small.0,
            small.1,
            format::Pixel::YUV420P,
            clip.0,
            clip.1,
            scaling::Flags::BICUBIC,
        )?;
        Ok(Self {
            input,
            cover,
            covered: ffmpeg::util::frame::Video::empty(),
            small: ffmpeg::util::frame::Video::new(format::Pixel::YUV420P, small.0, small.1),
            crop: (
                ((cover_size.0 - small.0) / 2) & !1,
                ((cover_size.1 - small.1) / 2) & !1,
            ),
            up,
            frame: ffmpeg::util::frame::Video::empty(),
        })
    }

    /// Draws the background of `picture` into `canvas`.
    fn draw(
        &mut self,
        picture: &ffmpeg::util::frame::Video,
        canvas: &mut ffmpeg::util::frame::Video,
    ) -> Result<(), SinkError> {
        self.cover.run(picture, &mut self.covered)?;
        let (w, h) = (self.small.width(), self.small.height());
        copy_region(&self.covered, self.crop, &mut self.small, (0, 0), (w, h));
        for plane in 0..3 {
            box_blur(&mut self.small, plane);
            box_blur(&mut self.small, plane);
        }
        self.up.run(&self.small, &mut self.frame)?;
        copy_region(
            &self.frame,
            (0, 0),
            canvas,
            (0, 0),
            (canvas.width(), canvas.height()),
        );
        // Darker, so the picture stands out.
        for y in canvas.data_mut(0) {
            *y = 16 + ((u16::from(y.saturating_sub(16)) * 11) / 20) as u8;
        }
        Ok(())
    }
}

/// 3×3 box blur of one plane, in place.
fn box_blur(frame: &mut ffmpeg::util::frame::Video, plane: usize) {
    let shift = u32::from(plane > 0);
    let (w, h) = (
        (frame.width() >> shift) as usize,
        (frame.height() >> shift) as usize,
    );
    let stride = frame.stride(plane);
    let data = frame.data_mut(plane);
    if w < 3 || h < 3 || data.len() < stride * h {
        return;
    }
    let source: Vec<u8> = data.to_vec();
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0u32;
            for dy in [-1i64, 0, 1] {
                for dx in [-1i64, 0, 1] {
                    let yy = (y as i64 + dy).clamp(0, h as i64 - 1) as usize;
                    let xx = (x as i64 + dx).clamp(0, w as i64 - 1) as usize;
                    sum += u32::from(source[yy * stride + xx]);
                }
            }
            data[y * stride + x] = (sum / 9) as u8;
        }
    }
}

/// YUV (limited range) of the frame: near black, with a lighter edge.
const FRAME_BODY: [u8; 3] = [26, 128, 128];
const FRAME_EDGE: [u8; 3] = [70, 128, 128];

/// Horizontal span `[from, to)` of row `y` (pixel centre) inside a rounded
/// rectangle, in the same units.
fn rounded_span(rect: (f64, f64, f64, f64), radius: f64, y: f64) -> Option<(f64, f64)> {
    let (x0, y0, w, h) = rect;
    if y < y0 || y >= y0 + h {
        return None;
    }
    let r = radius.min(w / 2.0).min(h / 2.0);
    let dy = if y < y0 + r {
        y0 + r - y
    } else if y > y0 + h - r {
        y - (y0 + h - r)
    } else {
        0.0
    };
    let inset = r - (r * r - dy * dy).max(0.0).sqrt();
    Some((x0 + inset, x0 + w - inset))
}

/// Paints a rounded rectangle (full-resolution coordinates) in every plane:
/// inside it, or only between it and its bounding box (`outside`).
fn paint_rounded(
    frame: &mut ffmpeg::util::frame::Video,
    rect: (u32, u32, u32, u32),
    radius: u32,
    color: [u8; 3],
    outside: bool,
) {
    for (plane, value) in color.into_iter().enumerate() {
        let shift = u32::from(plane > 0);
        let scale = f64::from(1u32 << shift);
        let (fw, fh) = (
            (frame.width() >> shift) as usize,
            (frame.height() >> shift) as usize,
        );
        let stride = frame.stride(plane);
        let data = frame.data_mut(plane);
        let r = (
            f64::from(rect.0) / scale,
            f64::from(rect.1) / scale,
            f64::from(rect.2) / scale,
            f64::from(rect.3) / scale,
        );
        let radius = f64::from(radius) / scale;
        let first = (r.1.floor() as usize).min(fh);
        let last = ((r.1 + r.3).ceil() as usize).min(fh);
        for row in first..last {
            let Some((from, to)) = rounded_span(r, radius, row as f64 + 0.5) else {
                continue;
            };
            let clamp = |v: f64| (v.round().max(0.0) as usize).min(fw);
            let line = &mut data[row * stride..row * stride + fw];
            if outside {
                let (left, right) = (clamp(r.0), clamp(r.0 + r.2));
                let (a, b) = (clamp(from), clamp(to));
                line[left..a.max(left)].fill(value);
                line[b.min(right)..right].fill(value);
            } else {
                let (a, b) = (clamp(from), clamp(to));
                if a < b {
                    line[a..b].fill(value);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScalerInput {
    format: format::Pixel,
    width: u32,
    height: u32,
}

/// Largest size with the source aspect ratio that fits in the clip, rounded
/// to even dimensions (YUV 4:2:0), and its offset to be centered.
pub(crate) fn fit_inside(src: (u32, u32), clip: (u32, u32)) -> ((u32, u32), (u32, u32)) {
    let (sw, sh) = (f64::from(src.0.max(1)), f64::from(src.1.max(1)));
    let (cw, ch) = (f64::from(clip.0), f64::from(clip.1));
    let scale = (cw / sw).min(ch / sh);
    let even = |v: f64, max: u32| ((v as u32) & !1).clamp(2, max);
    let (w, h) = (even(sw * scale, clip.0), even(sh * scale, clip.1));
    ((w, h), (((clip.0 - w) / 2) & !1, ((clip.1 - h) / 2) & !1))
}

impl EncodeSink {
    /// Adds an H.264 stream of `size` (even dimensions) to `out`.
    pub(crate) fn new(
        out: &mut format::context::Output,
        size: (u32, u32),
        bit_rate: usize,
    ) -> Result<Self, SinkError> {
        let codec = encoder::find_by_name("libopenh264")
            .or_else(|| encoder::find(codec::Id::H264))
            .ok_or(SinkError::NoEncoder)?;
        let global_header = out.format().flags().contains(format::Flags::GLOBAL_HEADER);
        let (width, height) = size;

        let mut ctx = codec::Context::new_with_codec(codec).encoder().video()?;
        ctx.set_width(width);
        ctx.set_height(height);
        ctx.set_format(format::Pixel::YUV420P);
        ctx.set_time_base(ENCODER_TIME_BASE);
        ctx.set_frame_rate(Some(Rational(60, 1)));
        ctx.set_bit_rate(bit_rate);
        ctx.set_gop(60);
        if global_header {
            ctx.set_flags(codec::Flags::GLOBAL_HEADER);
        }
        let encoder = ctx.open_as(codec)?;

        let mut stream = out.add_stream(codec)?;
        stream.set_parameters(&encoder);
        stream.set_time_base(ENCODER_TIME_BASE);
        let index = stream.index();

        Ok(Self {
            encoder,
            scaler: None,
            scaled: ffmpeg::util::frame::Video::empty(),
            canvas: ffmpeg::util::frame::Video::new(format::Pixel::YUV420P, width, height),
            index,
            width,
            height,
            pending: None,
            layout: Layout::default(),
            background: None,
        })
    }

    /// Blurred background and phone frame (social network formats).
    pub(crate) fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Encodes a frame at clip time `time_ms`. A frame of another size (a
    /// rotation inside the clip) is fitted without distortion, with black bars.
    pub(crate) fn encode(
        &mut self,
        out: &mut format::context::Output,
        frame: &ffmpeg::util::frame::Video,
        time_ms: i64,
    ) -> Result<(), SinkError> {
        let input = ScalerInput {
            format: frame.format(),
            width: frame.width(),
            height: frame.height(),
        };
        let clip = (self.width, self.height);
        let placement = place((input.width, input.height), clip, self.layout.frame);
        let ((w, h), (x, y)) = (placement.size, placement.at);
        let scaler = match &mut self.scaler {
            Some((current, scaler)) if *current == input => scaler,
            slot => {
                let scaler = scaling::Context::get(
                    input.format,
                    input.width,
                    input.height,
                    format::Pixel::YUV420P,
                    w,
                    h,
                    scaling::Flags::BILINEAR,
                )?;
                // The scaled picture changes size too (rotation): the old
                // buffer would be rejected ("output changed").
                self.scaled = ffmpeg::util::frame::Video::empty();
                &mut slot.insert((input, scaler)).1
            }
        };
        scaler.run(frame, &mut self.scaled)?;
        let fills = (w, h) == clip;
        if self.layout.blur && !fills {
            let background = match &mut self.background {
                Some(background) if background.input == input => background,
                slot => &mut *slot.insert(Background::new(input, clip)?),
            };
            background.draw(frame, &mut self.canvas)?;
        } else if !fills {
            paint_black(&mut self.canvas);
        }
        if placement.border > 0 {
            let b = placement.border;
            let outer = (x - b, y - b, w + 2 * b, h + 2 * b);
            paint_rounded(&mut self.canvas, outer, placement.radius, FRAME_EDGE, false);
            let edge = (b / 4).max(2) & !1;
            let body = (
                outer.0 + edge,
                outer.1 + edge,
                outer.2 - 2 * edge,
                outer.3 - 2 * edge,
            );
            paint_rounded(
                &mut self.canvas,
                body,
                placement.radius - edge,
                FRAME_BODY,
                false,
            );
        }
        blit(&self.scaled, &mut self.canvas, x, y);
        if placement.border > 0 {
            // The screen's corners are round too.
            let inner = placement.radius - placement.border;
            paint_rounded(&mut self.canvas, (x, y, w, h), inner, FRAME_BODY, true);
        }
        self.canvas.set_pts(Some(time_ms));
        self.encoder.send_frame(&self.canvas)?;
        self.write_packets(out)
    }

    /// Flushes the encoder; the last frame lasts until `end_ms` (clip time).
    pub(crate) fn finish(
        &mut self,
        out: &mut format::context::Output,
        end_ms: i64,
    ) -> Result<(), SinkError> {
        self.encoder.send_eof()?;
        self.write_packets(out)?;
        if let Some(last) = self.pending.take() {
            let pts = last.pts().unwrap_or(0);
            self.write(out, last, (end_ms - pts).max(1))?;
        }
        Ok(())
    }

    /// Writes encoded packets one late, each with its real duration.
    fn write_packets(&mut self, out: &mut format::context::Output) -> Result<(), SinkError> {
        let mut packet = ffmpeg::Packet::empty();
        while self.encoder.receive_packet(&mut packet).is_ok() {
            if let Some(previous) = self.pending.take() {
                let duration = packet.pts().unwrap_or(0) - previous.pts().unwrap_or(0);
                self.write(out, previous, duration.max(1))?;
            }
            self.pending = Some(packet.clone());
        }
        Ok(())
    }

    fn write(
        &self,
        out: &mut format::context::Output,
        mut packet: ffmpeg::Packet,
        duration_ms: i64,
    ) -> Result<(), SinkError> {
        let out_tb = out
            .stream(self.index)
            .map(|s| s.time_base())
            .ok_or(ffmpeg::Error::StreamNotFound)?;
        packet.set_stream(self.index);
        packet.set_duration(duration_ms);
        packet.rescale_ts(ENCODER_TIME_BASE, out_tb);
        packet.write_interleaved(out)?;
        Ok(())
    }
}

/// Fills a YUV420P frame with black (limited range: Y=16, U=V=128).
pub(crate) fn paint_black(frame: &mut ffmpeg::util::frame::Video) {
    for (plane, value) in [(0, 16u8), (1, 128), (2, 128)] {
        frame.data_mut(plane).fill(value);
    }
}

/// Copies a YUV420P frame into `dst` at (`x`, `y`) (both even).
pub(crate) fn blit(
    src: &ffmpeg::util::frame::Video,
    dst: &mut ffmpeg::util::frame::Video,
    x: u32,
    y: u32,
) {
    let size = (src.width(), src.height());
    copy_region(src, (0, 0), dst, (x, y), size);
}

/// Copies a `size` region of a YUV420P frame from `from` in `src` to `to` in
/// `dst` (all even).
fn copy_region(
    src: &ffmpeg::util::frame::Video,
    from: (u32, u32),
    dst: &mut ffmpeg::util::frame::Video,
    to: (u32, u32),
    size: (u32, u32),
) {
    for plane in 0..3 {
        let shift = u32::from(plane > 0); // chroma planes are half size
        let (sx, sy) = ((from.0 >> shift) as usize, (from.1 >> shift) as usize);
        let (dx, dy) = ((to.0 >> shift) as usize, (to.1 >> shift) as usize);
        let width = (size.0 >> shift) as usize;
        let rows = (size.1 >> shift) as usize;
        let (src_stride, dst_stride) = (src.stride(plane), dst.stride(plane));
        let src_data = src.data(plane);
        let dst_data = dst.data_mut(plane);
        for row in 0..rows {
            let a = (sy + row) * src_stride + sx;
            let b = (dy + row) * dst_stride + dx;
            if a + width <= src_data.len() && b + width <= dst_data.len() {
                dst_data[b..b + width].copy_from_slice(&src_data[a..a + width]);
            }
        }
    }
}

/// Runs an export request on its own thread, reporting [`ExportState`].
pub struct ExportJob {
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ExportJob {
    pub fn start(
        request: ExportRequest,
        events: Sender<Event>,
        notify: Notifier,
    ) -> std::io::Result<Self> {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let thread = thread::Builder::new()
            .name("export".into())
            .spawn(move || {
                let state = run_export(&request, &flag, &mut |state| {
                    if events.send(Event::Export(state)).is_ok() {
                        notify();
                    }
                });
                if events.send(Event::Export(state)).is_ok() {
                    notify();
                }
            })?;
        Ok(Self {
            cancel,
            thread: Some(thread),
        })
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for ExportJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Exports every selected segment; returns the final state.
pub fn run_export(
    request: &ExportRequest,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(ExportState),
) -> ExportState {
    let segments: Vec<&Segment> = request.segments.iter().filter(|s| s.export).collect();
    if segments.is_empty() {
        return ExportState::Failed {
            problem: ExportProblem::NothingToExport,
        };
    }
    if !request.video.is_file() {
        return ExportState::Failed {
            problem: ExportProblem::CannotRead,
        };
    }
    let folder = clips_folder(&request.video);
    if let Err(err) = fs::create_dir_all(&folder) {
        tracing::error!(%err, folder = %folder.display(), "cannot create clips folder");
        return ExportState::Failed {
            problem: ExportProblem::CannotWrite,
        };
    }

    let clips = segments.len();
    let mut files = Vec::new();
    for (clip, segment) in segments.into_iter().enumerate() {
        let format = request.style.format;
        let path = unique_path(&folder, &clip_file_name(clip, segment, format));
        tracing::info!(clip, path = %path.display(), mode = ?request.mode, style = ?request.style, "exporting clip");
        let mut last_reported = -1.0f32;
        let mut progress = |fraction: f32| {
            let overall = (clip as f32 + fraction) / clips as f32;
            if overall - last_reported >= 0.01 || fraction >= 1.0 {
                last_reported = overall;
                report(ExportState::Progress {
                    clip,
                    clips,
                    fraction: overall,
                });
            }
            !cancel.load(Ordering::Relaxed)
        };
        // A social network format is always re-encoded.
        let result = match (request.mode, format) {
            (_, ExportFormat::Gif) => crate::gif::export_gif(
                &request.video,
                segment,
                &path,
                request.style.gif_size.width(),
                &mut progress,
            ),
            (ExportMode::Fast, ExportFormat::Original) => {
                export_fast(&request.video, segment, &path, &mut progress)
            }
            _ => export_precise(&request.video, segment, &path, request.style, &mut progress),
        };
        match result {
            Ok(()) => files.push(path),
            Err(err) => {
                let _ = fs::remove_file(&path);
                if matches!(err, ExportError::Cancelled) {
                    tracing::info!("export cancelled");
                    return ExportState::Cancelled;
                }
                tracing::error!(%err, clip, "export failed");
                return ExportState::Failed {
                    problem: err.problem(),
                };
            }
        }
    }
    tracing::info!(clips = files.len(), "export finished");
    ExportState::Finished { files, folder }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_names() {
        let video = Path::new("C:/Videos/PhonyCut/PhonyCut 2026-11-17 17-22-45.mkv");
        assert_eq!(
            clips_folder(video),
            Path::new("C:/Videos/PhonyCut/PhonyCut 2026-11-17 17-22-45 - clips")
        );
        let segment = Segment {
            name: "Le but !".into(),
            start_ms: 0,
            end_ms: 1000,
            export: true,
        };
        assert_eq!(
            clip_file_name(0, &segment, ExportFormat::Original),
            "01 - Le but !.mp4"
        );
        assert_eq!(
            clip_file_name(11, &segment, ExportFormat::Original),
            "12 - Le but !.mp4"
        );
        assert_eq!(
            clip_file_name(0, &segment, ExportFormat::Vertical),
            "01 - Le but ! (9x16).mp4"
        );
        assert_eq!(
            clip_file_name(0, &segment, ExportFormat::Gif),
            "01 - Le but !.gif"
        );
    }

    #[test]
    fn rotated_frames_fit_with_bars() {
        // Same size: fills the clip.
        assert_eq!(fit_inside((862, 1920), (862, 1920)), ((862, 1920), (0, 0)));
        // Landscape frame in a portrait clip: full width, centered vertically.
        let ((w, h), (x, y)) = fit_inside((1920, 862), (862, 1920));
        assert_eq!(w, 862);
        assert!((386..=388).contains(&h), "{h}");
        assert_eq!(x, 0);
        assert_eq!(y % 2, 0);
        assert!(y + h <= 1920);
    }

    #[test]
    fn phone_frame_leaves_room_and_stays_inside() {
        // Portrait phone in a 16:9 clip.
        let p = place((1080, 2400), (1920, 1080), true);
        let (x, y) = p.at;
        let (w, h) = p.size;
        assert!(p.border >= 4);
        assert!(x >= p.border && y >= p.border, "{p:?}");
        assert!(
            x + w + p.border <= 1920 && y + h + p.border <= 1080,
            "{p:?}"
        );
        assert!(p.radius > p.border);
        assert_eq!((w % 2, h % 2, x % 2, y % 2), (0, 0, 0, 0));
        // Without a frame: the usual fit.
        assert_eq!(place((1080, 2400), (1080, 1920), false).size.1, 1920);
    }

    #[test]
    fn rounded_rows() {
        let rect = (0.0, 0.0, 100.0, 50.0);
        assert_eq!(rounded_span(rect, 10.0, 25.0), Some((0.0, 100.0)));
        let (a, b) = rounded_span(rect, 10.0, 0.5).unwrap();
        assert!(a > 5.0 && b < 95.0, "{a} {b}");
        assert_eq!(rounded_span(rect, 10.0, 60.0), None);
    }

    #[test]
    fn nothing_selected_is_reported() {
        let request = ExportRequest {
            video: PathBuf::from("missing.mkv"),
            segments: vec![Segment {
                name: "a".into(),
                start_ms: 0,
                end_ms: 1000,
                export: false,
            }],
            mode: ExportMode::Fast,
            style: ExportStyle::default(),
        };
        let state = run_export(&request, &AtomicBool::new(false), &mut |_| {});
        assert_eq!(
            state,
            ExportState::Failed {
                problem: ExportProblem::NothingToExport
            }
        );
    }
}
