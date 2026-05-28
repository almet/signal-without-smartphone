use eframe::egui;
use signal_setup_core as core;

mod app;
mod backend;
mod panels;
mod qr;
mod theme;
mod widgets;

use app::SignalSetupApp;
use backend::{mode, Mode, MODE};

/// Decode the embedded PNG into the RGBA buffer eframe wants for the window
/// icon. Returns an empty icon if decoding fails, so a bad icon never blocks
/// startup.
fn load_window_icon() -> egui::IconData {
    const PNG: &[u8] = include_bytes!("../assets/logo.png");
    match image::load_from_memory(PNG) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            }
        }
        Err(e) => {
            eprintln!("Could not decode embedded window icon: {e}");
            egui::IconData::default()
        }
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--demo") {
        MODE.set(Mode::Demo).ok();
    } else if args.iter().any(|a| a == "--staging") {
        MODE.set(Mode::Staging).ok();
        core::enable_staging();
    }

    let title = match mode() {
        Mode::Demo => "Signal Setup Tool [DEMO]",
        Mode::Staging => "Signal Setup Tool [STAGING]",
        Mode::Production => "Signal Setup Tool",
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([580.0, 600.0])
            .with_resizable(true)
            .with_icon(load_window_icon()),
        ..Default::default()
    };

    eframe::run_native(
        title,
        options,
        Box::new(|cc| Ok(Box::new(SignalSetupApp::new(cc)))),
    )
}
