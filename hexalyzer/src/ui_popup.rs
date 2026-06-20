use crate::HexViewerApp;
use crate::app::colors;
use crate::events;
use crate::loader;
use eframe::egui;
use std::path::PathBuf;

//  ========================== Close Action ================================== //

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseAction {
    Save,
    SaveAs,
    DontSave,
}

//  ========================== Popup State =================================== //

#[derive(Debug)]
pub enum PopupState {
    Error(String),
    About,
    ReAddr {
        addr: String,
    },
    Merge {
        path: PathBuf,
        addr_curr: String,
        addr_merge: String,
    },
    InsertRange {
        start: String,
        end: String,
    },
    CloseConfirm {
        session_id: usize,
        reload_after: bool,
    },
}

impl PopupState {
    pub(crate) const fn title(&self) -> &'static str {
        match self {
            Self::Error(_) => "Error",
            Self::About => "About",
            Self::ReAddr { .. } => "Re-Address",
            Self::Merge { .. } => "Merge",
            Self::InsertRange { .. } => "Insert Range",
            Self::CloseConfirm { .. } => "Unsaved Changes",
        }
    }

    /// Returns `true` if this popup type should block interaction with the
    /// rest of the application.
    pub(crate) const fn is_blocking(&self) -> bool {
        matches!(self, Self::Error(_) | Self::CloseConfirm { .. })
    }

    /// Render the popup content. Returns `true` when the user confirms (OK / Enter).
    /// For `CloseConfirm` popups, also sets `close_action` on the Popup container.
    fn show(&mut self, ui: &mut egui::Ui, events: &events::EventState) -> bool {
        match self {
            Self::Error(msg) => {
                ui.label(msg.as_str());
                ui.add_space(10.0);
                false
            }
            Self::About => Self::show_about(ui),
            Self::ReAddr { addr: address } => {
                Self::show_hex_field(ui, "New start address:", address);
                ui.button(" OK ").clicked() || events.enter_released
            }
            Self::Merge {
                addr_curr: addr_current,
                addr_merge,
                ..
            } => {
                Self::show_hex_field(
                    ui,
                    "New start address for the current file:\n(leave empty to not change it)",
                    addr_current,
                );
                Self::show_hex_field(
                    ui,
                    "New start address for the selected file:\n(leave empty to not change it)",
                    addr_merge,
                );
                ui.button(" OK ").clicked() || events.enter_released
            }
            Self::InsertRange { start, end } => {
                Self::show_hex_field(ui, "Start address:", start);
                Self::show_hex_field(ui, "End address (inclusive):", end);
                ui.button(" OK ").clicked() || events.enter_released
            }
            Self::CloseConfirm { .. } => {
                ui.label("This file has unsaved changes. What would you like to do?");
                ui.add_space(10.0);
                false
            }
        }
    }

    fn show_about(ui: &mut egui::Ui) -> bool {
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

        false
    }

    fn show_hex_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
        ui.vertical(|ui| {
            ui.add_space(3.0);
            ui.label(label);
            ui.add_space(3.0);

            ui.horizontal(|ui| {
                ui.label("0x");

                let response = ui.add(
                    egui::TextEdit::singleline(value).desired_width(ui.available_width() - 70.0),
                );

                if response.changed() {
                    value.retain(|c| c.is_ascii_hexdigit());
                    value.truncate(8);
                }
            });
        });

        ui.add_space(8.0);
    }

    /// Execute the action for a confirmed popup
    fn on_confirm(self, app: &mut HexViewerApp) {
        match self {
            Self::ReAddr { addr: address } => {
                let addr = usize::from_str_radix(&address, 16).unwrap_or_default();
                let Some(curr_session) = app.get_curr_session_mut() else {
                    return;
                };

                let old_start_addr = curr_session.ih.get_min_addr();

                if let Err(err) = curr_session.ih.relocate(addr) {
                    app.error.replace(err.to_string());
                    return;
                }

                if let Some(old_start_addr) = old_start_addr {
                    curr_session.editor.remap_modified(addr, old_start_addr);
                }

                curr_session.addr = curr_session.ih.get_min_addr().unwrap_or(0)
                    ..=curr_session.ih.get_max_addr().unwrap_or(0);
                curr_session.search.redo();
            }
            Self::InsertRange { start, end } => {
                let start_addr = usize::from_str_radix(&start, 16).ok();
                let end_addr = usize::from_str_radix(&end, 16).ok();

                let Some((start, end)) = start_addr.zip(end_addr) else {
                    app.error.replace("Invalid address format".to_string());
                    return;
                };

                let Some(curr_session) = app.get_curr_session_mut() else {
                    return;
                };

                if let Err(err) = curr_session.ih.write_range(start, end) {
                    app.error.replace(err.to_string());
                    return;
                }

                curr_session.addr = curr_session.ih.get_min_addr().unwrap_or(0)
                    ..=curr_session.ih.get_max_addr().unwrap_or(0);
                curr_session.search.redo();
            }
            Self::Merge {
                path,
                addr_curr: addr_current,
                addr_merge,
            } => {
                let addr1 = usize::from_str_radix(&addr_current, 16).ok();
                let addr2 = usize::from_str_radix(&addr_merge, 16).ok();

                app.merge_file_into_curr_session(&path, addr1, addr2);

                if let Some(curr_session) = app.get_curr_session_mut() {
                    curr_session.addr = curr_session.ih.get_min_addr().unwrap_or(0)
                        ..=curr_session.ih.get_max_addr().unwrap_or(0);
                    curr_session.search.redo();
                }
            }
            Self::Error(_) | Self::About => {}
            Self::CloseConfirm {
                session_id,
                reload_after,
            } => {
                Self::handle_close_confirm(app, session_id, reload_after);
            }
        }
    }

    fn handle_close_confirm(app: &mut HexViewerApp, session_id: usize, reload_after: bool) {
        let action = app
            .popup
            .close_action
            .take()
            .unwrap_or(CloseAction::DontSave);

        match action {
            CloseAction::Save => {
                app.save_curr_session();
                if reload_after {
                    app.reload_file(session_id);
                } else {
                    app.close_file(session_id);
                }
            }
            CloseAction::SaveAs => {
                if !Self::save_as_dialog(app) {
                    return;
                }
                if reload_after {
                    app.reload_file(session_id);
                } else {
                    app.close_file(session_id);
                }
            }
            CloseAction::DontSave => {
                if reload_after {
                    app.reload_file(session_id);
                } else {
                    app.close_file(session_id);
                }
            }
        }
    }

    /// Show the Save-As dialog for the current session. Returns `true` on success.
    fn save_as_dialog(app: &mut HexViewerApp) -> bool {
        let path = app
            .get_curr_session()
            .filter(|s| s.ih.size != 0)
            .and_then(|s| {
                rfd::FileDialog::new()
                    .set_title("Save As")
                    .set_file_name(s.name.clone())
                    .save_file()
            });

        let Some(mut path) = path else {
            return false;
        };

        if path.extension().is_none() {
            path.set_extension("bin");
        }

        let Some(session) = app.get_curr_session_mut() else {
            return false;
        };

        let kind = loader::kind_from_extension(&path);
        if let Err(msg) = loader::write_ih_to_path(&mut session.ih, &path, &kind) {
            app.error = Some(msg);
            return false;
        }

        let Some(session) = app.get_curr_session_mut() else {
            return false;
        };
        session.editor.modified.clear();
        true
    }
}

