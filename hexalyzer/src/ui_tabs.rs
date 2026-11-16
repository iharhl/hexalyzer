use crate::app::HexViewerApp;
use eframe::egui;
use std::sync::Arc;

/// Height of a tab
const TAB_HEIGHT: f32 = 26.0;
/// Space between the edges of a tab and its content
const TAB_PADDING_X: f32 = 8.0;
/// Space between the name and the close button
const NAME_CLOSE_GAP: f32 = 6.0;
/// Side of the square close button
const CLOSE_SIZE: f32 = 16.0;
/// Space between two tabs
const TAB_SPACING: f32 = 2.0;
/// Corner radius of a tab and the close button hover background
const TAB_CORNER_RADIUS: f32 = 4.0;
/// Width of the space left for the name in the narrowest tab (enough for "...")
const MIN_NAME_WIDTH: f32 = 16.0;
/// Narrowest tab width
const TAB_MIN_WIDTH: f32 =
    TAB_PADDING_X + TAB_PADDING_X + MIN_NAME_WIDTH + NAME_CLOSE_GAP + CLOSE_SIZE;

/// Horizontal margin of the panel holding the tabs (egui's default for top panels)
const PANEL_MARGIN_X: f32 = 8.0;

/// Narrowest width of the tab row (incl. its margins) with `max_tabs` tabs open
#[allow(clippy::cast_precision_loss)]
pub fn min_row_width(max_tabs: usize) -> f32 {
    let tabs = max_tabs as f32;
    let spacing = TAB_SPACING * max_tabs.saturating_sub(1) as f32;
    let margins = 2.0 * PANEL_MARGIN_X;
    tabs.mul_add(TAB_MIN_WIDTH, spacing + margins)
}

/// What the user did with a tab this frame
struct TabResponse {
    clicked: bool,
    close_clicked: bool,
}

impl HexViewerApp {
    /// Show tabs with the list of open files.
    /// Tabs are constrained to fit into the available space.
    /// If the number of tabs does not exceed the maximum allowed, the "Open New File" button
    /// is added.
    pub(crate) fn show_tabs(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("tabs_panel").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = TAB_SPACING;

                let names: Vec<String> = self
                    .sessions
                    .iter()
                    .map(|session| {
                        if session.has_unsaved_changes() {
                            format!("*{}", session.name)
                        } else {
                            session.name.clone()
                        }
                    })
                    .collect();

                // Width left for the tabs once the "Open New File" button and the spacing
                // between all items are taken out
                let show_add_button = self.sessions.len() < self.max_tabs;
                let items = names.len() + usize::from(show_add_button);
                let add_button_width = if show_add_button { TAB_HEIGHT } else { 0.0 };
                #[allow(clippy::cast_precision_loss)]
                let spacing = TAB_SPACING * items.saturating_sub(1) as f32;
                let available = ui.available_width() - add_button_width - spacing;

                let ideal: Vec<f32> = names.iter().map(|name| ideal_width(ui, name)).collect();
                let widths = fit_widths(&ideal, available, TAB_MIN_WIDTH);

                let mut tab_to_close = None;
                for (i, (name, width)) in names.iter().zip(widths).enumerate() {
                    let id = ui.make_persistent_id(("tab", self.sessions[i].id));
                    let tab = show_tab(ui, id, name, Some(i) == self.active_index, width);

                    if tab.close_clicked {
                        tab_to_close = Some(i);
                    } else if tab.clicked {
                        self.active_index = Some(i);
                    }
                }

                // Handle closing tabs after the loop
                if let Some(i) = tab_to_close {
                    if self.has_unsaved_changes(i) {
                        self.open_close_confirm(i, false);
                    } else {
                        self.close_file(i);
                    }
                }

                if show_add_button
                    && show_add_button_widget(ui).clicked()
                    && let Some(path) = rfd::FileDialog::new().set_title("Open File").pick_file()
                {
                    self.load_file(&path);
                }
            });
        });
    }
}

/// Width a tab needs to show its full name
fn ideal_width(ui: &egui::Ui, name: &str) -> f32 {
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let text_width = ui
        .painter()
        .layout_no_wrap(name.to_owned(), font_id, egui::Color32::PLACEHOLDER)
        .size()
        .x;
    (2.0f32.mul_add(TAB_PADDING_X, text_width) + NAME_CLOSE_GAP + CLOSE_SIZE).ceil()
}

