use crate::app::{HexSession, HexViewerApp, MIN_WINDOW_SIZE, colors};
use crate::loader::get_last_modified;
use crate::ui_inspector::format_with_separators;
use crate::ui_tabs;
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
                    self.show_side_panel_contents(ui);
                    false
                } else {
                    show_collapsed_strip(ui)
                }
            },
        )
        .inner;

        self.side_panel_expanded = expanded || strip_clicked;
        self.update_min_window_width(ui.ctx());
    }

    /// Keep the hex bytes of a 16-byte row (and all tabs) in view next to the
    /// open side panel
    fn update_min_window_width(&mut self, ctx: &egui::Context) {
        let expanded = self.side_panel_expanded;
        if self.min_width_set_for == Some(expanded) {
            return;
        }
        self.min_width_set_for = Some(expanded);

        let [mut min_width, min_height] = MIN_WINDOW_SIZE;
        if expanded {
            let hex_view = min_width + EXPANDED_WIDTH - COLLAPSED_WIDTH;
            let tabs = EXPANDED_WIDTH + ui_tabs::min_row_width(self.max_tabs);
            min_width = hex_view.max(tabs);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            min_width, min_height,
        )));
    }

    /// Show contents of the open side panel
    fn show_side_panel_contents(&mut self, ui: &mut egui::Ui) {
        // With no file open, the sections are drawn (greyed out) from a placeholder
        // session, so the layout doesn't jump once a file is opened.
        let has_session = self.active_index.is_some();
        let curr_session: &mut HexSession =
            match self.active_index.and_then(|i| self.sessions.get_mut(i)) {
                Some(session) => session,
                None => &mut self.placeholder_session,
            };

        show_section(ui, "File Information", has_session, |ui| {
            show_file_info(ui, curr_session, has_session);
        });
        show_section(ui, "Jump To Address", has_session, |ui| {
            curr_session.show_jumpto_contents(ui);
        });
        show_section(ui, "Search", has_session, |ui| {
            curr_session.show_search_contents(ui);
        });
        show_section(ui, "Data Inspector", has_session, |ui| {
            curr_session.show_data_inspector_contents(ui);
        });
    }
}

/// Draw a collapsible side panel section, open by default.
/// The header stays clickable while the contents are disabled.
fn show_section(
    ui: &mut egui::Ui,
    title: &str,
    enabled: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(3.0);
    egui::CollapsingHeader::new(title)
        .default_open(true)
        .show(ui, |ui| {
            if !enabled {
                ui.disable();
            }
            ui.add_space(5.0);
            add_contents(ui);
            ui.add_space(5.0);
        });
}

/// Show the file name and size, and warn if the file changed on disk
fn show_file_info(ui: &mut egui::Ui, session: &mut HexSession, has_session: bool) {
    let filepath = session.ih.filepath.to_string_lossy().into_owned();
    let (filename, size) = if has_session {
        let size = format_with_separators(session.ih.size);
        (session.name.as_str(), format!("{size} bytes"))
    } else {
        ("--", "--".to_string())
    };

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
            ui.label(size);
            ui.end_row();
        });

    // Get the last modified time of the file. Changed -> display warning.
    // Throttled at once per 2 seconds to avoid too many syscalls.
    if !filepath.is_empty() && session.last_mod_check.elapsed() > std::time::Duration::from_secs(2)
    {
        session.last_mod_check = std::time::Instant::now();
        if let Some(t) = get_last_modified(&session.ih.filepath).ok()
            && t != std::time::SystemTime::UNIX_EPOCH
        {
            session.file_changed_on_disk = t != session.last_modified;
        }
    }

    if session.file_changed_on_disk {
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
}
