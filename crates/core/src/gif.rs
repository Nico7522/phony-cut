//! GIF export: the clip as a looping animation without sound (Discord,
//! forums, READMEs).
//!
//! FFmpeg's filters do the work: `fps` (15 frames per second), `scale`
//! (chosen width, lanczos), then `palettegen` computes one 256-colour palette
//! for the whole clip and `paletteuse` maps every frame to it with a light
//! dithering. The `gif` encoder and muxer write the file, looping forever.
//! All of them are in FFmpeg's LGPL build.

use std::path::Path;

use ffmpeg::util::mathematics::Rescale;
use ffmpeg::{codec, encoder, filter, format, media, software::scaling, Dictionary, Rational};
use ffmpeg_next as ffmpeg;

use crate::decode::init_ffmpeg;
use crate::export::{ExportError, Progress};
use crate::markers::Segment;

/// Frames per second of the animation.
pub const GIF_FPS: i32 = 15;
const MICROS: Rational = Rational(1, 1_000_000);

/// Feeds frames of any size and format to the filter graph at the size of
/// the first one (a rotation inside the clip is fitted with black bars).
struct Normalizer {
    size: (u32, u32),
    scaler: Option<((format::Pixel, u32, u32), scaling::Context)>,
    scaled: ffmpeg::util::frame::Video,
    canvas: ffmpeg::util::frame::Video,
}

impl Normalizer {
    fn new(size: (u32, u32)) -> Self {
        Self {
            size,
            scaler: None,
            scaled: ffmpeg::util::frame::Video::empty(),
            canvas: ffmpeg::util::frame::Video::new(format::Pixel::YUV420P, size.0, size.1),
        }
    }

    fn frame(
        &mut self,
        frame: &ffmpeg::util::frame::Video,
    ) -> Result<&ffmpeg::util::frame::Video, ffmpeg::Error> {
        let key = (frame.format(), frame.width(), frame.height());
        let ((w, h), (x, y)) = crate::export::fit_inside((key.1, key.2), self.size);
        let scaler = match &mut self.scaler {
            Some((current, scaler)) if *current == key => scaler,
            slot => {
                let scaler = scaling::Context::get(
                    key.0,
                    key.1,
                    key.2,
                    format::Pixel::YUV420P,
                    w,
                    h,
                    scaling::Flags::BILINEAR,
                )?;
                self.scaled = ffmpeg::util::frame::Video::empty();
                &mut slot.insert((key, scaler)).1
            }
        };
        scaler.run(frame, &mut self.scaled)?;
        crate::export::paint_black(&mut self.canvas);
        crate::export::blit(&self.scaled, &mut self.canvas, x, y);
        Ok(&self.canvas)
    }
}

/// `fps → scale → split → palettegen / paletteuse`, from YUV420P frames of
/// `size` in `time_base` to PAL8 frames.
fn palette_graph(
    size: (u32, u32),
    time_base: Rational,
    width: u32,
) -> Result<filter::Graph, ffmpeg::Error> {
    let mut graph = filter::Graph::new();
    let args = format!(
        "video_size={}x{}:pix_fmt=yuv420p:time_base={}/{}:pixel_aspect=1/1",
        size.0,
        size.1,
        time_base.numerator(),
        time_base.denominator().max(1),
    );
    let buffer = filter::find("buffer").ok_or(ffmpeg::Error::FilterNotFound)?;
    let sink = filter::find("buffersink").ok_or(ffmpeg::Error::FilterNotFound)?;
    graph.add(&buffer, "in", &args)?;
    graph.add(&sink, "out", "")?;
    if let Some(mut out) = graph.get("out") {
        out.set_pixel_format(format::Pixel::PAL8);
    }
    // Never wider than the source (no blurry upscale), even width.
    let width = (width.min(size.0) & !1).max(2);
    let spec = format!(
        "fps={GIF_FPS},scale={width}:-2:flags=lanczos,split[a][b];\
         [a]palettegen=stats_mode=diff[p];\
         [b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle"
    );
    graph.output("in", 0)?.input("out", 0)?.parse(&spec)?;
    graph.validate()?;
    Ok(graph)
}

/// Writes the GIF frames coming out of the graph.
struct GifWriter {
    encoder: Option<encoder::Video>,
    time_base: Rational,
    index: usize,
    header_written: bool,
}

impl GifWriter {
    fn pull(
        &mut self,
        graph: &mut filter::Graph,
        out: &mut format::context::Output,
    ) -> Result<(), ExportError> {
        let mut frame = ffmpeg::util::frame::Video::empty();
        loop {
            let got = match graph.get("out") {
                Some(mut sink) => sink.sink().frame(&mut frame),
                None => return Err(ExportError::Write(ffmpeg::Error::FilterNotFound)),
            };
            match got {
                Ok(()) => {}
                Err(ffmpeg::Error::Other {
                    errno: ffmpeg::util::error::EAGAIN,
                })
                | Err(ffmpeg::Error::Eof) => return Ok(()),
                Err(err) => return Err(ExportError::Write(err)),
            }
            if self.encoder.is_none() {
                self.open(out, &frame)?;
            }
            if let Some(encoder) = &mut self.encoder {
                encoder.send_frame(&frame).map_err(ExportError::Write)?;
            }
            self.write_packets(out)?;
        }
    }

