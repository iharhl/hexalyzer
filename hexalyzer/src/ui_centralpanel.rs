use crate::app::HexSession;
use crate::events::EventState;
use crate::hexview::{HexRenderer, PageContext, Viewport, VisiblePage};
use eframe::egui;

impl HexSession {
    /// Displays the central panel of the UI for rendering the hex editor content.
    /// This function draws the main content area of the application. It uses the `egui::CentralPanel`
    /// to define the central region and implements a scrollable hex view with UI event handling.
    pub(crate) fn show_central_panel(
        &mut self,
        ctx: &egui::Context,
        bytes_per_row: usize,
        events: &EventState,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Align to 0x10 so every printed row address ends with '0'
            const ROW_ADDR_ALIGN: usize = 0x10;

            // Get start and end addresses of the visible area. Find total rows to display.
            let display_start = (*self.addr.start()) & !(ROW_ADDR_ALIGN - 1);
            let display_end = ((*self.addr.end()).saturating_add(1) + (ROW_ADDR_ALIGN - 1))
                & !(ROW_ADDR_ALIGN - 1);
            let total_rows = (display_end - display_start).div_ceil(bytes_per_row);

            // Get row height in pixels (depends on font size)
            let font_height = ui.text_style_height(&egui::TextStyle::Monospace);

            // Build the VisiblePage before entering the draw closure, so we
            // avoid borrow conflicts between page_builder and other HexSession fields.
            let mut page = VisiblePage {
                start_addr: 0,
                bytes_per_row,
                row_count: 0,
                data: Vec::new(),
                flags: Vec::new(),
                ascii: Vec::new(),
                display_hex: Vec::new(),
            };

            // Create a scroll area. Scroll if search or addr jump is triggered.
            self.create_step_scroll(bytes_per_row).show_rows(
                ui,
                font_height,
                total_rows,
                |ui, row_range| {
                    // Compute the visible page from model state
                    let viewport = Viewport {
                        display_start,
                        first_row: row_range.start,
                        row_count: row_range.end - row_range.start,
                        bytes_per_row,
                    };
                    let page_ctx = PageContext::editor(&self.selection, &self.editor, &self.search);
                    self.page_builder
                        .compute(&self.ih, &viewport, &page_ctx, &mut page);

                    // Handle user interaction and paint
                    self.draw_page(ui, &page, events);
                },
            );
        });

        // Reset the state of search and jump after drawing the central panel
        self.search.addr = None;
        self.jump_to.addr = None;
    }

    fn draw_page(&mut self, ui: &mut egui::Ui, page: &VisiblePage, events: &EventState) {
        let pointer_state = events.pointer_state;

        // Detect released click
        if !pointer_state.pointer_down {
            self.selection.released = true;
        }

        // Update byte edit buffer based on the key press
        self.update_edit_buffer(&events.hex_chars_released);

        // Cancel byte editing / selection on Esc press
        if events.escape_pressed {
            if !self.editor.in_progress {
                self.selection.clear();
            }
            self.search.clear();
            self.editor.clear();
        }

        // Paint the hex view and get interaction response
        let response = HexRenderer::paint(ui, page);

        // Handle click or drag on a hex byte
        if let Some(addr) = response.interacted_addr
            && self.ih.read_byte(addr).is_some()
        {
            self.search.loose_focus();
            self.jump_to.loose_focus();

            if events.shift_down {
                self.selection.shift_update(addr);
            } else {
                self.selection.update(addr);
            }
        }

        // Handle arrow key events — stop at gap boundaries.
        if let Some(r) = self.selection.range.as_mut() {
            let bpr = page.bytes_per_row;
            match events.arrow_key_released {
                Some(egui::Key::ArrowLeft) => {
                    let target = r[0].saturating_sub(1);
                    if self.ih.read_byte(target).is_some() {
                        r[0] = target;
                        r[1] = target;
                    }
                }
                Some(egui::Key::ArrowRight) => {
                    let target = r[0].saturating_add(1);
                    if self.ih.read_byte(target).is_some() {
                        r[0] = target;
                        r[1] = target;
                    }
                }
                Some(egui::Key::ArrowUp) => {
                    let target = r[0].saturating_sub(bpr);
                    if self.ih.read_byte(target).is_some() {
                        r[0] = target;
                        r[1] = target;
                    }
                }
                Some(egui::Key::ArrowDown) => {
                    let target = r[0].saturating_add(bpr);
                    if self.ih.read_byte(target).is_some() {
                        r[0] = target;
                        r[1] = target;
                    }
                }
                _ => {}
            }
        }
    }
}
