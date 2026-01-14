use crate::HexViewerApp;
use crate::app::colors;
use crate::events::collect_ui_events;
use eframe::egui;

//  ========================== Popup Type logic ============================= //

#[derive(Clone, PartialEq, Eq)]
pub enum PopupType {
    Error,
    About,
    ReAddr,
}

impl PopupType {
    pub const fn title(&self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::About => "About",
            Self::ReAddr => "Re-Address",
        }
    }
}

//  ========================== Popup logic =================================== //

#[derive(Default)]
pub struct Popup {
    /// Is there a pop-up
    pub(crate) active: bool,
    /// Type of the pop-up. Used to determine the title and content of the window.
    pub(crate) ptype: Option<PopupType>,
    /// Text field content in the pop-up, if present
    text_input: String,
}

impl Popup {
    /// Clear (aka remove) the pop-up
    pub const fn clear(&mut self) {
        self.active = false;
        self.ptype = None;
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
            same project.\n\nThe app does not support partial file loading (yet?) so RAM usage \
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

    fn display_readdr(&mut self, ui: &mut egui::Ui) -> bool {
        ui.vertical(|ui| {
            ui.add_space(3.0);
            ui.label("New start address:");
            ui.add_space(3.0);

            // Add text field to enter new start address
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.popup.text_input)
                    .desired_width(ui.available_width() - 70.0),
            );

            // Only allow up to 8 hex digits in the text field
            if response.changed() {
                self.popup.text_input.retain(|c| c.is_ascii_hexdigit());
                self.popup.text_input.truncate(8);
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

        // Block interaction with the app
        egui::Area::new(egui::Id::from("modal_blocker"))
            .order(egui::Order::Background)
            .fixed_pos(content_rect.left_top())
            .show(ctx, |ui| {
                ui.allocate_rect(content_rect, egui::Sense::click());

                // Collect input events once per frame and store in the app state
                *self.events.borrow_mut() = collect_ui_events(ui);
            });

        // Darken the background
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("modal_bg"),
        ));
        painter.rect_filled(content_rect, 0.0, colors::SHADOW);

        let mut is_open = self.popup.active;
        let was_open = self.popup.active;

        let Some(popup_type) = self.popup.ptype.clone() else {
            self.popup.clear();
            return;
        };

        // Display the pop-up
        let window = egui::Window::new(popup_type.title())
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);

        // Track OK button or Enter press
        let mut close_confirm = false;

        window.show(ctx, |ui| match popup_type {
            PopupType::Error => {
                let error = self.error.borrow().clone().unwrap_or_default();
                close_confirm = Self::display_error(ui, &error);
            }
            PopupType::About => close_confirm = Self::display_about(ui),
            PopupType::ReAddr => close_confirm = self.display_readdr(ui),
        });

        self.popup.active = !close_confirm && is_open && !self.events.borrow().escape_pressed;

        // If the window got closed this frame
        if was_open && !self.popup.active {
            *self.error.borrow_mut() = None;

            // If the pop-up closed was readdr -> relocate bytes and do some cleanup
            if self.popup.ptype == Some(PopupType::ReAddr) && close_confirm {
                let addr = usize::from_str_radix(&self.popup.text_input, 16).unwrap_or_default();

                // Clear text field
                self.popup.text_input.clear();

                if let Some(curr_session) = self.get_curr_session_mut() {
                    // Re-address the IntelHex
                    match curr_session.ih.relocate(addr) {
                        Ok(()) => {}
                        Err(err) => {
                            self.popup.clear();
                            self.error.borrow_mut().replace(err.to_string());
                            return;
                        }
                    }

                    // Re-calculate address range
                    curr_session.addr = curr_session.ih.get_min_addr().unwrap_or(0)
                        ..=curr_session.ih.get_max_addr().unwrap_or(0);

                    // Redo search
                    curr_session.search.redo();
                }
            }

            self.popup.clear();
        }
    }
}
