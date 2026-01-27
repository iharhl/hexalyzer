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

        // Start and end addresses
        let start = self.addr.start() + row_range.start * bytes_per_row;
        let end = self.addr.start() + row_range.end * bytes_per_row + bytes_per_row;

        let bytes = self.ih.read_range_safe(start, end - start);


        let font_id = egui::FontId::monospace(11.0);
        // Measure character width (critical for monospaced math)
        let char_width = ui.fonts_mut(|f| f.glyph_width(&font_id, '0'));
        let line_height = ui.fonts_mut(|f| f.row_height(&font_id));

        // let num_rows = (data.len() as f32 / bytes_per_row as f32).ceil() as usize;
        let num_rows = row_range.end - row_range.start;
        let grid_size = egui::Vec2::new(
            char_width * (bytes_per_row * 3) as f32, // "FF " per byte
            line_height * num_rows as f32
        );

        // 1. Allocate the entire area at once
        let (rect, response) = ui.allocate_at_least(grid_size, egui::Sense::click_and_drag());

        // 2. Handle Interaction (The "Virtual Button" logic)
        if response.clicked() {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                let local_pos = pointer_pos - rect.min;
                let col = (local_pos.x / (char_width * 3.0)) as usize;
                let row = (local_pos.y / line_height) as usize;
                let byte_index = row * bytes_per_row + col;

                if byte_index < bytes.len() {
                    println!("Clicked byte index: {}", byte_index);
                }
            }
        }

        // 3. Paint the data
        let painter = ui.painter_at(rect);
        for row in 0..num_rows {
            let row_start = row * bytes_per_row;
            let row_end = (row_start + bytes_per_row).min(bytes.len());
            let row_data = &bytes[row_start..row_end];

            // Format the entire row into one string to save draw calls
            let hex_string: String = row_data.iter()
                .map(|b| {
                    if let Some(b) = b {
                        format!("{b:02X} ")
                    } else {
                        "--".to_string()
                    }
                }).collect();

            let row_pos = rect.min + egui::Vec2::new(0.0, row as f32 * line_height);

            painter.text(
                row_pos,
                egui::Align2::LEFT_TOP,
                hex_string,
                font_id.clone(),
                ui.visuals().text_color(),
            );
        }



        // Draw rows
        // for (i, row) in row_range.enumerate() {
        //     self.draw_row(
        //         ui,
        //         row,
        //         // i,
        //         pointer_down,
        //         pointer_hover,
        //         bytes_per_row,
        //         &bytes[i * bytes_per_row..(i + 1) * bytes_per_row],
        //     );
        // }

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
        bytes: &[Option<u8>],
    ) {
        // Start and end addresses
        let start = self.addr.start() + row * bytes_per_row;
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
            for (i, addr) in (start..end).enumerate() {
                // Spacing between ascii labels
                ui.spacing_mut().item_spacing.x = 1.0;

                // Determine display char
                let byte = bytes[i];
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
