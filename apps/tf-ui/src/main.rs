#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::collapsible_if)]

mod app;
mod pid_editor;
mod plot_workspace;
mod project_io;
mod run_worker;
mod transient_model;
mod views;

use app::ThermoflowApp;
use std::env;

fn main() -> eframe::Result<()> {
    // Check for GUI smoke-test mode (non-interactive verification)
    let gui_smoke_test = env::args().any(|arg| arg == "--gui-smoke-test");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Thermoflow"),
        ..Default::default()
    };

    eframe::run_native(
        "Thermoflow",
        options,
        Box::new(move |cc| {
            let mut app = ThermoflowApp::new(cc);
            app.set_smoke_test_mode(gui_smoke_test);
            Ok(Box::new(app))
        }),
    )
}
