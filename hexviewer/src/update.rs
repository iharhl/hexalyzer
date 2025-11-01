use eframe;
use eframe::egui;
use super::HexViewer;

//
// IMPROVEMENTS:
// 1) Render one widget per line:
//    a) one for byte and one for ascii
//    b) render selection using ui.interact
// 2) Optimize how we store the hex data in the HexViewer
//    a) store in HashMap more optimal?
//
// ADDITIONAL FEATURES:
// 1) Multi-byte select
//


impl eframe::App for HexViewer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show_top_bar(ctx);
        self.show_popup_if_error(ctx);
        self.show_central_workspace(ctx);
    }
}




