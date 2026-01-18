use crate::app::{HexSession, colors};
use crate::events::collect_ui_events;
use crate::ui_button::light_mono_button;
use eframe::egui;
use std::ops::Range;

impl HexSession {
    /// Displays the central panel of the UI for rendering the hex editor content.
    /// This function draws the main content area of the application. It uses the `egui::CentralPanel`
    /// to define the central region and implements a scrollable hex view with UI event handling.
    pub(crate) fn show_central_panel(&mut self, ctx: &egui::Context, bytes_per_row: usize) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let total_rows = (self.addr.end() - self.addr.start()).div_ceil(bytes_per_row);

            // Get row height in pixels (depends on font size)
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);

            // Create scroll area. Scroll if search or addr jump is triggered.
            let scroll_area = self.create_scroll_area(ui, bytes_per_row);

            scroll_area
                .wheel_scroll_multiplier(egui::Vec2 { x: 1.0, y: 0.4 }) // slow vertical scroll
                .scroll_source(egui::containers::scroll_area::ScrollSource {
                    mouse_wheel: true,
                    scroll_bar: true,
                    drag: false,
                })
                .auto_shrink([false; 2])
                .show_rows(ui, row_height, total_rows, |ui, row_range| {
                    // Collect input events once per frame and store in the app state
                    *self.events.borrow_mut() = collect_ui_events(ui);
                    // Draw the main canvas with hex content
                    self.draw_main_canvas(ui, row_range, bytes_per_row);
                })
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
    ) {
        // Get state of the mouse click from aggregated events
        let pointer_down = self.events.borrow().pointer_down;
        let pointer_hover = self.events.borrow().pointer_hover;

        // Detect released clicked
        if !pointer_down {
            self.selection.released = true;
        }

        // Get state of key press (hex chars) from aggregated events
        let typed_char = self.events.borrow().last_hex_char_released;

        // Update byte edit buffer base on the key press
        self.update_edit_buffer(typed_char);

        // Cancel byte editing / selection on Esc press
        if self.events.borrow().escape_pressed {
            if !self.editor.in_progress {
                self.selection.clear();
            }
            self.search.clear();
            self.editor.clear();
        }

        // Draw rows
        for row in row_range {
            self.draw_row(ui, row, pointer_down, pointer_hover, bytes_per_row);
        }

        // Handle arrow key events
        // TODO: jump over empty bytes and up down presses
        if let Some(r) = self.selection.range.as_mut() {
            match self.events.borrow().arrow_key_released {
                Some(egui::Key::ArrowLeft) => {
                    r[0] = r[0].saturating_sub(1);
                    r[1] = r[0];
                }
                Some(egui::Key::ArrowRight) => {
                    r[0] = r[0].saturating_add(1);
                    r[1] = r[0];
                }
                _ => {}
            }
        }
    }

    fn draw_row(
        &mut self,
        ui: &mut egui::Ui,
        row: usize,
        pointer_down: bool,
        pointer_hover: Option<egui::Pos2>,
        bytes_per_row: usize,
    ) {
        ui.horizontal(|ui| {
            // Start and end addresses
            let start = self.addr.start() + row * bytes_per_row;
            let end = start + bytes_per_row;

            // Display address (fixed width, monospaced)
            ui.monospace(format!("{start:08X}"));

            // Add space before hex block
            ui.add_space(16.0);

            // Hex bytes representation row
            for addr in start..end {
                // Remove spacing between buttons
                ui.spacing_mut().item_spacing.x = 0.0;

                // Determine is the current byte selected
                let byte = self.ih.read_byte(addr);
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
                let button = light_mono_button(
                    ui,
                    egui::Vec2::new(21.0, 18.0),
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

                    self.selection.update(addr);
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
            for addr in start..end {
                // Spacing between ascii labels
                ui.spacing_mut().item_spacing.x = 1.0;

                // Determine display char
                let byte = self.ih.read_byte(addr);
                let ch = byte.map_or(' ', |b| if b.is_ascii_graphic() { b as char } else { '.' });

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
                    self.selection.update(addr);
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

        if !self.search.results.is_empty() {
            // If search active -> highlight if inside search results (2nd prio)
            let is_inside_match = self.search.results.iter().any(|&start| {
                let end = start.saturating_add(self.search.length);
                (start..end).contains(&addr)
            });

            if is_inside_match {
                ui.painter().rect_filled(widget.rect, 0.0, colors::GREEN);
                return;
            }
        }

        if self.editor.modified.contains_key(&addr) {
            // If modified -> highlight (3rd prio)
            ui.painter().rect_filled(widget.rect, 0.0, colors::MUD);
        }
    }
}
