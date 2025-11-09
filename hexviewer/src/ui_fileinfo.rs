use eframe::egui;
use eframe::egui::Ui;
use crate::hexviewer::HexViewer;

impl HexViewer {
    pub(crate) fn show_file_info_contents(&mut self, ui: &mut Ui) {
        // Get filename
        let filename = self
            .ih
            .filepath
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "-".to_string());

        egui::Grid::new("file_info_grid")
            .num_columns(2) // two columns: label + value
            .spacing([30.0, 4.0]) // horizontal & vertical spacing
            .show(ui, |ui| {
                ui.label("File Name");
                ui.label(filename);
                ui.end_row();
                ui.label("File Size");
                ui.label(format!("{} bytes", self.ih.size));
                ui.end_row();
            });
    }
}