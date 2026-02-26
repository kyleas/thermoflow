#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::collapsible_if)]

mod app;
mod project_io;
mod run_worker;
mod transient_model;
mod views;

use app::ThermoflowApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Thermoflow"),
        ..Default::default()
    };

    eframe::run_native(
        "Thermoflow",
        options,
        Box::new(|cc| Ok(Box::new(ThermoflowApp::new(cc)))),
    )
}
