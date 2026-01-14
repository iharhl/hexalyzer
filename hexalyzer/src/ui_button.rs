use eframe::egui;

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

#[allow(clippy::expect_used)]
pub fn tab_style_button<R>(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash,
    is_active: bool,
    fixed_width: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> (egui::Response, R) {
    // Define colors
    let (mut fill, text_color) = if is_active {
        (
            ui.visuals().widgets.active.bg_fill,
            ui.visuals().widgets.active.fg_stroke.color,
        )
    } else {
        (
            ui.visuals().widgets.noninteractive.bg_fill,
            ui.visuals().widgets.inactive.fg_stroke.color,
        )
    };

    // Create a unique ID for this tab's interaction
    let id = ui.make_persistent_id(id_source);

    // Workaround to fix the tab hovering
    let mut tab_rect = ui.max_rect();
    tab_rect.max.y += 10.0;

    let is_hovered = ui.rect_contains_pointer(tab_rect)
        && ui.interact(tab_rect, id, egui::Sense::hover()).hovered();

    // Highlight on hover if not active
    if is_hovered && !is_active {
        fill = ui.visuals().widgets.hovered.bg_fill;
    }

    // Create the frame
    let mut inner_ret = None;
    let response = egui::Frame::new()
        .fill(fill)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            if fixed_width > 0.0 {
                ui.set_width(fixed_width - 12.0); // 12.0 is the horizontal inner_margin (6*2)
            }

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // Ensure text doesn't steal focus/hover from the frame
                ui.style_mut().interaction.selectable_labels = false;

                // Set the default text color for this block
                ui.visuals_mut().override_text_color = Some(text_color);

                inner_ret = Some(add_contents(ui));
            });
        })
        .response;

    // Manual click handling for the frame area
    let sense = ui.interact(response.rect, id, egui::Sense::click());

    // Call expect on Option -> acceptable since the closure should always run.
    // Much easier to panic than trying to handle this edge case.
    (sense, inner_ret.expect("Closure should have run"))
}
