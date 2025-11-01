use eframe::egui;
use super::HexViewer;


impl HexViewer {
    pub(crate) fn show_popup_if_error(&mut self, ctx: &egui::Context) {
        if let Some(_) = self.error {
            let screen_rect = ctx.screen_rect();

            // Block interaction with the app
            egui::Area::new(egui::Id::from("modal_blocker"))
                .order(egui::Order::Background)
                .fixed_pos(screen_rect.left_top())
                .show(ctx, |ui| {
                    ui.allocate_rect(screen_rect, egui::Sense::click());
                });

            // Darken the background
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("modal_bg"),
            ));
            painter.rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(150));

            // Display pop-up
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .fixed_size([300.0, 150.0]) // TODO: fix
                .title_bar(false)
                .show(ctx, |ui| {
                    ui.label(
                        "Error during intelhex parsing:\n".to_string() +
                            self.error.as_ref().unwrap()
                    );
                    ui.add_space(10.0);
                    // Close the pop-up
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
    }
}