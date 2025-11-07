mod error;
mod hexviewer;
mod selection;
mod topbar;
mod workspace;

use eframe::egui;
use eframe::egui::ViewportBuilder;
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
        Box::new(|_cc| Ok(Box::new(HexViewer::default()))),
    )
}

impl eframe::App for HexViewer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show_top_bar(ctx);
        if self.error.is_some() {
            self.show_error_popup(ctx);
        }
        self.show_central_workspace(ctx);
    }
}


// TODO for MVP:
// 1. Hex bytes editing
// 2. Search feature
// 3. Floats nad utf-8 support in data inspector
// 4. Add line with failure in error popup
// 5. Verify export works OK
// 6. Add content to help
// 7. Verify performance acceptable (cap if needed)
