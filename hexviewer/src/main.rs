mod hexviewer;
mod update;
mod error;
mod topbar;
mod workspace;

use eframe::egui::{ViewportBuilder};
use hexviewer::HexViewer;


fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_resizable(true)
            .with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Hexalyzer",
        options,
        Box::new(|_cc| Ok(Box::new(HexViewer::default())))
    )
}