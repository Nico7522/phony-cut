//! Showing decoded video frames in egui.

use eframe::egui::{self, pos2, Color32, Rect, Sense, TextureHandle, TextureOptions};
use phonecut_core::api::VideoFrame;

/// Uploads `frame` into `texture` (created on first use).
pub fn upload(
    ctx: &egui::Context,
    texture: &mut Option<TextureHandle>,
    name: &str,
    frame: &VideoFrame,
) {
    let size = [frame.width as usize, frame.height as usize];
    // Frames are opaque (alpha = 255), so premultiplied == unmultiplied, and
    // this conversion is ~3x cheaper.
    let image = egui::ColorImage::from_rgba_premultiplied(size, &frame.rgba);
    match texture {
        Some(texture) => texture.set(image, TextureOptions::LINEAR),
        None => *texture = Some(ctx.load_texture(name, image, TextureOptions::LINEAR)),
    }
}

/// Largest rectangle with the texture's aspect ratio that fits in `area`,
/// centered.
pub fn fit_rect(area: Rect, texture: &TextureHandle) -> Rect {
    let [w, h] = texture.size().map(|v| v as f32);
    let scale = (area.width() / w.max(1.0))
        .min(area.height() / h.max(1.0))
        .max(0.0);
    Rect::from_center_size(area.center(), egui::vec2(w * scale, h * scale))
}

/// Draws the texture as large as possible in the available space, centered,
/// keeping its aspect ratio. Returns where it was drawn.
pub fn fit(ui: &mut egui::Ui, texture: &TextureHandle) -> Rect {
    let area = ui.available_rect_before_wrap();
    let rect = fit_rect(area, texture);
    ui.allocate_rect(area, Sense::hover());
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    rect
}
