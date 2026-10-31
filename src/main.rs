#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Bladr - a small, fast photo viewer built around one idea: the mouse wheel
//! flips through the images in the current folder, and fullscreen is one key
//! away.

mod app;
mod decode;
mod library;
mod tiff;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let mut initial: Option<PathBuf> = None;
    let mut start_fullscreen = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--fullscreen" | "-f" => start_fullscreen = true,
            "--help" | "-h" => {
                println!("Bladr - billedfremviser");
                println!();
                println!("Brug: bladr [mappe eller billede] [--fullscreen]");
                return Ok(());
            }
            other if other.starts_with('-') => {}
            other => initial = Some(PathBuf::from(other)),
        }
    }

    if let Some(path) = &initial {
        if !path.exists() {
            eprintln!("Findes ikke: {}", path.display());
            initial = None;
        }
    }

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).ok();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Bladr")
        .with_min_inner_size([560.0, 400.0])
        .with_maximized(true)
        .with_app_id("bladr");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        vsync: true,
        ..Default::default()
    };

    eframe::run_native(
        "Bladr",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::BladrApp::new(
                cc,
                initial.take(),
                start_fullscreen,
            )))
        }),
    )
}


