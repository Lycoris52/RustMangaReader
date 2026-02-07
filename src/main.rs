#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod font;
mod app;
mod utils;

use app::MangaReader;

fn main() -> eframe::Result<()> {
    let icon = image::open("src/assets/icon256.png")
        .expect("Failed to open icon")
        .to_rgba8();
    let (width, height) = icon.dimensions();

    // args[1] is the file path.
    let args: Vec<String> = std::env::args().collect();
    let initial_path = args.get(1).map(std::path::PathBuf::from);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_maximized(true)
            .with_decorations(true)
            .with_icon(std::sync::Arc::new(egui::IconData {
            rgba: icon.into_raw(),
            width,
            height,
        })),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Manga Reader for Windows - Productivity",
        native_options,
        Box::new(|cc| {
            Ok(Box::new(MangaReader::new(cc, initial_path)))
        }),
    )
}