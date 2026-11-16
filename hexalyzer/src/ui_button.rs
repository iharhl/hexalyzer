use eframe::egui;

/// Size of a byte button in the hex view
pub const BYTE_BUTTON_SIZE: egui::Vec2 = egui::Vec2::new(21.0, 18.0);

pub fn light_mono_button(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    text: &str,
    text_color: egui::Color32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);

        // Background (transparent; hover/click feedback works)
        if response.hovered() || response.clicked() {
            ui.painter().rect(
                rect,
                0.0,
                visuals.bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
        }

        // Text (monospace, fixed size)
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::monospace(12.0),
            text_color,
        );
    }

    response
}
