use crate::HexViewer;
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
                            // Fill min/max address
                            self.addr_range.start = self.ih.get_min_addr().unwrap();
                            self.addr_range.end = self.ih.get_max_addr().unwrap();
                        }
                    }

                    // EXPORT BUTTON
                    if ui.button("Export").clicked()
                        && let Some(path) = rfd::FileDialog::new().set_title("Save As").save_file()
                    {
                        match self.ih.write_hex(path) {
                            Ok(_) => {}
                            Err(msg) => {
                                self.error = Some(msg.to_string());
                            }
                        }
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
