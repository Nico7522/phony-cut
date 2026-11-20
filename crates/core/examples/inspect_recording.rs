//! Prints what players will see in a recording, and decodes it fully.
//!
//!   cargo run -p phonycut-core --example inspect_recording -- <file.mkv>

use anyhow::Context;
use ffmpeg_next as ffmpeg;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).context("missing file")?;
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Error);

    let mut input = ffmpeg::format::input(&path)?;
    println!("format:    {}", input.format().name());
    println!("duration:  {:.3} s", input.duration() as f64 / 1e6);

    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .context("no video stream")?;
    let index = stream.index();
    let time_base = stream.time_base();
    let mut decoder = ffmpeg::codec::Context::from_parameters(stream.parameters())?
        .decoder()
        .video()?;
    println!(
        "codec:     {:?}, track size {}x{}",
        decoder.id(),
        decoder.width(),
        decoder.height()
    );

    let (mut packets, mut keys, mut frames, mut errors) = (0, 0, 0, 0);
    let (mut first, mut last) = (None, 0);
    let mut sizes: Vec<(u32, u32)> = Vec::new();
    let mut frame = ffmpeg::util::frame::Video::empty();
    let mut drain = |decoder: &mut ffmpeg::decoder::Video, frames: &mut u32| {
        while decoder.receive_frame(&mut frame).is_ok() {
            *frames += 1;
            let size = (frame.width(), frame.height());
            if sizes.last() != Some(&size) {
                sizes.push(size);
            }
        }
    };
    for (s, packet) in input.packets() {
        if s.index() != index {
            continue;
        }
        packets += 1;
        keys += u32::from(packet.is_key());
        if let Some(pts) = packet.pts() {
            first.get_or_insert(pts);
            last = pts;
        }
        if decoder.send_packet(&packet).is_err() {
            errors += 1;
        }
        drain(&mut decoder, &mut frames);
    }
    decoder.send_eof()?;
    drain(&mut decoder, &mut frames);

    let span = (last - first.unwrap_or(0)) as f64 * f64::from(time_base);
    println!("packets:   {packets} ({keys} key frames, first→last frame {span:.3} s)");
    println!("decoded:   {frames} frames, {errors} decode errors");
    println!("sizes:     {sizes:?}");
    Ok(())
}
