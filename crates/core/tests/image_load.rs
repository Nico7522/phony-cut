//! Tutorial images are decoded with FFmpeg: check a real PNG round trip.

use std::fs;

use ffmpeg::format::Pixel;
use ffmpeg_next as ffmpeg;

use phonycut_core::image::load_rgba;

/// Encodes a small RGB image (left half red, right half blue) as PNG bytes.
fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    ffmpeg::init().unwrap();
    let codec = ffmpeg::encoder::find_by_name("png").expect("png encoder");
    let mut ctx = ffmpeg::codec::Context::new_with_codec(codec)
        .encoder()
        .video()
        .unwrap();
    ctx.set_width(width);
    ctx.set_height(height);
    ctx.set_format(Pixel::RGB24);
    ctx.set_time_base((1, 1));
    let mut encoder = ctx.open_as(codec).unwrap();

    let mut frame = ffmpeg::util::frame::Video::new(Pixel::RGB24, width, height);
    let stride = frame.stride(0);
    for (y, row) in frame.data_mut(0).chunks_mut(stride).enumerate() {
        if y >= height as usize {
            break;
        }
        for x in 0..width as usize {
            let px = if x < width as usize / 2 {
                [255, 0, 0]
            } else {
                [0, 0, 255]
            };
            row[x * 3..x * 3 + 3].copy_from_slice(&px);
        }
    }
    frame.set_pts(Some(0));
    encoder.send_frame(&frame).unwrap();
    encoder.send_eof().unwrap();
    let mut packet = ffmpeg::Packet::empty();
    encoder.receive_packet(&mut packet).unwrap();
    packet.data().unwrap().to_vec()
}

#[test]
fn png_is_decoded_to_rgba() {
    let path = std::env::temp_dir().join(format!("phonycut-img-{}.png", std::process::id()));
    fs::write(&path, png_bytes(8, 4)).unwrap();

    let image = load_rgba(&path).unwrap();
    assert_eq!((image.width, image.height), (8, 4));
    assert_eq!(image.rgba.len(), 8 * 4 * 4);
    assert_eq!(
        &image.rgba[0..4],
        &[255, 0, 0, 255],
        "left pixel red, opaque"
    );
    let right = (8 - 1) * 4;
    assert_eq!(
        &image.rgba[right..right + 4],
        &[0, 0, 255, 255],
        "right pixel blue"
    );
    fs::remove_file(&path).unwrap();
}

#[test]
fn missing_file_is_an_error() {
    assert!(load_rgba(std::path::Path::new("does-not-exist.png")).is_err());
}
