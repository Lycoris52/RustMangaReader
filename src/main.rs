#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod font;
mod app;

use eframe::egui;
use app::MangaReader;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_maximized(true)
            .with_decorations(true),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Manga Reader for Windows - Productivity",
        options,
        Box::new(|cc| Ok(Box::new(MangaReader::new(cc)))),
    )
}