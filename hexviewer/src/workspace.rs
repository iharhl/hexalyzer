use super::HexViewer;
use super::hexviewer::Endianness;
use eframe::egui;

impl HexViewer {
    pub(crate) fn show_central_workspace(&mut self, ctx: &egui::Context) {
        // Get filename
        let filename = self
            .ih
            .filepath
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "-".to_string());

        // LEFT PANEL (FILE INFORMATION & DATA INSPECTOR)
        egui::SidePanel::left("left_panel")
            .exact_width(250.0)
            .show(ctx, |ui| {
                // FILE INFORMATION
                egui::CollapsingHeader::new("File Information")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        egui::Grid::new("file_info_grid")
                            .num_columns(2) // two columns: label + value
                            .spacing([30.0, 4.0]) // horizontal & vertical spacing
                            .show(ui, |ui| {
                                ui.label("File Name");
                                ui.label(filename);
                                ui.end_row();
                                ui.label("File Size");
                                ui.label(format!("{} bytes", self.ih.size));
                                ui.end_row();
                            });
                        ui.add_space(5.0);
                    });

                // DATA INSPECTOR
                egui::CollapsingHeader::new("Data Inspector")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        ui.radio_value(&mut self.endianness, Endianness::Little, "Little Endian");
                        ui.radio_value(&mut self.endianness, Endianness::Big, "Big Endian");
                        ui.add_space(5.0);
                        egui::Grid::new("data_inspector_grid")
                            .num_columns(2) // two columns: label & value
                            .spacing([20.0, 4.0]) // horizontal & vertical spacing
                            .show(ui, |ui| {
                                ui.heading("Type");
                                ui.heading("Value");
                                ui.end_row();

                                if self.selection.range.is_none() {
                                    ui.label("--");
                                    ui.label("--");
                                    ui.end_row();
                                    return;
                                }

                                let sel = self.selection.range.as_ref().unwrap();
                                let mut bytes: Vec<u8> = self
                                    .byte_addr_map
                                    .range(sel.iter().min().unwrap()..=sel.iter().max().unwrap())
                                    .map(|(_, &b)| b)
                                    .collect();

                                if self.endianness == Endianness::Big && bytes.len() > 1 {
                                    bytes.reverse();
                                }

                                match bytes.len() {
                                    1 => {
                                        ui.label("u8");
                                        ui.label(u8::from_le_bytes([bytes[0]]).to_string());
                                        ui.end_row();
                                        ui.label("i8");
                                        ui.label(i8::from_le_bytes([bytes[0]]).to_string());
                                        ui.end_row();
                                    }
                                    2 => {
                                        ui.label("u16");
                                        ui.label(
                                            u16::from_le_bytes(
                                                bytes.as_slice().try_into().unwrap(),
                                            )
                                            .to_string(),
                                        );
                                        ui.end_row();
                                        ui.label("i16");
                                        ui.label(
                                            i16::from_le_bytes(
                                                bytes.as_slice().try_into().unwrap(),
                                            )
                                            .to_string(),
                                        );
                                        ui.end_row();
                                    }
                                    4 => {
                                        ui.label("u32");
                                        ui.label(
                                            u32::from_le_bytes(
                                                bytes.as_slice().try_into().unwrap(),
                                            )
                                            .to_string(),
                                        );
                                        ui.end_row();
                                        ui.label("i32");
                                        ui.label(
                                            i32::from_le_bytes(
                                                bytes.as_slice().try_into().unwrap(),
                                            )
                                            .to_string(),
                                        );
                                        ui.end_row();
                                        // TODO: fix display of f32
                                        // ui.label("f32");
                                        // ui.label(f32::from_le_bytes(bytes.as_slice().try_into().unwrap()).to_string());
                                        // ui.end_row();
                                    }
                                    8 => {
                                        ui.label("u64");
                                        ui.label(
                                            u64::from_le_bytes(bytes.clone().try_into().unwrap())
                                                .to_string(),
                                        );
                                        ui.end_row();
                                        ui.label("i64");
                                        ui.label(
                                            i64::from_le_bytes(bytes.clone().try_into().unwrap())
                                                .to_string(),
                                        );
                                        ui.end_row();
                                        // TODO: fix display of f64
                                        // ui.label("f64");
                                        // ui.label(f64::from_le_bytes(bytes.as_slice().try_into().unwrap()).to_string());
                                        // ui.end_row();
                                    }
                                    _ => {
                                        ui.label("--");
                                        ui.label("--");
                                        ui.end_row();
                                    }
                                }
                            });
                    });
            });

        // RIGHT PANEL
        egui::SidePanel::right("search_panel").show(ctx, |ui| {
            ui.label("Search panel");
        });

        // CENTRAL VIEW
        egui::CentralPanel::default().show(ctx, |ui| {
            let bytes_per_row = 16;
            // Rounds division up
            let total_rows = (self.max_addr - self.min_addr).div_ceil(bytes_per_row);
            // Get row height in pixels (depends on font size)
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);

            egui::ScrollArea::vertical()
                .scroll_source(egui::containers::scroll_area::ScrollSource {
                    mouse_wheel: true,
                    scroll_bar: true,
                    drag: false,
                })
                .auto_shrink([false; 2])
                .show_rows(ui, row_height, total_rows, |ui, row_range| {
                    //
                    for row in row_range {
                        ui.horizontal(|ui| {
                            // Start and end addresses
                            let start = self.min_addr + row * bytes_per_row;
                            let end = start + bytes_per_row;

                            // Display address (fixed width, monospaced)
                            ui.monospace(format!("{:08X}", start));

                            // Add space before hex block
                            ui.add_space(16.0);

                            // Hex bytes
                            for addr in start..end {
                                let byte = self.byte_addr_map.get(&addr).copied();
                                let is_selected =
                                    byte.is_some() && self.selection.is_addr_within_range(addr);

                                // Change color of every other byte for better readability
                                let bg_color = if addr % 2 == 0 {
                                    egui::Color32::from_gray(210)
                                } else {
                                    egui::Color32::from_gray(160) // light gray
                                };

                                if is_selected && self.editor.in_progress {
                                    // If another byte got selected - clear and return
                                    if !self.editor.is_addr_same(addr) {
                                        self.editor.clear();
                                        return;
                                    }
                                    // Create text edit field (TODO: same size as button)
                                    let text_edit =
                                        egui::TextEdit::singleline(&mut self.editor.buffer)
                                            .desired_width(21.0)
                                            .desired_rows(1);
                                    let response = ui.add_sized((21.0, 18.0), text_edit);

                                    // Allows direct typing without needing to select the edit zone
                                    response.request_focus();

                                    // Allow only hex chars
                                    self.editor.buffer.retain(|c| c.is_ascii_hexdigit());

                                    // When two hex chars are entered - commit automatically
                                    if self.editor.buffer.len() == 2 {
                                        if let Ok(value) =
                                            u8::from_str_radix(&self.editor.buffer, 16)
                                        {
                                            let value_ref =
                                                self.byte_addr_map.get_mut(&addr).unwrap();
                                            *value_ref = value;
                                        }
                                        self.editor.clear()
                                    }

                                    // Cancel on esc
                                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        self.editor.clear()
                                    }
                                } else {
                                    // Each byte is a button
                                    let mut display_value = "--".to_string();
                                    if let Some(b) = byte {
                                        display_value = format!("{:02X}", b);
                                    }
                                    let button = ui.add_sized(
                                        [21.0, 18.0],
                                        egui::Button::new(
                                            egui::RichText::new(display_value)
                                                .monospace()
                                                .size(12.0)
                                                .color(bg_color),
                                        )
                                        .fill(egui::Color32::from_white_alpha(0)), // fully transparent,
                                    );

                                    if is_selected
                                        && self.selection.is_single_byte()
                                        && self.selection.released
                                    {
                                        // Capture keyboard input
                                        let mut typed_char: Option<char> = None;
                                        ui.input(|i| {
                                            for event in &i.events {
                                                if let egui::Event::Text(t) = event
                                                    && let Some(c) = t.chars().next()
                                                {
                                                    typed_char = Some(c);
                                                }
                                            }
                                        });
                                        // Start editing if user types a hex character
                                        if let Some(ch) = typed_char
                                            && ch.is_ascii_hexdigit()
                                        {
                                            self.editor.in_progress = true;
                                            self.editor.addr = addr;
                                            self.editor.buffer =
                                                ch.to_ascii_uppercase().to_string();
                                        }
                                    }

                                    let pointer_down = ui.input(|i| i.pointer.primary_down());
                                    let pointer_hover = ui.input(|i| i.pointer.hover_pos());

                                    // Update the selection range
                                    if pointer_down
                                        && pointer_hover.is_some()
                                        && byte.is_some()
                                        && button.rect.contains(pointer_hover.unwrap())
                                    {
                                        self.selection.update(addr);
                                    }

                                    // Detect released clicked
                                    if !pointer_down {
                                        self.selection.released = true;
                                    }

                                    // Highlight selected byte
                                    if is_selected {
                                        ui.painter().rect_filled(
                                            button.rect,
                                            0.0,
                                            egui::Color32::from_rgba_premultiplied(33, 81, 109, 20),
                                            // 31, 53, 68
                                        );
                                    }

                                    // Add space every 8 bytes
                                    if (addr + 1) % 8 == 0 {
                                        ui.add_space(5.0);
                                    } else {
                                        // Make space between buttons as small as possible
                                        ui.add_space(-6.0);
                                    }
                                }
                            }

                            // Add space before ASCII column
                            ui.add_space(16.0);

                            // ASCII representation
                            for addr in start..end {
                                let mut ch = ' ';
                                let mut byte = None;
                                if let Some(b) = self.byte_addr_map.get(&addr).copied() {
                                    byte = Some(b);
                                    ch = if b.is_ascii_graphic() { b as char } else { '.' }
                                }

                                let is_selected =
                                    byte.is_some() && self.selection.is_addr_within_range(addr);

                                let label = ui.add(egui::Label::new(
                                    egui::RichText::new(ch.to_string())
                                        .color(egui::Color32::from_gray(160))
                                        .monospace(),
                                ));

                                let pointer_down = ui.input(|i| i.pointer.primary_down());
                                let pointer_hover = ui.input(|i| i.pointer.hover_pos());

                                if pointer_down
                                    && pointer_hover.is_some()
                                    && byte.is_some()
                                    && label.rect.contains(pointer_hover.unwrap())
                                {
                                    self.selection.update(addr);
                                }

                                if !pointer_down {
                                    self.selection.released = true;
                                }

                                if is_selected {
                                    // Highlight the selected byte
                                    ui.painter().rect_filled(
                                        label.rect,
                                        0.0,
                                        egui::Color32::from_rgba_premultiplied(33, 81, 109, 20),
                                        // 31, 53, 68
                                    );
                                }
                            }
                        });
                    }
                })
        });
    }
}
