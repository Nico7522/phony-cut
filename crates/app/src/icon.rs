//! The PhoneCut icon, made from the logo `assets/icon/PhyCut_logo_software.svg`
//! by `scripts/make-icon.ps1` (which runs `examples/make_icon.rs`):
//! - `assets/icon/phonecut.ico`: executable (embedded) and installer;
//! - `assets/icon/phonecut-256.rgba`: raw pixels for the window icon,
//!   embedded below so no image decoding happens at startup.

/// Side of the raw window icon.
pub const WINDOW_ICON_SIZE: u32 = 256;

/// Window icon pixels (RGBA, straight alpha).
#[allow(dead_code)] // used by the app, not by examples/make_icon.rs
pub fn window_icon() -> Vec<u8> {
    include_bytes!("../../../assets/icon/phonecut-256.rgba").to_vec()
}

/// Crops `rgba` (`width` × `height`) to its visible pixels, centers them on a
/// transparent square with a small margin, and returns that square.
#[allow(dead_code)] // used by examples/make_icon.rs
pub fn square(rgba: &[u8], width: u32, height: u32) -> (Vec<u8>, u32) {
    let (w, h) = (width as usize, height as usize);
    let alpha = |x: usize, y: usize| rgba[(y * w + x) * 4 + 3];
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if alpha(x, y) > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x {
        return (rgba.to_vec(), width.max(height)); // fully transparent
    }
    let (cw, ch) = (max_x - min_x + 1, max_y - min_y + 1);
    let side = (cw.max(ch) as f32 * 1.06).ceil() as usize; // 3 % margin per side
    let (ox, oy) = ((side - cw) / 2, (side - ch) / 2);
    let mut out = vec![0u8; side * side * 4];
    for y in 0..ch {
        let from = ((min_y + y) * w + min_x) * 4;
        let to = ((oy + y) * side + ox) * 4;
        out[to..to + cw * 4].copy_from_slice(&rgba[from..from + cw * 4]);
    }
    (out, side as u32)
}

/// Puts the (square) logo on a white rounded tile with a light grey border,
/// so the black drawing stays visible on dark taskbars and backgrounds.
/// Returns the new square image and its side.
#[allow(dead_code)] // used by examples/make_icon.rs
pub fn on_tile(logo: &[u8], logo_side: u32) -> (Vec<u8>, u32) {
    // The logo fills ~80 % of the tile; the tile leaves a thin transparent
    // margin so its rounded corners are not clipped.
    let side = (logo_side as f32 / 0.80).ceil() as u32;
    let s = side as f32;
    let (half, radius, border) = (s * 0.48, s * 0.20, (s * 0.018).max(1.0));
    let center = s / 2.0;
    let mut out = vec![0u8; (side * side * 4) as usize];

    for y in 0..side {
        for x in 0..side {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = rounded_square(px - center, py - center, half, radius);
            let tile_alpha = (0.5 - d).clamp(0.0, 1.0);
            if tile_alpha <= 0.0 {
                continue;
            }
            // Border: the outer `border` pixels of the tile.
            let inner = (0.5 - (d + border)).clamp(0.0, 1.0);
            let base = mix([200, 200, 200], [255, 255, 255], inner);
            let i = ((y * side + x) * 4) as usize;
            out[i..i + 3].copy_from_slice(&base);
            out[i + 3] = (tile_alpha * 255.0).round() as u8;
        }
    }

    // Logo, centered, composited over the tile ("over" operator).
    let offset = (side - logo_side) / 2;
    for y in 0..logo_side {
        for x in 0..logo_side {
            let li = ((y * logo_side + x) * 4) as usize;
            let a = f32::from(logo[li + 3]) / 255.0;
            if a <= 0.0 {
                continue;
            }
            let oi = (((y + offset) * side + x + offset) * 4) as usize;
            let under = f32::from(out[oi + 3]) / 255.0;
            let alpha = a + under * (1.0 - a);
            for c in 0..3 {
                let value = (f32::from(logo[li + c]) * a
                    + f32::from(out[oi + c]) * under * (1.0 - a))
                    / alpha.max(f32::EPSILON);
                out[oi + c] = value.round().clamp(0.0, 255.0) as u8;
            }
            out[oi + 3] = (alpha * 255.0).round() as u8;
        }
    }
    (out, side)
}

/// Signed distance to a rounded square of half side `half` centered on 0.
fn rounded_square(x: f32, y: f32, half: f32, radius: f32) -> f32 {
    let qx = x.abs() - (half - radius);
    let qy = y.abs() - (half - radius);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [0, 1, 2].map(|i| (f32::from(a[i]) * (1.0 - t) + f32::from(b[i]) * t).round() as u8)
}

