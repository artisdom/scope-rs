mod app;
mod serial_worker;

use app::GuiApp;

pub fn run_gui() -> Result<(), String> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Scope Monitor")
        .with_inner_size([1100.0, 720.0])
        .with_min_inner_size([700.0, 400.0]);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Scope Monitor",
        options,
        Box::new(|cc| Ok(Box::new(GuiApp::new(cc)))),
    )
    .map_err(|e| format!("GUI error: {}", e))
}
