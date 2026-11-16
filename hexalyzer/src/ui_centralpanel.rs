use crate::app::{HexSession, colors};
use crate::events::EventState;
use crate::ui_button;
use eframe::egui;
use intelhexlib::IntelHex;
use std::ops::Range;

/// Align to 0x10 so every printed row address ends with '0'
const ROW_ADDR_ALIGN: usize = 0x10;

/// First address `>= addr` that holds data: `addr` itself if it holds data,
/// otherwise the start of the next chunk.
fn next_data_addr(ih: &IntelHex, addr: usize) -> Option<usize> {
    if ih.read_byte(addr).is_some() {
        return Some(addr);
    }
    ih.iter()
        .find(|(start, data)| **start > addr && !data.is_empty())
        .map(|(start, _)| *start)
}

/// Last address `<= addr` that holds data: `addr` itself if it holds data,
/// otherwise the end of the previous chunk.
fn prev_data_addr(ih: &IntelHex, addr: usize) -> Option<usize> {
    if ih.read_byte(addr).is_some() {
        return Some(addr);
    }
    ih.iter()
        .rev()
        .filter(|(_, data)| !data.is_empty())
        .map(|(start, data)| start + data.len() - 1)
        .find(|&end| end < addr)
}

impl HexSession {
    /// Address of the first displayed row (start address aligned down to `ROW_ADDR_ALIGN`)
    pub(crate) const fn display_start(&self) -> usize {
        *self.addr.start() & !(ROW_ADDR_ALIGN - 1)
    }

    /// Move the cursor on arrow key presses. Gaps between data chunks are skipped.
    /// Returns the cursor address if any arrow key was handled, so the view can follow it.
    fn handle_arrow_keys(&mut self, keys: &[egui::Key], bytes_per_row: usize) -> Option<usize> {
        if keys.is_empty() || self.selection.blocked {
            return None;
        }
        let [_, mut cursor] = self.selection.range?;

        for key in keys {
            let target = match key {
                egui::Key::ArrowLeft => cursor
                    .checked_sub(1)
                    .and_then(|a| prev_data_addr(&self.ih, a)),
                egui::Key::ArrowRight => cursor
                    .checked_add(1)
                    .and_then(|a| next_data_addr(&self.ih, a)),
                egui::Key::ArrowUp => cursor
                    .checked_sub(bytes_per_row)
                    .and_then(|a| prev_data_addr(&self.ih, a)),
                egui::Key::ArrowDown => cursor
                    .checked_add(bytes_per_row)
                    .and_then(|a| next_data_addr(&self.ih, a)),
                _ => None,
            };
            if let Some(addr) = target {
                cursor = addr;
            }
        }

        self.selection.range = Some([cursor, cursor]);
        Some(cursor)
    }

