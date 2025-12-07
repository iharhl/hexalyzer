mod byteedit;
mod hexviewer;
mod selection;
mod ui_error;
mod ui_events;
mod ui_fileinfo;
mod ui_inspector;
mod ui_jumpto;
mod ui_scrollarea;
mod ui_search;
mod ui_topbar;
mod ui_workspace;
mod utils;

use eframe::egui;
use eframe::egui::ViewportBuilder;
use hexviewer::HexViewer;
use std::time::{Duration, Instant};

pub mod colors {
    use eframe::egui::Color32;

    pub const TRANSPARENT: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 0);
    pub const LIGHT_BLUE: Color32 = Color32::from_rgba_premultiplied(33, 81, 109, 20);
    pub const GRAY_160: Color32 = Color32::from_gray(160);
    pub const GRAY_210: Color32 = Color32::from_gray(210);
    pub const SHADOW: Color32 = Color32::from_black_alpha(150);
}

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
        // Cap the FPS to 30
        let target_dt = Duration::from_secs_f64(1.0 / 60.0);
        let elapsed = Instant::now() - self.last_frame_time;

        // debug fps
        println!("fps={:.1}", 1.0 / elapsed.as_secs_f64());

        if elapsed < target_dt {
            std::thread::sleep(target_dt - elapsed);
        }
        self.last_frame_time = Instant::now();

        self.show_top_bar(ctx);
        if self.error.is_some() {
            self.show_error_popup(ctx);
        }
        self.show_central_workspace(ctx);
    }
}

// TODO for MVP:
// Verify export works OK
// Add content to help
// Verify performance acceptable (cap if needed)
// Polish up code
// Add documentation
