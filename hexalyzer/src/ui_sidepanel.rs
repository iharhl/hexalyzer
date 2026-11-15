use crate::app::{HexSession, HexViewerApp, colors};
use crate::events::EventState;
use crate::loader::get_last_modified;
use crate::ui_inspector::format_with_separators;
use eframe::egui;

/// Width of the open side panel (incl margins)
const EXPANDED_WIDTH: f32 = 280.0;
/// Width of the strip left in place of the collapsed side panel (incl margins)
const COLLAPSED_WIDTH: f32 = 30.0;

/// Draw the collapsed side panel.
/// Returns `true` if the strip was clicked.
fn show_collapsed_strip(ui: &egui::Ui) -> bool {
    let rect = ui.max_rect();
    let response = ui
        .interact(
            rect,
            egui::Id::unique("side_panel_collapsed_strip"),
            egui::Sense::click(),
        )
        .on_hover_text("Show Sidebar")
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    let visuals = ui.style().interact(&response);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        ">",
        egui::TextStyle::Body.resolve(ui.style()),
        visuals.text_color(),
    );

    response.clicked()
}

impl HexViewerApp {
    /// Show the side panel with the file information, jump to address, search, and data inspector.
    ///
    /// When collapsed, a thin strip stays visible: clicking it or dragging its edge outward
    /// reopens the panel. The open panel's edge can be dragged to the window edge to close
    /// it, and double-clicking the edge toggles it.
    pub(crate) fn show_side_panel(&mut self, ui: &mut egui::Ui) {
        // Clone events before mutable borrow of self for session access
        let mut events = self.events.clone();

        // Consume Enter confirming a popup or typed into the converter window
        if self.popup.active || self.converter.has_focus() {
            events.enter_released = false;
        }

        // Copied as the panel contents borrow `self` mutably
        let mut expanded = self.side_panel_expanded;

        let collapsed_panel = egui::Panel::left("left_panel_collapsed")
            .exact_size(COLLAPSED_WIDTH)
            .resizable(true);
        let expanded_panel = egui::Panel::left("left_panel")
            .exact_size(EXPANDED_WIDTH)
            .resizable(true);

        let strip_clicked = egui::Panel::show_switched(
            ui,
            &mut expanded,
            collapsed_panel,
            expanded_panel,
            |ui, show_expanded| {
                if show_expanded {
                    self.show_side_panel_contents(ui, &events);
                    false
                } else {
                    show_collapsed_strip(ui)
                }
            },
        )
        .inner;

        self.side_panel_expanded = expanded || strip_clicked;
    }

    /// Show contents of the open side panel
    fn show_side_panel_contents(&mut self, ui: &mut egui::Ui, events: &EventState) {
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
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                            ui.label("File Name");
                        });
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

                        ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                            ui.label("Payload Size");
                        });
                        let size = format_with_separators(curr_session.ih.size);
                        ui.label(format!("{size} bytes"));
                        ui.end_row();
                    });

                // Get the last modified time of the file. Changed -> display warning.
                // Throttled at once per 2 seconds to avoid too many syscalls.
                if !filepath.is_empty()
                    && curr_session.last_mod_check.elapsed() > std::time::Duration::from_secs(2)
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
                curr_session.show_jumpto_contents(ui, events);
                ui.add_space(5.0);
            });

        ui.add_space(3.0);

        // SEARCH
        egui::CollapsingHeader::new("Search")
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(5.0);
                curr_session.show_search_contents(ui, events);
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
    }
}
