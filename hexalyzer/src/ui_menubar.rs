use crate::HexViewerApp;
use crate::app::HexSession;
use crate::ui_popup::PopupState;
use eframe::egui;

pub enum SaveFormat {
    Bin,
    Hex,
}

pub fn format_from_extension(path: &std::path::Path) -> Option<SaveFormat> {
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
                    self.file_menu(ui);
                    self.edit_menu(ui);
                    self.view_menu(ui);
                    self.tools_menu(ui);
                    self.about_button(ui);
                });
            });

            ui.add_space(2.0);
        });
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            // OPEN BUTTON
            if ui.button("Open file...").clicked()
                && let Some(path) = rfd::FileDialog::new().set_title("Open File").pick_file()
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

                let res = match format {
                    SaveFormat::Bin => curr_session.ih.write_bin(path, 0x00),
                    SaveFormat::Hex => curr_session.ih.write_hex(path),
                };
                if let Err(msg) = res {
                    self.error = Some(msg.to_string());
                }
            }

            // RELOAD BUTTON
            let has_file = self
                .get_curr_session()
                .is_some_and(|s| !s.ih.filepath.as_os_str().is_empty());

            if ui
                .add_enabled(has_file, egui::Button::new("Reload"))
                .clicked()
                && let Some(session_id) = self.active_index
            {
                if self.has_unsaved_changes(session_id) {
                    self.popup.open(PopupState::CloseConfirm {
                        session_id,
                        reload_after: true,
                    });
                } else {
                    self.reload_file(session_id);
                }
            }

            // CLOSE BUTTON
            if ui.button("Close file").clicked()
                && let Some(session_id) = self.active_index
                && self.get_curr_session().is_some()
            {
                if self.has_unsaved_changes(session_id) {
                    self.popup.open(PopupState::CloseConfirm {
                        session_id,
                        reload_after: false,
                    });
                } else {
                    self.close_file(session_id);
                }
            }
        });
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Edit", |ui| {
            self.edit_popup_items(ui);
            ui.separator();
            self.edit_copy_items(ui);
        });
    }

    fn edit_popup_items(&mut self, ui: &mut egui::Ui) {
        // RELOCATE BUTTON
        if ui.button("Relocate...").clicked()
            && !self.popup.active
            && let Some(curr_session) = self.get_curr_session()
            && curr_session.ih.size != 0
        {
            self.popup.open(PopupState::ReAddr {
                addr: String::new(),
            });
        }

        // MERGE BUTTON
        if ui.button("Merge...").clicked()
            && !self.popup.active
            && let Some(curr_session) = self.get_curr_session()
            && curr_session.ih.size != 0
            && let Some(path) = rfd::FileDialog::new()
                .set_title("Merge with File")
                .pick_file()
        {
            self.popup.open(PopupState::Merge {
                path,
                addr_curr: String::new(),
                addr_merge: String::new(),
            });
        }

        // INSERT RANGE BUTTON
        if ui.button("Insert Range...").clicked()
            && !self.popup.active
            && let Some(curr_session) = self.get_curr_session()
            && curr_session.ih.size != 0
        {
            self.popup.open(PopupState::InsertRange {
                start: String::new(),
                end: String::new(),
            });
        }

        // RESTORE BUTTON
        if ui.button("Restore byte changes").clicked()
            && let Some(curr_session) = self.get_curr_session_mut()
            && curr_session.ih.size != 0
        {
            curr_session.restore();
        }
    }

    fn edit_copy_items(&self, ui: &mut egui::Ui) {
        let has_selection = self
            .get_curr_session()
            .is_some_and(|s| s.selection.range.is_some());

        let hex_shortcut = if cfg!(target_os = "macos") {
            "Cmd+C"
        } else {
            "Ctrl+C"
        };
        let ascii_shortcut = if cfg!(target_os = "macos") {
            "Shift+Cmd+C"
        } else {
            "Ctrl+Shift+C"
        };
        let addr_shortcut = if cfg!(target_os = "macos") {
            "Opt+C"
        } else {
            "Alt+C"
        };

        self.copy_button(ui, has_selection, "Copy as Hex", hex_shortcut, |s| {
            s.selected_bytes_as_hex()
        });
        self.copy_button(ui, has_selection, "Copy as ASCII", ascii_shortcut, |s| {
            s.selected_bytes_as_ascii()
        });
        self.copy_button(ui, has_selection, "Copy Address", addr_shortcut, |s| {
            s.selected_addr()
        });
    }

    fn copy_button(
        &self,
        ui: &mut egui::Ui,
        enabled: bool,
        label: &str,
        shortcut: &str,
        get_text: impl Fn(&HexSession) -> Option<String>,
    ) {
        if ui
            .add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut))
            .clicked()
            && let Some(s) = self.get_curr_session()
            && let Some(text) = get_text(s)
        {
            ui.ctx().copy_text(text);
        }
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("View", |ui| {
            ui.label("Select Bytes per Row:");
            ui.add_space(3.0);
            ui.radio_value(&mut self.bytes_per_row, 16, "16 bytes");
            ui.add_space(1.0);
            ui.radio_value(&mut self.bytes_per_row, 32, "32 bytes");
        });
    }

    fn tools_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Tools", |ui| {
            if ui.button("Hex Converter").clicked() {
                self.converter.active = true;
            }
        });
    }

    fn about_button(&mut self, ui: &mut egui::Ui) {
        let about_button = ui.button("About");

        if about_button.clicked() && !self.popup.active {
            self.popup.open(PopupState::About);
        }
    }
}
