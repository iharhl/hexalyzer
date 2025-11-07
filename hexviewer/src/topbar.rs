use super::HexViewer;
use eframe::egui;
use intelhex::IntelHex;

impl HexViewer {
    pub(crate) fn show_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // FILE MENU
                ui.menu_button("File", |ui| {
                    // OPEN BUTTON
                    if ui.button("Open").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title("Open Hex File")
                            .pick_file()
                    {
                        let ih = IntelHex::from_hex(path);

                        if let Err(msg) = ih {
                            self.error = Some(msg.to_string());
                        } else {
                            self.ih = ih.unwrap();
                            // Clear the map if another hex was loaded before
                            self.byte_addr_map.clear();
                            // Fill data array
                            for (addr, byte) in &self.ih.to_btree_map() {
                                self.byte_addr_map.insert(*addr, *byte);
                            }
                            // Fill address
                            self.min_addr = *self.byte_addr_map.keys().min().unwrap();
                            self.max_addr = *self.byte_addr_map.keys().max().unwrap();
                        }
                    }

                    // EXPORT BUTTON
                    if ui.button("Export").clicked()
                        && let Some(path) = rfd::FileDialog::new().set_title("Save As").save_file()
                    {
                        // TODO: handle saving going wrong
                        self.ih.write_hex(path).expect("Failed to save the file");
                    }
                });

                // TODO: HELP BUTTON
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        println!("About clicked");
                    }
                });
            });
        });
    }
}