    /// The encoder needs the palette frames' size: opened on the first one.
    fn open(
        &mut self,
        out: &mut format::context::Output,
        frame: &ffmpeg::util::frame::Video,
    ) -> Result<(), ExportError> {
        let codec = encoder::find(codec::Id::GIF).ok_or(ExportError::NoEncoder)?;
        let mut ctx = codec::Context::new_with_codec(codec)
            .encoder()
            .video()
            .map_err(ExportError::Write)?;
        ctx.set_width(frame.width());
        ctx.set_height(frame.height());
        ctx.set_format(format::Pixel::PAL8);
        ctx.set_time_base(self.time_base);
        let encoder = ctx.open_as(codec).map_err(ExportError::Write)?;
        let mut stream = out.add_stream(codec).map_err(ExportError::Write)?;
        stream.set_parameters(&encoder);
        stream.set_time_base(self.time_base);
        self.index = stream.index();
        let mut options = Dictionary::new();
        options.set("loop", "0"); // forever
        out.write_header_with(options).map_err(ExportError::Write)?;
        self.header_written = true;
        self.encoder = Some(encoder);
        Ok(())
    }

    fn write_packets(&mut self, out: &mut format::context::Output) -> Result<(), ExportError> {
        let Some(encoder) = &mut self.encoder else {
            return Ok(());
        };
        let out_tb = out
            .stream(self.index)
            .map(|s| s.time_base())
            .ok_or(ExportError::Write(ffmpeg::Error::StreamNotFound))?;
        let mut packet = ffmpeg::Packet::empty();
        while encoder.receive_packet(&mut packet).is_ok() {
            packet.set_stream(self.index);
            packet.set_duration(1);
            packet.rescale_ts(self.time_base, out_tb);
            packet.write_interleaved(out).map_err(ExportError::Write)?;
        }
        Ok(())
    }

    fn finish(&mut self, out: &mut format::context::Output) -> Result<(), ExportError> {
        if let Some(encoder) = &mut self.encoder {
            encoder.send_eof().map_err(ExportError::Write)?;
        }
        self.write_packets(out)?;
        if !self.header_written {
            return Err(ExportError::Read(ffmpeg::Error::InvalidData));
        }
        out.write_trailer().map_err(ExportError::Write)
    }
}

/// Exports one segment as a GIF of `width` pixels (at most the source's).
pub fn export_gif(
    video: &Path,
    segment: &Segment,
    output: &Path,
    width: u32,
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
    let ms = |v: u64| i64::try_from(v).unwrap_or(i64::MAX / 1000) * 1000;
    let (start_us, end_us) = (ms(segment.start_ms), ms(segment.end_ms));
    input
        .seek(start_us, ..start_us)
        .map_err(ExportError::Read)?;

    let mut out = format::output_as(output, "gif").map_err(ExportError::Write)?;
    let mut writer = GifWriter {
        encoder: None,
        time_base: Rational(1, GIF_FPS),
        index: 0,
        header_written: false,
    };
    let mut graph: Option<(filter::Graph, Normalizer)> = None;
    let mut decoded = ffmpeg::util::frame::Video::empty();
    let mut packet = ffmpeg::Packet::empty();
    let span = (end_us - start_us).max(1) as f32;
    let mut finished = false;
    while !finished {
        match packet.read(&mut input) {
            Ok(()) => {
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
                finished = true;
            }
            Err(err) => return Err(ExportError::Read(err)),
        }
        while decoder.receive_frame(&mut decoded).is_ok() {
            let pts = decoded.timestamp().or(decoded.pts()).unwrap_or(0);
            let time_us = pts.rescale(in_tb, MICROS);
            if time_us < start_us {
                continue; // between the key frame and the in point
            }
            if time_us >= end_us {
                finished = true;
                break;
            }
            // Analysing the whole clip comes first: the palette is computed
            // at the end, so reading counts for 90 %.
            if !progress(((time_us - start_us) as f32 / span * 0.9).min(0.9)) {
                return Err(ExportError::Cancelled);
            }
            let (graph, normalizer) = match &mut graph {
                Some(g) => g,
                slot => {
                    let size = (decoded.width() & !1, decoded.height() & !1);
                    let g = palette_graph(size, MICROS, width).map_err(ExportError::Write)?;
                    &mut *slot.insert((g, Normalizer::new(size)))
                }
            };
            let mut frame = normalizer
                .frame(&decoded)
                .map_err(ExportError::Write)?
                .clone();
            frame.set_pts(Some(time_us - start_us));
            graph
                .get("in")
                .ok_or(ExportError::Write(ffmpeg::Error::FilterNotFound))?
                .source()
                .add(&frame)
                .map_err(ExportError::Write)?;
            writer.pull(graph, &mut out)?;
        }
    }
    let Some((mut graph, _)) = graph else {
        return Err(ExportError::Read(ffmpeg::Error::InvalidData));
    };
    graph
        .get("in")
        .ok_or(ExportError::Write(ffmpeg::Error::FilterNotFound))?
        .source()
        .flush()
        .map_err(ExportError::Write)?;
    writer.pull(&mut graph, &mut out)?;
    writer.finish(&mut out)?;
    progress(1.0);
    Ok(())
}
