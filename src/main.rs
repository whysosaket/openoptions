#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app;

use eframe::egui;

fn main() -> eframe::Result {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("io.github.whysosaket.openoptions")
            .with_inner_size([1080., 720.])
            .with_min_inner_size([650., 470.]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "OpenOptions",
        native,
        Box::new(|creation| Ok(Box::new(app::OpenOptions::new(&creation.egui_ctx)))),
    )
}
