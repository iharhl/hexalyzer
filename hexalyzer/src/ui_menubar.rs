use crate::HexViewerApp;
use crate::ui_popup::PopupType;
use eframe::egui;
use std::error::Error;

enum SaveFormat {
    Bin,
    Hex,
}

fn format_from_extension(path: &std::path::Path) -> Option<SaveFormat> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)?
        .as_str()
    {
        "bin" => Some(SaveFormat::Bin),
        "hex" => Some(SaveFormat::Hex),
        _ => None,
    }
}

impl HexViewerApp {
    /// Displays the top menu bar with File, Edit, View, and About buttons
    pub(crate) fn show_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            ui.add_space(3.0);

            egui::MenuBar::new().ui(ui, |ui| {
                ui.horizontal(|ui| {
                    // FILE MENU
                    ui.menu_button("File", |ui| {
                        // OPEN BUTTON
                        if ui.button("Open file...").clicked()
                            && let Some(path) =
                                rfd::FileDialog::new().set_title("Open File").pick_file()
                        {
                            self.load_file(&path);
                        }

                        // EXPORT BUTTON
                        if ui.button("Export file...").clicked()
                            && let Some(curr_session) = self.get_curr_session_mut()
                            && curr_session.ih.size != 0
                            && let Some(mut path) = rfd::FileDialog::new()
                                .set_title("Save As")
                                .set_file_name(curr_session.name.clone())
                                .save_file()
                        {
                            if path.extension().is_none() {
                                path.set_extension("bin");
                            }

                            let format = format_from_extension(&path).unwrap_or(SaveFormat::Bin);

                            let res: Result<(), Box<dyn Error>> = match format {
                                SaveFormat::Bin => curr_session.ih.write_bin(path, 0x00),
                                SaveFormat::Hex => curr_session.ih.write_hex(path),
                            };
                            if let Err(msg) = res {
                                self.error.borrow_mut().replace(msg.to_string());
                            }
                        }

                        // CLOSE BUTTON
                        if ui.button("Close file").clicked()
                            && let Some(curr_session_id) = self.active_index
                            && let Some(_) = self.get_curr_session()
                        {
                            self.close_file(curr_session_id);
                        }
                    });

                    // EDIT BUTTON
                    ui.menu_button("Edit", |ui| {
                        // READDRESS BUTTON
                        if ui.button("Relocate...").clicked()
                            && let Some(curr_session) = self.get_curr_session()
                            && curr_session.ih.size != 0
                        {
                            self.popup.active = true;
                            self.popup.ptype = Some(PopupType::ReAddr);
                        }

                        // RESTORE BUTTON
                        if ui.button("Restore byte changes").clicked()
                            && let Some(curr_session) = self.get_curr_session_mut()
                            && curr_session.ih.size != 0
                        {
                            curr_session.restore();
                        }
                    });

                    // VIEW BUTTON
                    ui.menu_button("View", |ui| {
                        ui.label("Select Bytes per Row:");

                        ui.add_space(3.0);

                        // RadioButtons to select between 16 and 32 bytes per row
                        ui.radio_value(&mut self.bytes_per_row, 16, "16 bytes");
                        ui.add_space(1.0);
                        ui.radio_value(&mut self.bytes_per_row, 32, "32 bytes");
                    });

                    // ABOUT BUTTON
                    let about_button = ui.button("About");

                    if about_button.clicked() {
                        self.popup.active = true;
                        self.popup.ptype = Some(PopupType::About);
                    }
                });
            });

            ui.add_space(2.0);
        });
    }
}