/// Fit the tabs into `available` width by shrinking the wider ones first.
///
/// All tabs wider than a common cap are cut down to it, and the cap is chosen so that
/// the tabs fill the available width exactly.
#[allow(clippy::cast_precision_loss)]
fn fit_widths(ideal: &[f32], available: f32, min: f32) -> Vec<f32> {
    let mut sorted = ideal.to_vec();
    sorted.sort_by(f32::total_cmp);

    let mut remaining = available;
    let mut cap = f32::INFINITY;
    for (i, &width) in sorted.iter().enumerate() {
        let tabs_left = (sorted.len() - i) as f32;
        if width * tabs_left > remaining {
            cap = (remaining / tabs_left).floor();
            break;
        }
        remaining -= width;
    }

    ideal.iter().map(|&width| width.min(cap).max(min)).collect()
}

/// Lay out a tab name. Cut with "..." if it is wider than `max_width`
fn name_galley(
    ui: &egui::Ui,
    name: &str,
    color: egui::Color32,
    max_width: f32,
) -> Arc<egui::Galley> {
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::simple_singleline(name.to_owned(), font_id, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width);
    ui.painter().layout_job(job)
}

/// Draw a tab: the name on the left, the close button on the right.
/// Hovering a tab with a cut name shows the full name.
fn show_tab(
    ui: &mut egui::Ui,
    id: egui::Id,
    name: &str,
    is_active: bool,
    width: f32,
) -> TabResponse {
    let (_, rect) = ui.allocate_space(egui::vec2(width, TAB_HEIGHT));
    let response = ui.interact(rect, id, egui::Sense::click());

    let close_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.right() - TAB_PADDING_X - CLOSE_SIZE / 2.0,
            rect.center().y,
        ),
        egui::Vec2::splat(CLOSE_SIZE),
    );
    let close = ui.interact(close_rect, id.with("close"), egui::Sense::click());

    // Name the painted widgets for accessibility (screen readers, UI tests)
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::accesskit::Role::Tab, true, is_active, name)
    });
    close.widget_info(|| {
        egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, format!("Close {name}"))
    });

    let visuals = ui.visuals();
    let (fill, text_color) = if is_active {
        (
            visuals.widgets.active.bg_fill,
            visuals.widgets.active.fg_stroke.color,
        )
    } else if response.contains_pointer() {
        (
            visuals.widgets.hovered.bg_fill,
            visuals.widgets.inactive.fg_stroke.color,
        )
    } else {
        (
            visuals.widgets.noninteractive.bg_fill,
            visuals.widgets.inactive.fg_stroke.color,
        )
    };

    let name_max_width = close_rect.left() - NAME_CLOSE_GAP - (rect.left() + TAB_PADDING_X);
    let galley = name_galley(ui, name, text_color, name_max_width.max(0.0));
    let elided = galley.elided;

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, TAB_CORNER_RADIUS, fill);

        let name_pos = egui::pos2(
            rect.left() + TAB_PADDING_X,
            rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(name_pos, galley, text_color);

        if close.hovered() {
            painter.rect_filled(
                close_rect,
                TAB_CORNER_RADIUS,
                text_color.gamma_multiply(0.15),
            );
        }
        painter.text(
            close_rect.center(),
            egui::Align2::CENTER_CENTER,
            "×",
            egui::TextStyle::Body.resolve(ui.style()),
            text_color,
        );
    }

    let response = if elided {
        response.on_hover_text(name)
    } else {
        response
    };

    TabResponse {
        clicked: response.clicked(),
        close_clicked: close.clicked(),
    }
}

/// Draw the "Open New File" button that follows the tabs
fn show_add_button_widget(ui: &mut egui::Ui) -> egui::Response {
    let (id, rect) = ui.allocate_space(egui::Vec2::splat(TAB_HEIGHT));
    let response = ui.interact(rect, id, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = if response.contains_pointer() {
            visuals.widgets.hovered.bg_fill
        } else {
            visuals.widgets.noninteractive.bg_fill
        };
        let painter = ui.painter();
        painter.rect_filled(rect, TAB_CORNER_RADIUS, fill);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "+",
            egui::TextStyle::Body.resolve(ui.style()),
            visuals.strong_text_color(),
        );
    }

    response.on_hover_text("Open New File")
}
