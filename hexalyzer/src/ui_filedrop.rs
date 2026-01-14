use crate::app::HexViewerApp;
use eframe::egui;

impl HexViewerApp {
    /// Handle drag and drop events:
    /// - If a file is dropped, load it into the app.
    /// - If a file is dragged over the central panel, display a message.
    /// - If the popup is shown, do not handle drag and drop events.
    pub(crate) fn handle_drag_and_drop(&mut self, ctx: &egui::Context) {
        // Return if the popup is shown
        // TODO: also consider async file dialog (for the future) as the app panics if file is dragged when the dialog window is open
        if self.popup.active {
            return;
        }

        // Overwrite the central panel with a message when file is dragged over
        let hovering_files = ctx.input(|i| i.raw.hovered_files.clone());
        if !hovering_files.is_empty() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.heading("Drop file to open");
                });
            });
        }

        // Load file if dropped
        if ctx.input(|i| !i.raw.dropped_files.is_empty()) {
            for file in ctx.input(|i| i.raw.dropped_files.clone()) {
                if let Some(path) = file.path {
                    self.load_file(&path);
                }
            }
        }
    }
}