/// Downscales a square RGBA image to `size` × `size` by area averaging, in
/// premultiplied alpha (no dark fringes around transparent edges).
#[allow(dead_code)] // used by examples/make_icon.rs
pub fn resize(rgba: &[u8], side: u32, size: u32) -> Vec<u8> {
    let scale = f64::from(side) / f64::from(size);
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for oy in 0..size {
        for ox in 0..size {
            let (x0, x1) = (f64::from(ox) * scale, f64::from(ox + 1) * scale);
            let (y0, y1) = (f64::from(oy) * scale, f64::from(oy + 1) * scale);
            let mut acc = [0f64; 4];
            let mut total = 0f64;
            let mut y = y0.floor();
            while y < y1 {
                let wy = (y1.min(y + 1.0) - y0.max(y)).max(0.0);
                let mut x = x0.floor();
                while x < x1 {
                    let wx = (x1.min(x + 1.0) - x0.max(x)).max(0.0);
                    let (sx, sy) = ((x as u32).min(side - 1), (y as u32).min(side - 1));
                    let i = ((sy * side + sx) * 4) as usize;
                    let weight = wx * wy;
                    let a = f64::from(rgba[i + 3]) / 255.0;
                    for c in 0..3 {
                        acc[c] += f64::from(rgba[i + c]) * a * weight;
                    }
                    acc[3] += a * weight;
                    total += weight;
                    x += 1.0;
                }
                y += 1.0;
            }
            let alpha = if total > 0.0 { acc[3] / total } else { 0.0 };
            for value in &acc[..3] {
                let straight = if acc[3] > 0.0 { value / acc[3] } else { 0.0 };
                out.push(straight.round().clamp(0.0, 255.0) as u8);
            }
            out.push((alpha * 255.0).round().clamp(0.0, 255.0) as u8);
        }
    }
    out
}

/// A Windows `.ico` file with one 32-bit BMP image per `(size, rgba)`.
#[allow(dead_code)] // used by examples/make_icon.rs
pub fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let entries: Vec<Vec<u8>> = images
        .iter()
        .map(|(size, rgba)| bmp_entry(*size, rgba))
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for ((size, _), entry) in images.iter().zip(&entries) {
        let dim = if *size >= 256 { 0 } else { *size as u8 }; // 0 means 256
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(entry.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += entry.len() as u32;
    }
    for entry in entries {
        out.extend_from_slice(&entry);
    }
    out
}

/// BITMAPINFOHEADER + bottom-up BGRA pixels + empty AND mask.
#[allow(dead_code)] // see `ico`
fn bmp_entry(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // XOR + AND masks
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]); // no compression, sizes and palette unused
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    out.resize(out.len() + (mask_row * size) as usize, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_icon_has_the_right_size() {
        let px = window_icon();
        assert_eq!(px.len(), (WINDOW_ICON_SIZE * WINDOW_ICON_SIZE * 4) as usize);
        assert_eq!(px[3], 0, "corner transparent");
    }

    #[test]
    fn square_crops_and_centers() {
        // 4 × 2 image, one opaque pixel at (3, 1).
        let mut rgba = vec![0u8; 4 * 2 * 4];
        rgba[(4 + 3) * 4..(4 + 3) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
        let (sq, side) = square(&rgba, 4, 2);
        assert_eq!(side, 2, "1 px content + margin rounds up to 2");
        assert_eq!(sq.len(), 2 * 2 * 4);
        assert_eq!(sq.chunks(4).filter(|p| p[3] == 255).count(), 1);
    }

    #[test]
    fn tile_is_opaque_inside_and_transparent_in_corners() {
        let logo = vec![0u8; 80 * 80 * 4]; // empty logo
        let (tile, side) = on_tile(&logo, 80);
        assert_eq!(side, 100);
        let px = |x: u32, y: u32| &tile[((y * side + x) * 4) as usize..][..4];
        assert_eq!(px(0, 0)[3], 0, "rounded corner transparent");
        assert_eq!(px(50, 50), &[255, 255, 255, 255], "white center");
        assert!(px(50, 3)[0] < 255, "grey border at the edge");
    }

    #[test]
    fn resize_averages_without_dark_fringe() {
        // 2 × 2: one opaque white pixel, three transparent black ones.
        let rgba = [
            255, 255, 255, 255, 0, 0, 0, 0, //
            0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let out = resize(&rgba, 2, 1);
        assert_eq!(&out[..3], &[255, 255, 255], "color stays white");
        assert_eq!(out[3], 64, "a quarter covered");
    }

    #[test]
    fn ico_layout() {
        let data = ico(&[(16, vec![0; 16 * 16 * 4]), (256, vec![0; 256 * 256 * 4])]);
        assert_eq!(&data[0..6], &[0, 0, 1, 0, 2, 0]);
        assert_eq!(data[6], 16, "first entry 16 px");
        assert_eq!(data[22], 0, "256 px is written as 0");
        let first_offset = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);
        assert_eq!(first_offset, 6 + 2 * 16);
    }
}
