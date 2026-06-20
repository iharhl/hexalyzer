use crate::app::{HexSession, HexViewerApp, colors};
use crate::loader::get_last_modified;
use crate::ui_inspector::format_with_separators;
use eframe::egui;

impl HexViewerApp {
    /// Show the side panel with the file information, jump to address, search, and data inspector.
    pub(crate) fn show_side_panel(&mut self, ctx: &egui::Context) {
        if !self.side_panel_expanded {
            return;
        }

        // Clone events before mutable borrow of self for session access
        let events = self.events.clone();

        egui::SidePanel::left("left_panel")
            .exact_width(280.0)
            .show(ctx, |ui| {
                ui.add_space(3.0);

                // Get the currently active session. If none active - use the dummy one
                // to construct the UI.
                // TODO: better way than using dummy?
                let mut dummy_session = HexSession::default();
                let curr_session: &mut HexSession =
                    self.get_curr_session_mut().unwrap_or(&mut dummy_session);

                // FILE INFORMATION
                egui::CollapsingHeader::new("File Information")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);

                        let filepath = curr_session.ih.filepath.to_string_lossy().into_owned();
                        let filename = &curr_session.name;

                        egui::Grid::new("file_info_grid")
                            .num_columns(2) // two columns: label + value
                            .spacing([30.0, 4.0]) // horizontal & vertical spacing
                            .show(ui, |ui| {
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::LEFT),
                                    |ui| {
                                        ui.label("File Name");
                                    },
                                );
                                // Wrap the name + show the filepath on hover
                                let response = ui.add(
                                    egui::Label::new(filename)
                                        .wrap()
                                        .sense(egui::Sense::hover()),
                                );
                                if !filepath.is_empty() {
                                    response.on_hover_text(&filepath);
                                }
                                ui.end_row();

                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::LEFT),
                                    |ui| {
                                        ui.label("Payload Size");
                                    },
                                );
                                let size = format_with_separators(curr_session.ih.size);
                                ui.label(format!("{size} bytes"));
                                ui.end_row();
                            });

                        // Get the last modified time of the file. Changed -> display warning.
                        // Throttled at once per 2 seconds to avoid too many syscalls.
                        if !filepath.is_empty()
                            && curr_session.last_mod_check.elapsed()
                                > std::time::Duration::from_secs(2)
                        {
                            curr_session.last_mod_check = std::time::Instant::now();
                            if let Some(t) = get_last_modified(&curr_session.ih.filepath).ok()
                                && t != std::time::SystemTime::UNIX_EPOCH
                            {
                                curr_session.file_changed_on_disk = t != curr_session.last_modified;
                            }
                        }

                        if curr_session.file_changed_on_disk {
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new("File on disk has been modified!")
                                    .color(colors::WARNING)
                                    .size(12.0)
                                    .strong(),
                            )
                            .on_hover_text(
                                "This file has been modified on disk since it was opened.\n\
                                Use File > Reload to reload it from disk.",
                            );
                        }

                        ui.add_space(5.0);
                    });

                ui.add_space(3.0);

                // JUMP TO ADDRESS
                egui::CollapsingHeader::new("Jump To Address")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        curr_session.show_jumpto_contents(ui, &events);
                        ui.add_space(5.0);
                    });

                ui.add_space(3.0);

                // SEARCH
                egui::CollapsingHeader::new("Search")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        curr_session.show_search_contents(ui, &events);
                        ui.add_space(5.0);
                    });

                ui.add_space(3.0);

                // DATA INSPECTOR
                egui::CollapsingHeader::new("Data Inspector")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        curr_session.show_data_inspector_contents(ui);
                        ui.add_space(5.0);
                    });
            });
    }
}
