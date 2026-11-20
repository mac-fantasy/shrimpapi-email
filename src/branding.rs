use anyhow::{Context, Result};
use eframe::egui;

const LOGO: &[u8] = include_bytes!("../assets/brand/logo.png");
const ICON: &[u8] = include_bytes!("../assets/brand/icon.png");

pub fn window_icon() -> Result<egui::IconData> {
    eframe::icon_data::from_png_bytes(ICON).context("无法读取内置应用图标")
}

pub fn logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let logo = eframe::icon_data::from_png_bytes(LOGO).expect("embedded brand logo is valid PNG");
    ctx.load_texture(
        "shrimpapi-logo",
        egui::ColorImage::from_rgba_unmultiplied(
            [logo.width as usize, logo.height as usize],
            &logo.rgba,
        ),
        egui::TextureOptions::LINEAR,
    )
}
