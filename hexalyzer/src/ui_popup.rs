use crate::HexViewerApp;
use crate::app::colors;
use crate::events::{collect_ui_events, collect_ui_events_ctx};
use eframe::egui;
use std::path::PathBuf;

//  ========================== Popup Type logic ============================= //

#[derive(Clone, PartialEq, Eq)]
pub enum PopupType {
    Error,
    About,
    ReAddr,
    Merge(PathBuf),
    InsertRange,
}

impl PopupType {
    pub const fn title(&self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::About => "About",
            Self::ReAddr => "Re-Address",
            Self::Merge(_) => "Merge",
            Self::InsertRange => "Insert Range",
        }
    }

    /// Returns `true` if this popup type should block interaction with the
    /// rest of the application.
    pub const fn is_blocking(&self) -> bool {
        matches!(self, Self::Error)
    }
}

//  ========================== Popup logic =================================== //

#[derive(Default)]
pub struct Popup {
    /// Is there a pop-up
    pub(crate) active: bool,
    /// Type of the pop-up. Used to determine the title and content of the window.
    pub(crate) ptype: Option<PopupType>,
    /// First text field content in the pop-up, if present
    text_input_1: String,
    /// Second text field content in the pop-up, if present
    text_input_2: String,
    /// Whether the popup window is being hovered or dragged, used to suppress
    /// selection interaction in the `CentralPanel`.
    pub(crate) interacting: bool,
}

impl Popup {
    /// Clear (aka remove) the pop-up
    pub fn clear(&mut self) {
        self.active = false;
        self.ptype = None;
        self.interacting = false;
    }
}

//  ========================== HexViewer logic ============================= //

impl HexViewerApp {
    fn display_error(ui: &mut egui::Ui, msg: &str) -> bool {
        ui.label(msg);

        // Add space before close button
        ui.add_space(10.0);

        // Keep the window open
        false
    }

    /// Display the 'About' pop-up
    fn display_about(ui: &mut egui::Ui) -> bool {
        ui.vertical(|ui| {
            ui.add_space(5.0);

            ui.heading("Hexalyzer");
            ui.label("Cross-platform hex viewing and editing app");

            ui.add_space(3.0);
            ui.separator();
            ui.add_space(3.0);

            ui.label(
                "The app is built with egui - immediate-mode GUI library. \
            The hex parsing and writing is handled by IntelHex library, built as part of the \
            same project.\n\nThe app does not support partial file loading so RAM usage \
            while working with very large files will be high.",
            );

            ui.label("\nCheck out the source code on GitHub:");
            ui.hyperlink_to(
                "https://github.com/iharhl/hexalyzer",
                "https://github.com/iharhl/hexalyzer",
            );

            ui.add_space(3.0);
            ui.separator();
            ui.add_space(3.0);

            ui.label(format!(
                "v{} | Copyright (c) 2026 Ihar Hlukhau",
                env!("CARGO_PKG_VERSION")
            ));
            ui.add_space(5.0);
        });

        // Keep the window open
        false
    }

