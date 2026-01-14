use crate::app::HexViewerApp;
use crate::ui_button;
use eframe::egui;

impl HexViewerApp {
    #[allow(clippy::cast_precision_loss)]
    /// Calculate the dynamic width for each tab based on the available space.
    /// Returns a vector of widths for each tab.
    /// TODO: improve, right now it is hacky and not precise
    fn get_dynamic_width_for_each_tab(&self, ui: &egui::Ui, spacing: f32) -> Vec<f32> {
        // Calculate ideal widths for all tabs
        let mut ideal_widths = Vec::with_capacity(self.sessions.len() + 1);
        let font_id = egui::TextStyle::Body.resolve(ui.style());

        // Estimate width of each tab: name width + padding + close button space
        for session in &self.sessions {
            let galley = ui.painter().layout_no_wrap(
                session.name.clone(),
                font_id.clone(),
                ui.visuals().widgets.active.text_color(),
            );
            let text_width = galley.size().x;
            let ideal_w = text_width + 32.0; // 32px for margins and "×" button
            ideal_widths.push(ideal_w);
        }

        // Determine scaling
        let add_button_width = if self.sessions.len() < self.max_tabs {
            70.0
        } else {
            35.0 // still leave some margin
        };

        // Total space taken by the gaps between tabs
        let total_spacing = spacing * (self.sessions.len() as f32);

        // Get width available for tabs
        let available_width = ui.available_width() - add_button_width - total_spacing;

        // Get ideal width for all tabs
        let total_ideal_width: f32 = ideal_widths.iter().sum();

        // Only scale down if we actually exceed the available space
        // Scale to the closest .05 downwards min some margin
        let scale_factor = if total_ideal_width > available_width {
            (available_width / total_ideal_width * 30.0).floor() / 30.0 - 0.03
        } else {
            1.0
        };

        ideal_widths.iter().map(|w| w * scale_factor).collect()
    }

    /// Show tabs with the list of open files.
    /// Tabs are constrained to fit into the available space.
    /// If the number of tabs does not exceed the maximum allowed, the "Open New File" tab is added.
    pub(crate) fn show_tabs(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("tabs_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mut tab_to_close = None;

                // Modify spacing between tabs
                let spacing = 2.0;
                ui.spacing_mut().item_spacing.x = spacing;

                let dynamic_width = self.get_dynamic_width_for_each_tab(ui, spacing);

                for (i, session) in self.sessions.iter().enumerate() {
                    let is_active = Some(i) == self.active_index;

                    // Create a tab
                    let (response, close_clicked) = ui_button::tab_style_button(
                        ui,
                        ("tab", i),
                        is_active,
                        dynamic_width[i],
                        |ui| {
                            // Truncate the name if it is too long for the calculated width
                            let name = egui::RichText::new(&session.name);
                            ui.add(egui::Label::new(name).truncate());

                            // Close button (with a transparent background)
                            ui.scope(|ui| {
                                ui.visuals_mut().widgets.inactive.weak_bg_fill =
                                    egui::Color32::TRANSPARENT;
                                ui.button("×").clicked()
                            })
                            .inner
                        },
                    );

                    if close_clicked {
                        tab_to_close = Some(i);
                    } else if response.clicked() {
                        self.active_index = Some(i);
                    }
                }

                // Handle closing tabs after the loop to avoid borrow checker issues
                if let Some(i) = tab_to_close {
                    self.close_file(i);
                }

                // "Open New File" tab button
                if self.sessions.len() < self.max_tabs {
                    let (response, ()) =
                        ui_button::tab_style_button(ui, "add_tab", false, 0.0, |ui| {
                            ui.label(egui::RichText::new(" + ").strong());
                        });
                    if response.on_hover_text("Open New File").clicked()
                        && let Some(path) =
                            rfd::FileDialog::new().set_title("Open File").pick_file()
                    {
                        self.load_file(&path);
                    }
                }
            });
        });
    }
}
