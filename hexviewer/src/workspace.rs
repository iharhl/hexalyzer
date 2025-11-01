use eframe::egui;
use eframe::egui::{CentralPanel, Color32, Context, Label, ScrollArea, TextStyle};
use super::HexViewer;


impl HexViewer {
    pub(crate) fn show_central_workspace(&mut self, ctx: &Context) {
        let filename = self.ih.filepath
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "-".to_string());

        // Left side
        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            egui::CollapsingHeader::new("File Information")
                .default_open(true)
                .show(ui, |ui| {
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
                });

            egui::CollapsingHeader::new("Data Inspector")
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("data_inspector_grid")
                        .num_columns(2) // two columns: label & value
                        .spacing([20.0, 4.0]) // horizontal & vertical spacing
                        .show(ui, |ui| {
                            ui.heading("Type");
                            ui.heading("Value");
                            ui.end_row();

                            let val = self.selected.unwrap_or((0, 0)).1;

                            ui.label("uint_8");
                            ui.label((val as u8).to_string());
                            ui.end_row();
                            // let val: u16 = (val as u16) << 8;
                            ui.label("uint_16");
                            ui.label((val as u16).to_string());
                            ui.end_row();
                            // let val: u32 = (val as u32) << 16;
                            ui.label("uint_32");
                            ui.label((val as u32).to_string());
                            ui.end_row();
                            // let val: u64 = (val as u64) << 32;
                            ui.label("uint_64");
                            ui.label((val as u64).to_string());
                            ui.end_row();

                            ui.label("int_8");
                            ui.label((val as i8).to_string());
                            ui.end_row();
                            ui.label("int_16");
                            ui.label((val as i16).to_string());
                            ui.end_row();
                            ui.label("int_32");
                            ui.label((val as i32).to_string());
                            ui.end_row();
                            ui.label("int_64");
                            ui.label((val as i64).to_string());
                            ui.end_row();

                            // TODO: fix floats to show in exponent if too long number
                            // ui.label("float_32");
                            // ui.label((f32::from_le_bytes([val, 0, 0, 0])).to_string());
                            // ui.end_row();
                            // ui.label("float_64");
                            // ui.label((f64::from_le_bytes([val, 0, 0, 0, 0, 0, 0, 0])).to_string());
                            // ui.end_row();
                        });
                });
        });
        // Right side
        egui::SidePanel::right("search_panel").show(ctx, |ui| {
            ui.label("Search panel");
        });
        // Cental view
        CentralPanel::default().show(ctx, |ui| {
            let bytes_per_row = 16;
            // Same as (self.max_addr - self.min_addr) / bytes_per_row
            // but division rounds result up
            let total_rows = ((self.max_addr - self.min_addr) + bytes_per_row - 1) / bytes_per_row;
            // Get row height in pixels (depends on font size)
            let row_height = ui.text_style_height(&TextStyle::Monospace);

            ScrollArea::vertical()
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
                            for i in start..end {
                                let byte = self.byte_addr_map.get(&i).copied().unwrap_or(0xFF);
                                let is_selected = self.selected == Some((i, byte));

                                // Change color of every other byte for better readability
                                let bg_color = if i % 2 == 0 {
                                    Color32::from_gray(210)
                                } else {
                                    Color32::from_gray(160) // light gray
                                };

                                // Each byte is a button
                                let button = ui.add_sized(
                                    [21.0, 18.0],
                                    egui::Button::new(egui::RichText::new(format!("{:02X}", byte))
                                                          .monospace()
                                                          .size(12.0)
                                                          .color(bg_color),
                                    ).fill(Color32::from_white_alpha(0)), // fully transparent,
                                );

                                if button.clicked() {
                                    self.selected = Some((i, byte));
                                }

                                if is_selected {
                                    // Highlight the selected byte
                                    ui.painter().rect_filled(
                                        button.rect,
                                        0.0,
                                        Color32::from_rgba_premultiplied(33, 81, 109, 20),
                                        // 31, 53, 68
                                    );
                                }

                                // Add space every 8 bytes
                                if (i + 1) % 8 == 0 {
                                    ui.add_space(5.0);
                                } else {
                                    // Make space between buttons as small as possible
                                    ui.add_space(-6.0);
                                }
                            }

                            // Add space before ASCII column
                            ui.add_space(16.0);

                            // ASCII representation
                            for i in start..end {
                                // TODO: Retrieving values once again -> not optimal
                                let mut ch = ' ';
                                let mut byte = 0;
                                if let Some(b) = self.byte_addr_map.get(&i).copied() {
                                    byte = b;
                                    ch = if b.is_ascii_graphic() {
                                        b as char
                                    } else {
                                        '.'
                                    }
                                }
                                let is_selected = self.selected == Some((i, byte));

                                let label = ui.add(Label::new(
                                    egui::RichText::new(ch.to_string())
                                        .color(Color32::from_gray(160))
                                        .monospace(),
                                ));

                                if label.clicked() {
                                    self.selected = Some((i, 0)); // TODO: bug
                                }

                                if is_selected {
                                    ui.painter().rect_filled(
                                        label.rect,
                                        0.0,
                                        Color32::from_rgba_premultiplied(33, 81, 109, 20),
                                    );
                                }
                            }
                        });
                    };
                })
        });
    }
}