//  ========================== Popup container =============================== //

#[derive(Default)]
pub struct Popup {
    pub(crate) active: bool,
    pub(crate) state: Option<PopupState>,
    pub(crate) close_action: Option<CloseAction>,
}

impl Popup {
    /// Open a new popup with the given state.
    pub(crate) fn open(&mut self, state: PopupState) {
        self.active = true;
        self.state = Some(state);
    }

    /// Clear (aka remove) the popup.
    pub(crate) fn clear(&mut self) {
        self.active = false;
        self.state = None;
        self.close_action = None;
    }
}

//  ========================== HexViewerApp ================================== //

impl HexViewerApp {
    /// Show the pop-up
    pub(crate) fn show_popup(&mut self, ctx: &egui::Context) {
        let content_rect = ctx.content_rect();

        let mut is_open = self.popup.active;
        let was_open = self.popup.active;

        let Some(popup_state) = self.popup.state.as_mut() else {
            self.popup.clear();
            return;
        };

        let blocking = popup_state.is_blocking();

        if blocking {
            // Block interactions
            egui::Area::new(egui::Id::from("modal_blocker"))
                .order(egui::Order::Background)
                .fixed_pos(content_rect.left_top())
                .show(ctx, |ui| {
                    ui.allocate_rect(content_rect, egui::Sense::click());
                });

            // Darken the background
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("modal_bg"),
            ));
            painter.rect_filled(content_rect, 0.0, colors::SHADOW);
        }

        // Display the pop-up
        let mut window = egui::Window::new(popup_state.title())
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

        // For CloseConfirm: need to render custom buttons and capture action
        let is_close_confirm = matches!(
            self.popup.state.as_ref(),
            Some(PopupState::CloseConfirm { .. })
        );

        if is_close_confirm {
            // Drop the mutable borrow on state before re-borrowing in the closure
            let _ = self.popup.state.as_mut();

            let mut chosen_action: Option<CloseAction> = None;

            window.show(ctx, |ui| {
                ui.label("This file has unsaved changes. What would you like to do?");
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        chosen_action = Some(CloseAction::Save);
                    }
                    if ui.button("Save As...").clicked() {
                        chosen_action = Some(CloseAction::SaveAs);
                    }
                    if ui.button("Don't Save").clicked() {
                        chosen_action = Some(CloseAction::DontSave);
                    }
                });
            });

            if let Some(action) = chosen_action {
                self.popup.close_action = Some(action);
                close_confirm = true;
            }
        } else {
            window.show(ctx, |ui| {
                close_confirm = self
                    .popup
                    .state
                    .as_mut()
                    .is_some_and(|s| s.show(ui, &self.events));
            });
        }

        self.popup.active = !close_confirm && is_open && !self.events.escape_pressed;

        // If the window got closed this frame
        if was_open && !self.popup.active {
            self.error = None;

            if close_confirm {
                // Take ownership of the state to run the confirm action
                if let Some(state) = self.popup.state.take() {
                    state.on_confirm(self);
                }
            }

            self.popup.clear();
        }
    }
}
