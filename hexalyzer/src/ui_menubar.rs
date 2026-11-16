use crate::HexViewerApp;
use crate::app::HexSession;
use crate::hex_input::parse_hex_addr;
use crate::ui_popup::PopupState;
use eframe::egui;

impl HexViewerApp {
    /// Displays the top menu bar with File, Edit, View, and About buttons
    pub(crate) fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menubar").show(ui, |ui| {
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
        let has_file = self.get_curr_session().is_some();

        ui.menu_button("File", |ui| {
            // OPEN BUTTON
            if ui.button("Open file...").clicked()
                && let Some(path) = rfd::FileDialog::new().set_title("Open File").pick_file()
            {
                self.load_file(&path);
            }

            let has_filepath = self
                .get_curr_session()
                .is_some_and(|s| !s.ih.filepath.as_os_str().is_empty());
            let has_changes = self
                .active_index
                .is_some_and(|id| self.has_unsaved_changes(id));

            // SAVE BUTTON
            let save_shortcut = if cfg!(target_os = "macos") {
                "Cmd+S"
            } else {
                "Ctrl+S"
            };
            if ui
                .add_enabled(
                    has_filepath && has_changes,
                    egui::Button::new("Save").shortcut_text(save_shortcut),
                )
                .clicked()
            {
                self.save_curr_session();
            }

            // EXPORT BUTTON
            if ui
                .add_enabled(has_file, egui::Button::new("Export..."))
                .clicked()
            {
                self.export_curr_session();
            }

            // GAP FILL MENU
            self.gap_fill_menu(ui);

            // RELOAD BUTTON
            if ui
                .add_enabled(has_filepath, egui::Button::new("Reload"))
                .clicked()
                && let Some(index) = self.active_index
            {
                if self.has_unsaved_changes(index) {
                    self.open_close_confirm(index, true);
                } else {
                    self.reload_file(index);
                }
            }

            // CLOSE BUTTON
            if ui
                .add_enabled(has_file, egui::Button::new("Close file"))
                .clicked()
                && let Some(index) = self.active_index
            {
                if self.has_unsaved_changes(index) {
                    self.open_close_confirm(index, false);
                } else {
                    self.close_file(index);
                }
            }
        });
    }

    /// Submenu with a hex field for the gap fill byte
    fn gap_fill_menu(&mut self, ui: &mut egui::Ui) {
        // Keep the submenu open on clicks inside it, so the field can be edited
        let config = egui::containers::menu::MenuConfig::new()
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);

        // Avoid wrapping onto next line due to the label width changing with the new value
        let button = egui::Button::new(format!("Gap Fill: 0x{:02X}", self.gap_fill))
            .wrap_mode(egui::TextWrapMode::Extend)
            .right_text(egui::containers::menu::SubMenuButton::arrow_atom(None));

        egui::containers::menu::SubMenuButton::from_button(button)
            .config(config)
            .ui(ui, |ui| {
                ui.label("Fill byte for gaps when\nexporting to BIN");
                ui.separator();

                // Edit by typing only: zero speed disables dragging and the text cursor hides
                // the drag hint. The parser accepts the `0x` prefix.
                ui.add(
                    egui::DragValue::new(&mut self.gap_fill)
                        .hexadecimal(2, false, true)
                        .custom_parser(|s| {
                            parse_hex_addr(s).map(|v| u8::try_from(v).unwrap_or(u8::MAX).into())
                        })
                        .prefix("0x")
                        .speed(0.0),
                )
                .on_hover_cursor(egui::CursorIcon::Text);
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
        // Edit popups are bound to the tab they are opened for
        let curr_id = self.get_curr_session().map(|s| s.id);

        let has_file = self.get_curr_session().is_some();
        let has_data = self.get_curr_session().is_some_and(|s| s.ih.size != 0);
        let has_modifications = self
            .get_curr_session()
            .is_some_and(|s| !s.editor.modified.is_empty());

        // RELOCATE BUTTON
        if ui
            .add_enabled(has_data, egui::Button::new("Relocate..."))
            .clicked()
            && !self.popup.active
            && let Some(id) = curr_id
        {
            self.popup.open(PopupState::ReAddr {
                id,
                addr: String::new(),
            });
        }

        // MERGE BUTTON
        if ui
            .add_enabled(has_data, egui::Button::new("Merge..."))
            .clicked()
            && !self.popup.active
            && let Some(id) = curr_id
            && let Some(path) = rfd::FileDialog::new()
                .set_title("Merge with File")
                .pick_file()
        {
            self.popup.open(PopupState::Merge {
                id,
                path,
                addr_curr: String::new(),
                addr_merge: String::new(),
            });
        }

        ui.separator();

        // INSERT RANGE BUTTON
        if ui
            .add_enabled(has_file, egui::Button::new("Insert Range..."))
            .clicked()
            && !self.popup.active
            && let Some(id) = curr_id
        {
            self.popup.open(PopupState::InsertRange {
                id,
                start: String::new(),
                end: String::new(),
            });
        }

        // REMOVE RANGE BUTTON
        if ui
            .add_enabled(has_data, egui::Button::new("Remove Range..."))
            .clicked()
            && !self.popup.active
            && let Some(id) = curr_id
        {
            self.popup.open(PopupState::RemoveRange {
                id,
                start: String::new(),
                end: String::new(),
            });
        }

        ui.separator();

        // RESTORE BUTTON
        if ui
            .add_enabled(has_modifications, egui::Button::new("Restore byte changes"))
            .clicked()
            && let Some(curr_session) = self.get_curr_session_mut()
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
