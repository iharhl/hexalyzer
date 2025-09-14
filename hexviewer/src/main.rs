use std::collections::BTreeMap;
use eframe::egui::{CentralPanel, Color32, Context, Label, TextStyle, TopBottomPanel, ViewportBuilder};
use eframe::{egui, Frame};
use eframe::egui::ScrollArea;
use intelhex::IntelHex;


//
// IMPROVEMENTS:
// 1) Render one widget per line:
//    a) one for byte and one for ascii
//    b) render selection using ui.interact
// 2) Optimize how we store the hex data in the app
//    a) store in HashMap more optimal?
//
// ADDITIONAL FEATURES:
// 1) Multi-byte select
//


#[derive(Default)]
struct App {
    ih: IntelHex,
    byte_addr_map: BTreeMap<usize, u8>,
    min_addr: usize,
    max_addr: usize,
    selected: Option<(usize, u8)>,
    selection: String,
    error: Option<String>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        // set_styles(ctx);
        self.show_top_bar(ctx);
        self.show_popup_if_error(ctx);
        self.show_central_workspace(ctx);
    }
}

impl App {
    fn show_popup_if_error(&mut self, ctx: &Context) {
        if let Some(msg) = self.error.clone() {
            let screen_rect = ctx.screen_rect();

            // Block interaction with the app
            egui::Area::new(egui::Id::from("modal_blocker"))
                .order(egui::Order::Background)
                .fixed_pos(screen_rect.left_top())
                .show(ctx, |ui| {
                    ui.allocate_rect(screen_rect, egui::Sense::click());
                });

            // Darken the background
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("modal_bg"),
            ));
            painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(150));

            // Display pop-up
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .fixed_size([300.0, 150.0]) // TODO: fix
                .title_bar(false)
                .show(ctx, |ui| {
                    ui.label(msg);
                    ui.add_space(10.0);
                    if ui.button("OK").clicked() {
                        self.error = None; // close
                    }
                });
        }
    }

    fn show_top_bar(&mut self, ctx: &Context) {
        TopBottomPanel::top("menubar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Open a file")
                            .pick_file()
                        {
                            let ih = IntelHex::from_hex(path);

                            if let Err(msg) = ih {
                                self.error = Some(msg.to_string());
                            } else {
                                self.ih = ih.unwrap();

                                for (addr, byte) in &self.ih.to_btree_map() {
                                    self.byte_addr_map.insert(*addr, *byte);
                                }

                                self.min_addr = self.byte_addr_map.keys().min().unwrap().clone();
                                self.max_addr = self.byte_addr_map.keys().max().unwrap().clone();
                            }
                        }
                    }
                    if ui.button("Export").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Save As")
                            .save_file()
                        {
                            let output_path = concat!(env!("CARGO_MANIFEST_DIR"), "/build/ih_gen.hex");
                            self.ih.write_hex(output_path)
                                .expect("Failed to save file");
                        }
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        println!("About clicked");
                    }
                });
            });
        });
    }

    fn show_central_workspace(&mut self, ctx: &Context) {
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
                            ui.label(self.ih.filepath.to_str().unwrap());
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
                        .num_columns(3) // two columns: label + value
                        .spacing([20.0, 4.0]) // horizontal & vertical spacing
                        .show(ui, |ui| {

                            // ui.allocate_ui_with_layout(
                            //     egui::vec2(80.0, 0.0),  // 80 px wide, height auto
                            //     Layout::left_to_right(egui::Align::Center),
                            //     |ui| {
                            //         ui.label("Short");
                            //         ui.label("Short");
                            //         ui.label("Short");
                            //         ui.end_row();
                            //     },
                            // );

                            ui.heading("Type");
                            ui.heading("Signed");
                            ui.heading("Unsigned");
                            ui.end_row();

                            let val = self.selected.unwrap_or((0, 0)).1;
                            ui.label("(u)int-8");
                            ui.label((val as i8).to_string());
                            ui.label((val as u8).to_string());
                            ui.end_row();

                            let val: u16 = (val as u16) << 8;
                            ui.label("(u)int-16");
                            ui.label((val as i16).to_string());
                            ui.label((val as u16).to_string());
                            ui.end_row();

                            let val: u32 = (val as u32) << 16;
                            ui.label("(u)int-32");
                            ui.label((val as i32).to_string());
                            ui.label((val as u32).to_string());
                            ui.end_row();

                            let val: u64 = (val as u64) << 32;
                            ui.label("(u)int-64");
                            ui.label((val as i64).to_string());
                            ui.label((val as u64).to_string());
                            // ui.text_edit_singleline(&m);
                            // ui.text_edit_singleline(&mut self.s);
                            ui.end_row();

                            // let val: u32 = (val as u32) >> 32;
                            // ui.label("float-32");
                            // ui.label((val as f32).to_string());
                            // ui.end_row();

                            ui.label("float-64");
                            ui.label((val as f64).to_string());
                            ui.end_row();
                        });
                });
        });
        // Right side
        egui::SidePanel::right("search_panel").show(ctx, |ui| {
            ui.label("Search panel");
        });

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
                            let start = self.min_addr + row * bytes_per_row;
                            let end = (start + bytes_per_row); //.min(self.min_addr + self.byte_addr_map.len());

                            // Address (fixed width, monospaced)
                            ui.monospace(format!("{:08X}", start));
                            ui.add_space(16.0); // spacing before hex block

                            // Hex bytes
                            for i in start..end {
                                let byte = self.byte_addr_map.get(&i).copied().unwrap_or(0xFF);
                                let is_selected = self.selected == Some((i, byte));

                                let bg_color = if i % 2 == 0 {
                                    Color32::from_gray(210)
                                } else {
                                    Color32::from_gray(160) // light gray
                                };

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
                                    ui.painter().rect_filled(
                                        button.rect,
                                        0.0,
                                        egui::Color32::from_rgba_premultiplied(33, 81, 109, 20),
                                        // 31, 53, 68
                                    );
                                }

                                if (i + 1) % 8 == 0 {
                                    ui.add_space(5.0);
                                } else {
                                    // making the space between buttons as small as possible
                                    ui.add_space(-6.0);
                                }
                            }

                            ui.add_space(16.0); // spacing before ASCII

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
                                        egui::Color32::from_rgba_premultiplied(33, 81, 109, 20),
                                    );
                                }
                            }
                        });
                    };
                })
        });
    }
}


fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_resizable(true)
            .with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Hexalyzer",
        options,
        Box::new(|_cc| Ok(Box::new(App::default())))
    )
}


// fn set_styles(ctx: &Context) {
//     let mut style = (*ctx.style()).clone();
//     style.text_styles = [
//         (TextStyle::Heading, FontId::new(22.0, FontFamily::Monospace)),
//         (TextStyle::Body, FontId::new(18.0, FontFamily::Monospace)),
//         (TextStyle::Button, FontId::new(14.0, FontFamily::Monospace)),
//     ].into();
//     ctx.set_style(style);
// }
