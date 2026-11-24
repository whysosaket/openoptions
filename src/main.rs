#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app;

use eframe::egui;

fn main() -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_app_id("io.github.whysosaket.openoptions")
        .with_inner_size([1080., 720.])
        .with_min_inner_size([650., 470.]);
    #[cfg(target_os = "macos")]
    let viewport = viewport
        .with_title_shown(false)
        .with_titlebar_shown(false)
        .with_fullsize_content_view(true);
    let native = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "OpenOptions",
        native,
        Box::new(|creation| Ok(Box::new(app::OpenOptions::new(&creation.egui_ctx)))),
    )
}
