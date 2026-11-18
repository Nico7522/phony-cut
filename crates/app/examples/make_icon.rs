//! Builds the icon files from a PNG rendering of the logo:
//!   cargo run -p app --example make_icon -- <logo.png>
//! Normally run through `scripts/make-icon.ps1`, which renders the SVG first.

#[path = "../src/icon.rs"]
mod icon;

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let png = std::env::args()
        .nth(1)
        .ok_or("usage: make_icon <logo.png>")?;
    let logo = phonecut_core::image::load_rgba(Path::new(&png))?;
    let (logo, logo_side) = icon::square(&logo.rgba, logo.width, logo.height);
    // White rounded tile behind the black logo: readable on dark backgrounds.
    let (square, side) = icon::on_tile(&logo, logo_side);

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon");
    std::fs::create_dir_all(&dir)?;

    let sizes = [16, 20, 24, 32, 40, 48, 64, 256];
    let images: Vec<(u32, Vec<u8>)> = sizes
        .iter()
        .map(|&s| (s, icon::resize(&square, side, s)))
        .collect();
    std::fs::write(dir.join("phonecut.ico"), icon::ico(&images))?;

    let window = icon::resize(&square, side, icon::WINDOW_ICON_SIZE);
    std::fs::write(dir.join("phonecut-256.rgba"), window)?;
    println!("icons written to {} (source {side}x{side})", dir.display());
    Ok(())
}