    /// Display the 'Re-address' pop-up
    fn display_readdr(&mut self, ui: &mut egui::Ui) -> bool {
        ui.vertical(|ui| {
            ui.add_space(3.0);
            ui.label("New start address:");
            ui.add_space(3.0);

            // Add text field to enter new start address
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.popup.text_input_1)
                    .desired_width(ui.available_width() - 70.0),
            );

            // Only allow up to 8 hex digits in the text field
            if response.changed() {
                self.popup.text_input_1.retain(|c| c.is_ascii_hexdigit());
                self.popup.text_input_1.truncate(8);
            }
        });

        ui.add_space(8.0);

        if ui.button(" OK ").clicked() || self.events.borrow().enter_released {
            // Close the window
            return true;
        }

        // Keep the window open
        false
    }

    /// Display the 'Merge' pop-up
    fn display_merge(&mut self, ui: &mut egui::Ui) -> bool {
        ui.vertical(|ui| {
            ui.add_space(3.0);
            ui.label("New start address for the current file:\n(leave empty to not change it)");
            ui.add_space(3.0);

            // Add text field to enter new start address
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.popup.text_input_1)
                    .desired_width(ui.available_width() - 70.0),
            );

            // Only allow up to 8 hex digits in the text field
            if response.changed() {
                self.popup.text_input_1.retain(|c| c.is_ascii_hexdigit());
                self.popup.text_input_1.truncate(8);
            }

            // Repeat the same for the file selected for merging
            ui.add_space(3.0);
            ui.label("New start address for the selected file:\n(leave empty to not change it)");
            ui.add_space(3.0);

            let response = ui.add(
                egui::TextEdit::singleline(&mut self.popup.text_input_2)
                    .desired_width(ui.available_width() - 70.0),
            );

            if response.changed() {
                self.popup.text_input_2.retain(|c| c.is_ascii_hexdigit());
                self.popup.text_input_2.truncate(8);
            }
        });

        ui.add_space(8.0);

        if ui.button(" OK ").clicked() || self.events.borrow().enter_released {
            // Close the window
            return true;
        }

        // Keep the window open
        false
    }

    /// Display the 'Insert Range' pop-up
    fn display_insert_range(&mut self, ui: &mut egui::Ui) -> bool {
        ui.vertical(|ui| {
            ui.add_space(3.0);
            ui.label("Start address:");
            ui.add_space(3.0);

            let response = ui.add(
                egui::TextEdit::singleline(&mut self.popup.text_input_1)
                    .desired_width(ui.available_width() - 70.0),
            );

            if response.changed() {
                self.popup.text_input_1.retain(|c| c.is_ascii_hexdigit());
                self.popup.text_input_1.truncate(8);
            }

            ui.add_space(3.0);
            ui.label("End address (inclusive):");
            ui.add_space(3.0);

            let response = ui.add(
                egui::TextEdit::singleline(&mut self.popup.text_input_2)
                    .desired_width(ui.available_width() - 70.0),
            );

            if response.changed() {
                self.popup.text_input_2.retain(|c| c.is_ascii_hexdigit());
                self.popup.text_input_2.truncate(8);
            }
        });

        ui.add_space(8.0);

        if ui.button(" OK ").clicked() || self.events.borrow().enter_released {
            // Close the window
            return true;
        }

        // Keep the window open
        false
    }

    /// Show the pop-up
    pub(crate) fn show_popup(&mut self, ctx: &egui::Context) {
        let content_rect = ctx.content_rect();

        let mut is_open = self.popup.active;
        let was_open = self.popup.active;

        let Some(popup_type) = self.popup.ptype.clone() else {
            self.popup.clear();
            return;
        };

        let blocking = popup_type.is_blocking();

        if blocking {
            // Collect input events via the modal blocker
            egui::Area::new(egui::Id::from("modal_blocker"))
                .order(egui::Order::Background)
                .fixed_pos(content_rect.left_top())
                .show(ctx, |ui| {
                    ui.allocate_rect(content_rect, egui::Sense::click());
                    *self.events.borrow_mut() = collect_ui_events(ui);
                });

            // Darken the background
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("modal_bg"),
            ));
            painter.rect_filled(content_rect, 0.0, colors::SHADOW);
        } else {
            // Non-blocking: collect events from context directly
            *self.events.borrow_mut() = collect_ui_events_ctx(ctx);
        }

        // Display the pop-up
        let mut window = egui::Window::new(popup_type.title())
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false);

        if blocking {
            window = window.anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
        } else {
            window = window
                .default_pos(content_rect.center() - egui::vec2(100.0, 50.0))
                .movable(true);
        }

        // Track OK button or Enter press
        let mut close_confirm = false;

        let popup_response = window.show(ctx, |ui| match popup_type {
            PopupType::Error => {
                let error = self.error.borrow().clone().unwrap_or_default();
                close_confirm = Self::display_error(ui, &error);
            }
            PopupType::About => close_confirm = Self::display_about(ui),
            PopupType::ReAddr => close_confirm = self.display_readdr(ui),
            PopupType::Merge(_) => close_confirm = self.display_merge(ui),
            PopupType::InsertRange => close_confirm = self.display_insert_range(ui),
        });

        self.popup.interacting = popup_response
            .as_ref()
            .is_some_and(|r| r.response.hovered() || r.response.dragged());

        self.popup.active = !close_confirm && is_open && !self.events.borrow().escape_pressed;

        // If the window got closed this frame
        if was_open && !self.popup.active {
            self.handle_popup_close(close_confirm);
        }
    }

    /// Execute the action associated with the popup that was just closed.
    fn handle_popup_close(&mut self, close_confirm: bool) {
        *self.error.borrow_mut() = None;

        if self.popup.ptype == Some(PopupType::ReAddr) && close_confirm {
            self.close_readdr();
        } else if self.popup.ptype == Some(PopupType::InsertRange) && close_confirm {
            self.close_insert_range();
        } else if let Some(PopupType::Merge(path)) = self.popup.ptype.take()
            && close_confirm
        {
            self.close_merge(&path);
        }

        self.popup.clear();
    }

    /// Perform re-addressing and close the 'Re-address' pop-up
    fn close_readdr(&mut self) {
        let addr = usize::from_str_radix(&self.popup.text_input_1, 16).unwrap_or_default();
        self.popup.text_input_1.clear();

        let Some(curr_session) = self.get_curr_session_mut() else {
            return;
        };

        // Relocate the bytes to a new address
        if let Err(err) = curr_session.ih.relocate(addr) {
            self.popup.clear();
            self.error.borrow_mut().replace(err.to_string());
            return;
        }

        // Re-calculate the address range and redo previously active search
        curr_session.addr = curr_session.ih.get_min_addr().unwrap_or(0)
            ..=curr_session.ih.get_max_addr().unwrap_or(0);
        curr_session.search.redo();
    }

    /// Insert a range of bytes into the current session and close the 'Insert Range' pop-up
    fn close_insert_range(&mut self) {
        let start_addr = usize::from_str_radix(&self.popup.text_input_1, 16).ok();
        let end_addr = usize::from_str_radix(&self.popup.text_input_2, 16).ok();
        self.popup.text_input_1.clear();
        self.popup.text_input_2.clear();

        let Some((start, end)) = start_addr.zip(end_addr) else {
            self.popup.clear();
            self.error
                .borrow_mut()
                .replace("Invalid address format".to_string());
            return;
        };

        let Some(curr_session) = self.get_curr_session_mut() else {
            return;
        };

        // Insert the range of bytes
        if let Err(err) = curr_session.ih.write_range(start, end) {
            self.popup.clear();
            self.error.borrow_mut().replace(err.to_string());
            return;
        }

        // Re-calculate the address range and redo previously active search
        curr_session.addr = curr_session.ih.get_min_addr().unwrap_or(0)
            ..=curr_session.ih.get_max_addr().unwrap_or(0);
        curr_session.search.redo();
    }

    /// Merge the selected file into the current session and close the 'Merge' pop-up
    fn close_merge(&mut self, path: &PathBuf) {
        let addr1 = usize::from_str_radix(&self.popup.text_input_1, 16).ok();
        let addr2 = usize::from_str_radix(&self.popup.text_input_2, 16).ok();
        self.popup.text_input_1.clear();
        self.popup.text_input_2.clear();

        self.merge_file_into_curr_session(path, addr1, addr2);

        // Re-calculate the address range and redo previously active search
        if let Some(curr_session) = self.get_curr_session_mut() {
            curr_session.addr = curr_session.ih.get_min_addr().unwrap_or(0)
                ..=curr_session.ih.get_max_addr().unwrap_or(0);
            curr_session.search.redo();
        }
    }
}
