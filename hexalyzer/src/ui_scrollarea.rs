use crate::app::HexSession;
use eframe::egui;

/// Where to scroll the view this frame
#[derive(Clone, Copy)]
pub enum ScrollTarget {
    /// Put this row at the top of the view
    Top(usize),
    /// Bring this row into view with the least scrolling
    Follow(usize),
}

/// Custom scroll area that scrolls in discrete steps
pub struct StepScrollArea {
    id: egui::Id,
    target: Option<ScrollTarget>,
}

impl StepScrollArea {
    pub const fn new(id: egui::Id) -> Self {
        Self { id, target: None }
    }

    pub const fn with_target(mut self, target: Option<ScrollTarget>) -> Self {
        self.target = target;
        self
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    pub fn show_rows<R>(
        self,
        ui: &mut egui::Ui,
        row_height: f32,
        total_rows: usize,
        add_contents: impl FnOnce(&mut egui::Ui, std::ops::Range<usize>) -> R,
    ) -> R {
        // Allocate the full available space
        let (rect, _response) = ui.allocate_at_least(ui.available_size(), egui::Sense::click());

        // Get / set row state
        let mut top_row = match self.target {
            Some(ScrollTarget::Top(row)) => row,
            _ => ui.data_mut(|d| *d.get_temp_mut_or_default(self.id)),
        };

        // Distance between the tops of two consecutive rows
        let item_spacing = ui.spacing().item_spacing.y;
        let full_row_size = row_height + item_spacing;

        // Scroll logic
        top_row = self.apply_scroll(ui, top_row, full_row_size);

        // Number of rows that fit fully.
        // At least one row is always treated as visible (tiny windows).
        let visible_rows =
            (((rect.height() + item_spacing) / full_row_size).floor() as usize).max(1);

        // Scroll just enough to keep the followed row in view
        if let Some(ScrollTarget::Follow(row)) = self.target {
            if row < top_row {
                top_row = row;
            } else if row >= top_row + visible_rows {
                top_row = row + 1 - visible_rows;
            }
        }

        // Stop scrolling once the last row is fully in view
        let max_top_row = total_rows.saturating_sub(visible_rows);
        top_row = top_row.min(max_top_row);

        ui.data_mut(|d| d.insert_temp(self.id, top_row));

        // Draw a custom scrollbar
        draw_custom_scrollbar(ui, rect, top_row, total_rows, visible_rows, self.id);

        // Rows that fit fully rarely fill the view exactly. Fill the leftover space with one
        // more, partly hidden row: below the view normally, or above it at the end of the
        // data, where the content is aligned to the bottom edge instead.
        let (row_range, offset_y) = if top_row > 0 && top_row == max_top_row {
            let shown_rows = visible_rows + 1;
            let content_height = (shown_rows as f32).mul_add(full_row_size, -item_spacing);
            (top_row - 1..total_rows, rect.height() - content_height)
        } else {
            (top_row..(top_row + visible_rows + 1).min(total_rows), 0.0)
        };

        // Render content
        let content_rect = rect.with_min_y(rect.top() + offset_y);
        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child_ui.set_clip_rect(rect);

        add_contents(&mut child_ui, row_range)
    }

    /// Move `top_row` by the distance scrolled this frame, in whole rows.
    ///
    /// The view moves proportionally to the input: a wheel notch moves a couple of rows,
    /// a trackpad follows the fingers. Distance smaller than a row is carried over to the
    /// next frame and dropped once the scroll gesture ends, so it never leaks into the
    /// next gesture. Rounding makes even a small wheel notch move at least one row.
    #[allow(clippy::cast_possible_truncation)]
    fn apply_scroll(&self, ui: &egui::Ui, top_row: usize, row_height: f32) -> usize {
        let residual_id = self.id.with("scroll_residual");
        let (delta, scrolling) =
            ui.input(|i| (i.smooth_scroll_delta.y, i.scroll_source().is_some()));

        let carried: f32 = if scrolling {
            ui.data(|d| d.get_temp(residual_id).unwrap_or_default())
        } else {
            0.0
        };
        let distance = carried + delta;
        let rows = (distance / row_height).round();
        ui.data_mut(|d| d.insert_temp(residual_id, rows.mul_add(-row_height, distance)));

        // A positive delta moves the content down, i.e. the view towards earlier rows
        top_row.saturating_add_signed(-(rows as isize))
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
    if travel_range <= 0.0 {
        // The track is too short to fit the handle (tiny window)
        return;
    }
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
    /// Helper to convert address to the index of the row that displays it
    const fn get_row(&self, addr: usize, bytes_per_row: usize) -> usize {
        addr.saturating_sub(self.display_start()) / bytes_per_row
    }

    /// Create `StepScrollArea` - a custom scroll area with row-wise discrete steps.
    /// Search results and address jumps scroll to put the address near the top, and if
    /// the selection is moving - keep it in view.
    pub(crate) fn create_step_scroll(
        &self,
        bytes_per_row: usize,
        follow_addr: Option<usize>,
    ) -> StepScrollArea {
        // Priority: search result, then address jump, then the cursor
        let target = self
            .search
            .addr
            .or(self.jump_to.addr)
            // Offset so that the target row is 3 rows after the start of the visible area
            // (if possible)
            .map(|addr| ScrollTarget::Top(self.get_row(addr, bytes_per_row).saturating_sub(3)))
            .or_else(|| {
                follow_addr.map(|addr| ScrollTarget::Follow(self.get_row(addr, bytes_per_row)))
            });

        StepScrollArea::new(egui::Id::unique(self.id)).with_target(target)
    }
}
