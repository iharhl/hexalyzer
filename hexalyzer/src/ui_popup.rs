use crate::HexViewerApp;
use crate::app::colors;
use crate::events;
use crate::loader;
use eframe::egui;
use std::path::PathBuf;

/// Maximum size of a range that can be inserted at once. Guards against typos
/// allocating gigabytes on the UI thread.
const MAX_INSERT_RANGE_SIZE: usize = 256 * 1024 * 1024;

/// Width at which the text of a blocking popup wraps
const MODAL_MAX_WIDTH: f32 = 420.0;

//  ========================== Close Action ================================== //

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseAction {
    Save,
    Export,
    DontSave,
}

//  ========================== Popup Outcome ================================= //

/// What the user did with the popup in this frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopupOutcome {
    /// Nothing - the popup stays open
    Open,
    /// Confirmed (OK / Enter / a close action) - run the confirm action and close
    Confirmed,
    /// Dismissed (OK on an error / Cancel) - close without any action
    Dismissed,
}

impl PopupOutcome {
    const fn confirmed_if(cond: bool) -> Self {
        if cond { Self::Confirmed } else { Self::Open }
    }
}

//  ========================== Popup State =================================== //

#[derive(Debug)]
pub enum PopupState {
    Error(String),
    About,
    ReAddr {
        id: usize,
        addr: String,
    },
    Merge {
        id: usize,
        path: PathBuf,
        addr_curr: String,
        addr_merge: String,
    },
    InsertRange {
        id: usize,
        start: String,
        end: String,
    },
    RemoveRange {
        id: usize,
        start: String,
        end: String,
    },
    CloseConfirm {
        id: usize,
        reload_after: bool,
    },
    QuitConfirm {
        /// Names of the tabs with unsaved changes
        names: Vec<String>,
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
            Self::RemoveRange { .. } => "Remove Range",
            Self::CloseConfirm { .. } | Self::QuitConfirm { .. } => "Unsaved Changes",
        }
    }

    /// ID of the session an edit popup acts on
    const fn target_id(&self) -> Option<usize> {
        match self {
            Self::ReAddr { id, .. }
            | Self::Merge { id, .. }
            | Self::InsertRange { id, .. }
            | Self::RemoveRange { id, .. } => Some(*id),
            _ => None,
        }
    }

    /// Returns `true` if this popup type should block interaction with the
    /// rest of the application.
    pub(crate) const fn is_blocking(&self) -> bool {
        matches!(
            self,
            Self::Error(_) | Self::CloseConfirm { .. } | Self::QuitConfirm { .. }
        )
    }

    /// Render the popup content and return what the user did with it.
    /// For `CloseConfirm` / `QuitConfirm` popups, also sets the chosen `close_action`.
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        events: &events::EventState,
        close_action: &mut Option<CloseAction>,
    ) -> PopupOutcome {
        match self {
            Self::Error(msg) => {
                ui.label(msg.as_str());
                ui.add_space(10.0);
                if ui.button(" OK ").clicked() {
                    PopupOutcome::Dismissed
                } else {
                    PopupOutcome::Open
                }
            }
            Self::About => {
                Self::show_about(ui);
                PopupOutcome::Open
            }
            Self::ReAddr { addr: address, .. } => {
                Self::show_hex_field(ui, "New start address:", address);
                PopupOutcome::confirmed_if(ui.button(" OK ").clicked() || events.enter_released)
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
                PopupOutcome::confirmed_if(ui.button(" OK ").clicked() || events.enter_released)
            }
            Self::InsertRange { start, end, .. } | Self::RemoveRange { start, end, .. } => {
                Self::show_hex_field(ui, "Start address:", start);
                Self::show_hex_field(ui, "End address (inclusive):", end);
                PopupOutcome::confirmed_if(ui.button(" OK ").clicked() || events.enter_released)
            }
            Self::CloseConfirm { .. } => {
                ui.label("This file has unsaved changes. What would you like to do?");
                ui.add_space(10.0);

                let mut outcome = PopupOutcome::Open;
                ui.horizontal(|ui| {
                    for (label, action) in [
                        ("Save", CloseAction::Save),
                        ("Export...", CloseAction::Export),
                        ("Don't Save", CloseAction::DontSave),
                    ] {
                        if ui.button(label).clicked() {
                            *close_action = Some(action);
                            outcome = PopupOutcome::Confirmed;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = PopupOutcome::Dismissed;
                    }
                });
                outcome
            }
            Self::QuitConfirm { names } => {
                ui.label("These files have unsaved changes:");
                for name in names.iter() {
                    ui.label(format!("  •  {name}"));
                }
                ui.add_space(5.0);
                ui.label("Save them before quitting?");
                ui.add_space(10.0);

                let mut outcome = PopupOutcome::Open;
                ui.horizontal(|ui| {
                    for (label, action) in [
                        ("Save All", CloseAction::Save),
                        ("Don't Save", CloseAction::DontSave),
                    ] {
                        if ui.button(label).clicked() {
                            *close_action = Some(action);
                            outcome = PopupOutcome::Confirmed;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = PopupOutcome::Dismissed;
                    }
                });
                outcome
            }
        }
    }

    fn show_about(ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.add_space(5.0);

            ui.heading("Hexalyzer");
            ui.label("Cross-platform hex viewing and editing app");

            ui.add_space(3.0);
            ui.separator();
            ui.add_space(3.0);

            ui.label(
                "The app is built with egui - immediate-mode GUI library. \
                 HEX/BIN parsing and writing is handled by IntelHex standalone library, \
                 built as part of the same project.",
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
        // Act on the tab the pop-up was opened for (might be different from active one)
        if let Some(id) = self.target_id()
            && !app.switch_to_session(id)
        {
            return;
        }

        match self {
            Self::ReAddr { addr: address, .. } => {
                let Some(addr) = usize::from_str_radix(&address, 16).ok() else {
                    app.error.replace("Invalid address format".to_string());
                    return;
                };

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

                curr_session.on_layout_changed();
            }
            Self::InsertRange { start, end, .. } => {
                let start_addr = usize::from_str_radix(&start, 16).ok();
                let end_addr = usize::from_str_radix(&end, 16).ok();

                let Some((start, end)) = start_addr.zip(end_addr) else {
                    app.error.replace("Invalid address format".to_string());
                    return;
                };

                if end.saturating_sub(start) >= MAX_INSERT_RANGE_SIZE {
                    app.error.replace(format!(
                        "Range is too large: {} (max {})",
                        loader::format_size((end - start).saturating_add(1)),
                        loader::format_size(MAX_INSERT_RANGE_SIZE)
                    ));
                    return;
                }

                let Some(curr_session) = app.get_curr_session_mut() else {
                    return;
                };

                if let Err(err) = curr_session.ih.write_range(start, end) {
                    app.error.replace(err.to_string());
                    return;
                }

                curr_session.on_layout_changed();
            }
            Self::RemoveRange { start, end, .. } => {
                let start_addr = usize::from_str_radix(&start, 16).ok();
                let end_addr = usize::from_str_radix(&end, 16).ok();

                let Some((start, end)) = start_addr.zip(end_addr) else {
                    app.error.replace("Invalid address format".to_string());
                    return;
                };

                let Some(curr_session) = app.get_curr_session_mut() else {
                    return;
                };

                if let Err(err) = curr_session.ih.remove_range(start, end) {
                    app.error.replace(err.to_string());
                    return;
                }

                curr_session.on_layout_changed();
            }
            Self::Merge {
                path,
                addr_curr: addr_current,
                addr_merge,
                ..
            } => {
                let addr1 = usize::from_str_radix(&addr_current, 16).ok();
                let addr2 = usize::from_str_radix(&addr_merge, 16).ok();

                // Only update the layout if the merge was actually applied
                if app.merge_file_into_curr_session(&path, addr1, addr2)
                    && let Some(curr_session) = app.get_curr_session_mut()
                {
                    curr_session.on_layout_changed();
                }
            }
            Self::Error(_) | Self::About => {}
            Self::CloseConfirm { id, reload_after } => {
                Self::handle_close_confirm(app, id, reload_after);
            }
            Self::QuitConfirm { .. } => Self::handle_quit_confirm(app),
        }
    }

    fn handle_quit_confirm(app: &mut HexViewerApp) {
        if app.popup.close_action.take() == Some(CloseAction::Save) {
            for index in 0..app.sessions.len() {
                if !app.has_unsaved_changes(index) {
                    continue;
                }
                // Show the tab being saved, e.g. if its save fails or needs a confirmation
                app.active_index = Some(index);
                // Stay open if saving failed or was canceled
                if !app.save_curr_session() {
                    return;
                }
            }
        }

        // The window is closed by `handle_close_request` on the next frame
        app.quit_confirmed = true;
    }

    fn handle_close_confirm(app: &mut HexViewerApp, id: usize, reload_after: bool) {
        let action = app
            .popup
            .close_action
            .take()
            .unwrap_or(CloseAction::DontSave);

        // The tab may have been closed while the popup was open -> nothing to do
        let Some(index) = app.get_session_index(id) else {
            return;
        };

        // Swap the active session to the one being closed (in case the user
        // closes the tab that is not in the view)
        app.active_index = Some(index);

        let saved = match action {
            CloseAction::Save => app.save_curr_session(),
            CloseAction::Export => app.export_curr_session(),
            CloseAction::DontSave => true,
        };

        // Keep the tab (and its edits) if saving failed or was canceled
        if !saved {
            return;
        }

        if reload_after {
            app.reload_file(index);
        } else {
            app.close_file(index);
        }
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
    /// Ask the user what to do with the unsaved changes of the session at `index`
    /// before closing it (or reloading it, if `reload_after` is set).
    pub(crate) fn open_close_confirm(&mut self, index: usize, reload_after: bool) {
        if let Some(session) = self.sessions.get(index) {
            self.popup.open(PopupState::CloseConfirm {
                id: session.id,
                reload_after,
            });
        }
    }

    /// Make the session with the given id the active one.
    /// Returns `false` if there is no such session (e.g. tab was closed).
    fn switch_to_session(&mut self, id: usize) -> bool {
        let Some(index) = self.get_session_index(id) else {
            return false;
        };
        self.active_index = Some(index);
        true
    }

    /// Show the pop-up
    pub(crate) fn show_popup(&mut self, ctx: &egui::Context) {
        // Name of the file an edit popup acts on. Close the popup if its tab is gone.
        let target_index = self
            .popup
            .state
            .as_ref()
            .and_then(PopupState::target_id)
            .map(|id| self.get_session_index(id));
        let target_name = match target_index {
            Some(Some(index)) => Some(self.sessions[index].name.as_str()),
            Some(None) => {
                self.popup.clear();
                return;
            }
            None => None,
        };

        let Some(state) = self.popup.state.as_mut() else {
            self.popup.clear();
            return;
        };

        let title = state.title();
        let events = &self.events;
        let close_action = &mut self.popup.close_action;
        let mut is_open = true;

        let outcome = if state.is_blocking() {
            // A modal blocks both the pointer and keyboard focus for everything below
            // it, so nothing behind the popup can be activated while it is open
            egui::Modal::new(egui::Id::new("blocking_popup"))
                .backdrop_color(colors::SHADOW)
                .show(ctx, |ui| {
                    ui.set_max_width(MODAL_MAX_WIDTH);
                    ui.label(egui::RichText::new(title).strong());
                    ui.separator();
                    ui.add_space(5.0);
                    state.show(ui, events, close_action)
                })
                .inner
        } else {
            egui::Window::new(title)
                .open(&mut is_open)
                .collapsible(false)
                .resizable(false)
                .default_pos(ctx.content_rect().center() - egui::vec2(100.0, 50.0))
                .movable(true)
                .show(ctx, |ui| {
                    if let Some(name) = target_name {
                        ui.label(format!("File: {name}"));
                        ui.add_space(3.0);
                    }
                    state.show(ui, events, close_action)
                })
                .and_then(|response| response.inner)
                .unwrap_or(PopupOutcome::Open)
        };

        let close = !is_open || outcome != PopupOutcome::Open || self.events.escape_pressed;
        self.popup.active = !close;

        if close {
            self.error = None;

            // Take ownership of the state to run the confirm action
            if outcome == PopupOutcome::Confirmed
                && let Some(state) = self.popup.state.take()
            {
                state.on_confirm(self);
                // Run the next frame right away to close the window after
                // a confirmed quit
                ctx.request_repaint();
            }

            self.popup.clear();
        }
    }
}
