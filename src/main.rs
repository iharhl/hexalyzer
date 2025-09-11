use eframe::egui::{CentralPanel, Color32, Context, FontFamily, FontId, Label, StrokeKind, TextStyle, TopBottomPanel, ViewportBuilder};
use eframe::{egui, Frame};
use eframe::egui::ScrollArea;
use intelhex_parser::IntelHex;


#[derive(Default)]
struct App {
    ih: IntelHex,
    data: Vec<u8>,
    addr: Vec<usize>,
    selected: Option<usize>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        // set_styles(ctx);
        self.show_top_bar(ctx);
        self.show_central_workspace(ctx);
    }
}

impl App {
    fn show_top_bar(&mut self, ctx: &Context) {
        TopBottomPanel::top("menubar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Open a file")
                            .pick_file()
                        {
                            // TODO: fix path to be &PathBuf not &str
                            let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_1.hex");
                            self.ih = IntelHex::from_hex(input_path).unwrap();

                            for (addr, byte) in &self.ih.to_bttree_map() {
                                self.data.push(*byte);
                                self.addr.push(*addr);
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
            ui.label("Left panel");
        });
        // Right side
        egui::SidePanel::right("right_panel").show(ctx, |ui| {
            ui.label("Right panel");
        });

        CentralPanel::default().show(ctx, |ui| {

            let bytes_per_row = 16;
            // fake data
            // self.data = (0..=255).cycle().take(1_000).collect();

            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for row in 0..(self.data.len() + bytes_per_row - 1) / bytes_per_row {
                        ui.horizontal(|ui| {
                            let start = row * bytes_per_row;
                            let end = (start + bytes_per_row).min(self.data.len());

                            // Address (fixed width, monospaced)
                            ui.monospace(format!("{:08X}", self.addr[start]));
                            ui.add_space(16.0); // spacing before hex block

                            // Hex bytes
                            for i in start..end {
                                let byte = self.data[i];
                                let is_selected = self.selected == Some(i);

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

                                if (i + 1) % 8 == 0 {
                                    ui.add_space(5.0);
                                } else {
                                    // making the space between buttons as small as possible
                                    ui.add_space(-6.0);
                                }

                                if button.clicked() {
                                    self.selected = Some(i);
                                }

                                if is_selected {
                                    ui.painter().rect_filled(
                                        button.rect,
                                        0.0,
                                        egui::Color32::from_rgba_premultiplied(33, 81, 109, 20),
                                        // 31, 53, 68
                                    );
                                }
                            }

                            // Fill remaining hex slots with empty space
                            // TODO: reused code -> put in fn
                            for i in end..(start + bytes_per_row) {
                                // make invisible cell
                                ui.add_sized(
                                    [21.0, 18.0],
                                    egui::Button::new(egui::RichText::new("  ")
                                                          .monospace()
                                                          .size(12.0)
                                    ).fill(Color32::from_white_alpha(0)), // fully transparent
                                );
                                // keep the spacing
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
                                let byte = self.data[i];
                                let ch = if byte.is_ascii_graphic() {
                                    byte as char
                                } else {
                                    '.'
                                };

                                let is_selected = self.selected == Some(i);
                                let label = ui.add(Label::new(
                                    egui::RichText::new(ch.to_string())
                                        .color(Color32::from_gray(160))
                                        .monospace(),
                                ));

                                if label.clicked() {
                                    self.selected = Some(i);
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


fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_resizable(true)
            .with_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native("Hexalyzer", options, Box::new(|_cc| Ok(Box::<App>::default())))
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