    /// Displays the central panel of the UI for rendering the hex editor content.
    /// This function draws the main content area of the application. It uses the `egui::CentralPanel`
    /// to define the central region and implements a scrollable hex view with UI event handling.
    pub(crate) fn show_central_panel(
        &mut self,
        ui: &mut egui::Ui,
        bytes_per_row: usize,
        events: &EventState,
    ) {
        egui::CentralPanel::default().show(ui, |ui| {
            // Show placeholder for empty files
            if self.ih.size == 0 {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.label(
                        egui::RichText::new("No data")
                            .color(egui::Color32::GRAY)
                            .monospace(),
                    );
                });
                return;
            }
            let align_up = |addr: usize| (addr + (ROW_ADDR_ALIGN - 1)) & !(ROW_ADDR_ALIGN - 1);

            // Get start and end addresses of the visible area. Find total rows to display.
            let display_start = self.display_start();
            let display_end = align_up((*self.addr.end()).saturating_add(1));
            let total_rows = (display_end - display_start).div_ceil(bytes_per_row);

            // Get row height in pixels. A row is as tall as its tallest item: text,
            // byte buttons, or the minimum height of a horizontal layout.
            let row_height = ui
                .text_style_height(&egui::TextStyle::Monospace)
                .max(ui_button::BYTE_BUTTON_SIZE.y)
                .max(ui.spacing().interact_size.y);

            // Move the cursor before creating the scroll area, so the view follows it this frame
            let cursor = self.handle_arrow_keys(&events.arrow_keys_pressed, bytes_per_row);

            // Create a scroll area. Scroll if search or addr jump is triggered,
            // or the cursor was moved by the arrow keys.
            self.create_step_scroll(bytes_per_row, cursor).show_rows(
                ui,
                row_height,
                total_rows,
                |ui, row_range| {
                    // Draw the main canvas with hex content
                    self.draw_main_canvas(ui, row_range, bytes_per_row, display_start, events);
                },
            );
        });

        // Reset the state of search and jump after drawing the central panel
        self.search.addr = None;
        self.jump_to.addr = None;
    }

    fn draw_main_canvas(
        &mut self,
        ui: &mut egui::Ui,
        row_range: Range<usize>,
        bytes_per_row: usize,
        display_start: usize,
        events: &EventState,
    ) {
        // Get state of the mouse click from aggregated events
        let pointer_state = events.pointer_state;

        // Detect released clicked
        if !pointer_state.pointer_down {
            self.selection.released = true;
        }

        // Update byte edit buffer based on the key press
        self.update_edit_buffer(&events.hex_chars);

        // Cancel byte editing / selection on Esc press
        if events.escape_pressed {
            if !self.editor.in_progress {
                self.selection.clear();
            }
            self.search.clear();
            self.editor.clear();
        }

        // Start and end addresses of the whole visible area
        let start = display_start + row_range.start * bytes_per_row;
        let end = display_start + row_range.end * bytes_per_row + bytes_per_row;

        // Get bytes from the buffer for the whole area at once.
        // Limitation: Results in heap allocation. Cannot use `self.ih.iter_range` because it
        // would require a borrow of `self.ih` which conflicts with the mut borrow in `draw_row`
        let bytes = self.ih.read_range_safe(start, end - start);

        // Draw rows
        for (i, row) in row_range.enumerate() {
            self.draw_row(
                ui,
                row,
                events,
                bytes_per_row,
                display_start,
                &bytes[i * bytes_per_row..(i + 1) * bytes_per_row],
            );
        }
    }

    fn draw_row(
        &mut self,
        ui: &mut egui::Ui,
        row: usize,
        events: &EventState,
        bytes_per_row: usize,
        display_start: usize,
        bytes: &[Option<u8>],
    ) {
        // Get state of the mouse click
        let pointer_state = events.pointer_state;
        let shift_down = events.shift_down;
        // Ignore the pointer outside the view (partly hidden rows at its edges)
        let pointer_hover = pointer_state
            .pointer_hover
            .filter(|&hover| ui.clip_rect().contains(hover));
        let pointer_down = pointer_state.pointer_down;

        // Start and end addresses of the current row
        let start = display_start + row * bytes_per_row;
        let end = start + bytes_per_row;

        ui.horizontal(|ui| {
            // Display address (fixed width, monospaced)
            ui.monospace(format!("{start:08X}"));

            // Add space before hex block
            ui.add_space(16.0);

            // Hex bytes representation row
            for (i, addr) in (start..end).enumerate() {
                // Remove spacing between buttons
                ui.spacing_mut().item_spacing.x = 0.0;

                // Determine is the current byte selected
                let byte = bytes[i];
                let is_selected = byte.is_some() && self.selection.is_addr_within_range(addr);

                // Change color of every other byte for better readability
                let bg_color = if addr % 2 == 0 {
                    colors::GRAY_210
                } else {
                    colors::GRAY_160
                };

                // Determine display value of the byte
                let display_value = if let Some(b) = byte {
                    if is_selected && self.editor.in_progress {
                        self.editor.buffer.clone()
                    } else {
                        format!("{b:02X}")
                    }
                } else {
                    "--".to_string()
                };

                // Show byte as a button
                let button = ui_button::light_mono_button(
                    ui,
                    ui_button::BYTE_BUTTON_SIZE,
                    display_value.as_str(),
                    bg_color,
                );

                // Update the selection range
                if pointer_down
                    && byte.is_some()
                    && let Some(hover) = pointer_hover
                    && button.rect.contains(hover)
                {
                    // Force text edit boxes to loose focus if selection is updated
                    self.search.loose_focus();
                    self.jump_to.loose_focus();

                    if shift_down {
                        self.selection.shift_update(addr);
                    } else {
                        self.selection.update(addr);
                    }
                }

                // Highlight byte if selected or modified
                self.highlight_widget(ui, &button, addr, is_selected);

                // Add space every 8 bytes
                if (addr - start + 1).is_multiple_of(8) {
                    ui.add_space(5.0);
                }
            }

            // Add space before ASCII row
            ui.add_space(16.0);

            // ASCII representation row
            for (i, addr) in (start..end).enumerate() {
                // Spacing between ascii labels
                ui.spacing_mut().item_spacing.x = 1.0;

                // Determine display char
                let byte = bytes[i];
                let ch = byte.map_or(' ', |b| {
                    if b.is_ascii_graphic() {
                        b as char
                    } else {
                        '·'
                    }
                });

                // Determine is char selected
                let is_selected = byte.is_some() && self.selection.is_addr_within_range(addr);

                // Show char as label
                let label = ui.add(
                    egui::Label::new(
                        egui::RichText::new(ch.to_string())
                            .color(colors::GRAY_160)
                            .monospace(),
                    )
                    .selectable(false),
                );

                // Update the selection range
                if pointer_down
                    && byte.is_some()
                    && let Some(hover) = pointer_hover
                    && label.rect.contains(hover)
                {
                    if shift_down {
                        self.selection.shift_update(addr);
                    } else {
                        self.selection.update(addr);
                    }
                }

                // Highlight char if selected or modified
                self.highlight_widget(ui, &label, addr, is_selected);
            }
        });
    }

    fn highlight_widget(
        &self,
        ui: &egui::Ui,
        widget: &egui::Response,
        addr: usize,
        is_selected: bool,
    ) {
        if is_selected {
            // If selected -> highlight (1st prio)
            ui.painter()
                .rect_filled(widget.rect, 0.0, colors::LIGHT_BLUE);
            return;
        }

        if self.search.is_hit(addr) {
            // If inside search results -> highlight (2nd prio).
            // The current result is brighter than the others.
            let color = if self.search.is_current_hit(addr) {
                colors::BRIGHT_GREEN
            } else {
                colors::GREEN
            };
            ui.painter().rect_filled(widget.rect, 0.0, color);
            return;
        }

        if self.editor.modified.contains_key(&addr) {
            // If modified -> highlight (3rd prio)
            ui.painter().rect_filled(widget.rect, 0.0, colors::MUD);
        }
    }
}
