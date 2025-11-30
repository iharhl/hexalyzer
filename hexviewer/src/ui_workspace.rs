use crate::ui_events::EventManager;
use crate::{HexViewer, colors};
use eframe::egui;
use std::ops::Range;

impl HexViewer {
    pub(crate) fn show_central_workspace(&mut self, ctx: &egui::Context) {
        // LEFT PANEL
        egui::SidePanel::left("left_panel")
            .exact_width(280.0)
            .show(ctx, |ui| {
                // FILE INFORMATION
                egui::CollapsingHeader::new("File Information")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        self.show_file_info_contents(ui);
                        ui.add_space(5.0);
                    });
                // JUMP TO ADDRESS
                egui::CollapsingHeader::new("Jump To Address")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        self.show_jumpto_contents(ui);
                        ui.add_space(5.0);
                    });
                // SEARCH
                egui::CollapsingHeader::new("Search")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        self.show_search_contents(ui);
                        ui.add_space(5.0);
                    });
                // DATA INSPECTOR
                egui::CollapsingHeader::new("Data Inspector")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        self.show_data_inspector_contents(ui);
                        ui.add_space(5.0);
                    });
            });

        // CENTRAL VIEW
        egui::CentralPanel::default().show(ctx, |ui| {
            let total_rows =
                (self.addr_range.end - self.addr_range.start).div_ceil(self.bytes_per_row);

            // Get row height in pixels (depends on font size)
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);

            // Create scroll area. Scroll if search or addr jump is triggered.
            let scroll_area = self.create_scroll_area(ui);

            scroll_area
                .scroll_source(egui::containers::scroll_area::ScrollSource {
                    mouse_wheel: true,
                    scroll_bar: true,
                    drag: false,
                })
                .auto_shrink([false; 2])
                .show_rows(ui, row_height, total_rows, |ui, row_range| {
                    self.draw_main_canvas(ui, row_range);
                })
        });
    }

    pub(crate) fn draw_main_canvas(&mut self, ui: &mut egui::Ui, row_range: Range<usize>) {
        // Get state of the mouse click
        let pointer_down = EventManager::is_pointer_down(ui);
        let pointer_hover = EventManager::get_pointer_hover(ui);

        // Detect released clicked
        if !pointer_down {
            self.selection.released = true;
        }

        // Get state of key press
        let typed_char = EventManager::get_keyboard_input_char(ui);

        // Update byte edit buffer base on the key press
        self.update_edit_buffer(typed_char);

        // Cancel byte editing on Esc press
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.editor.clear()
        }

        // Draw rows
        for row in row_range {
            self.draw_row(ui, row, pointer_down, pointer_hover);
        }
    }

    fn draw_row(
        &mut self,
        ui: &mut egui::Ui,
        row: usize,
        pointer_down: bool,
        pointer_hover: Option<egui::Pos2>,
    ) {
        ui.horizontal(|ui| {
            // Start and end addresses
            let start = self.addr_range.start + row * self.bytes_per_row;
            let end = start + self.bytes_per_row;

            // Display address (fixed width, monospaced)
            ui.monospace(format!("{:08X}", start));

            // Add space before hex block
            ui.add_space(16.0);

            // Hex bytes representation row
            for addr in start..end {
                // Determine is the current byte selected
                let byte = self.ih.get_byte(addr);
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
                        format!("{:02X}", b)
                    }
                } else {
                    "--".to_string()
                };

                // Show byte as a button
                let button = ui.add_sized(
                    [21.0, 18.0],
                    egui::Button::new(
                        egui::RichText::new(display_value)
                            .monospace()
                            .size(12.0)
                            .color(bg_color),
                    )
                    .fill(colors::TRANSPARENT),
                );

                // Update the selection range
                if pointer_down
                    && pointer_hover.is_some()
                    && byte.is_some()
                    && button.rect.contains(pointer_hover.unwrap())
                {
                    self.selection.update(addr);
                }

                // Highlight byte if selected
                if is_selected {
                    ui.painter()
                        .rect_filled(button.rect, 0.0, colors::LIGHT_BLUE);
                }

                // Add space every 8 bytes
                if (addr + 1) % 8 == 0 {
                    ui.add_space(5.0);
                } else {
                    // Make space between buttons as small as possible
                    ui.add_space(-6.0);
                }
            }

            // Add space before ASCII row
            ui.add_space(16.0);

            // ASCII representation row
            for addr in start..end {
                // Determine display char
                let byte = self.ih.get_byte(addr);
                let ch = if let Some(b) = byte {
                    if b.is_ascii_graphic() { b as char } else { '.' }
                } else {
                    ' '
                };

                // Determine is char selected
                let is_selected = byte.is_some() && self.selection.is_addr_within_range(addr);

                // Show char as label
                let label = ui.add(egui::Label::new(
                    egui::RichText::new(ch.to_string())
                        .color(colors::GRAY_160)
                        .monospace(),
                ));

                // Update the selection range
                if pointer_down
                    && pointer_hover.is_some()
                    && byte.is_some()
                    && label.rect.contains(pointer_hover.unwrap())
                {
                    self.selection.update(addr);
                }

                // Highlight char if selected
                if is_selected {
                    ui.painter()
                        .rect_filled(label.rect, 0.0, colors::LIGHT_BLUE);
                }
            }
        });
    }
}
