use crate::app::HexSession;
use eframe::egui;

/// Custom scroll area that scrolls in discrete steps
pub struct StepScrollArea {
    id: egui::Id,
    target_row: Option<usize>,
}

impl StepScrollArea {
    pub const fn new(id: egui::Id) -> Self {
        Self {
            id,
            target_row: None,
        }
    }

    pub const fn with_target_row(mut self, row: Option<usize>) -> Self {
        self.target_row = row;
        self
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn show_rows<R>(
        self,
        ui: &mut egui::Ui,
        font_height: f32,
        total_rows: usize,
        add_contents: impl FnOnce(&mut egui::Ui, std::ops::Range<usize>) -> R,
    ) -> R {
        // Allocate the full available space
        let (rect, _response) = ui.allocate_at_least(ui.available_size(), egui::Sense::click());

        // Get / set row state
        let mut top_row = self
            .target_row
            .unwrap_or_else(|| ui.data_mut(|d| *d.get_temp_mut_or_default(self.id)));

        // Make discrete scroll logic (one row is a scroll step).
        // Add threshold to ignore small drifts.
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta.abs() > 0.4 {
            let row_delta = if scroll_delta > 0.0 { -1 } else { 1 };
            top_row = top_row.saturating_add_signed(row_delta);
        }

        // Make view boundary.
        // Visible rows are not fully accurate. Had to add margin for a more consistent display.
        let full_row_size = font_height + ui.spacing().item_spacing.y + 2.5;
        let visible_rows = (rect.height() / full_row_size).floor() as usize;
        // Allow 1 empty row at the bottom
        let max_top_row = total_rows.saturating_sub(visible_rows - 1);
        top_row = top_row.min(max_top_row);

        ui.data_mut(|d| d.insert_temp(self.id, top_row));

        // Draw a custom scrollbar
        draw_custom_scrollbar(ui, rect, top_row, total_rows, visible_rows, self.id);

        // Render content
        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child_ui.set_clip_rect(rect);

        let row_range = top_row..(top_row + visible_rows).min(total_rows);
        add_contents(&mut child_ui, row_range)
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
/// Draw a custom scrollbar
fn draw_custom_scrollbar(
    ui: &egui::Ui,
    rect: egui::Rect,
    top_row: usize,
    total_rows: usize,
    visible_rows: usize,
    id: egui::Id,
) {
    if total_rows <= visible_rows {
        return;
    }

    // Setup layout & margins
    let margin = 4.0;
    let bottom_margin = 12.0; // extra space from the bottom
    let scroll_area_rect = egui::Rect::from_min_max(
        egui::pos2(rect.right() - 14.0, rect.top() + margin),
        egui::pos2(rect.right() - 2.0, rect.bottom() - bottom_margin),
    );

    // Animation logic (for shrinking / expanding)
    let is_hovered = ui.rect_contains_pointer(scroll_area_rect);
    let expansion = ui.ctx().animate_bool(id.with("anim"), is_hovered);
    let bar_width = egui::lerp(4.0..=10.0, expansion);
    let scrollbar_rect = scroll_area_rect.with_min_x(scroll_area_rect.right() - bar_width);

    // Calculate handle size based on the total number of rows and visible rows.
    // Set a limit for how small the handle gets.
    let max_top_row = total_rows.saturating_sub(visible_rows);
    let mut handle_height = (visible_rows as f32 / total_rows as f32) * scrollbar_rect.height();
    handle_height = handle_height.max(20.0);

    // Travel range is the track height minus the handle height
    let travel_range = scrollbar_rect.height() - handle_height;
    let progress = top_row as f32 / max_top_row as f32;
    let handle_y_offset = progress * travel_range;

    // Track scrollbar interactions
    let response = ui.interact(
        scrollbar_rect,
        id.with("bar"),
        egui::Sense::click_and_drag(),
    );
    if (response.clicked() || response.dragged())
        && let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos())
    {
        // Calculate where the mouse is relative to the track.
        // Center the handle on the mouse click for a better feel.
        let click_y = pointer_pos.y - scrollbar_rect.top() - handle_height / 2.0;
        let t = (click_y / travel_range).clamp(0.0, 1.0);

        let new_row = (t * max_top_row as f32).round() as usize;
        ui.data_mut(|d| d.insert_temp(id, new_row));
    }

    // Paint the track
    let track_color = ui.visuals().extreme_bg_color;
    ui.painter()
        .rect_filled(scrollbar_rect, 2.0, track_color.gamma_multiply(expansion));

    // Paint the handle
    let handle_rect = egui::Rect::from_min_size(
        egui::pos2(
            scrollbar_rect.left(),
            scrollbar_rect.top() + handle_y_offset,
        ),
        egui::vec2(bar_width, handle_height),
    );
    let handle_color = if response.dragged() {
        ui.visuals().widgets.active.bg_fill
    } else if is_hovered {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };

    ui.painter().rect_filled(handle_rect, 2.0, handle_color);
}

impl HexSession {
    /// Helper to convert address to row index. Offsets so that the target row is 3 rows
    /// after the start of the visible area (if possible).
    const fn get_target_row(&self, addr: usize, bytes_per_row: usize) -> usize {
        let byte_idx = addr.saturating_sub(*self.addr.start());
        (byte_idx / bytes_per_row).saturating_sub(3)
    }

    /// Create `StepScrollArea` - a custom scroll area with row-wise discrete steps
    pub(crate) fn create_step_scroll(&self, bytes_per_row: usize) -> StepScrollArea {
        let mut target_row = None;

        if let Some(addr) = self.search.addr {
            target_row = Some(self.get_target_row(addr, bytes_per_row));
        } else if let Some(addr) = self.jump_to.addr {
            target_row = Some(self.get_target_row(addr, bytes_per_row));
        }

        StepScrollArea::new(egui::Id::new(self.scroll_id)).with_target_row(target_row)
    }
}
