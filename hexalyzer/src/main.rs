#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
// Tell OS to hide the console window when running.
// This attribute is only applied if the target OS is Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod byteedit;
mod events;
mod loader;
mod selection;
mod ui_button;
mod ui_centralpanel;
mod ui_filedrop;
mod ui_inspector;
mod ui_jumpto;
mod ui_menubar;
mod ui_popup;
mod ui_scrollarea;
mod ui_search;
mod ui_sidepanel;
mod ui_tabs;

use crate::ui_popup::PopupType;
use app::HexViewerApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        vsync: true,
        viewport: egui::ViewportBuilder::default()
            .with_icon(load_icon())
            .with_resizable(true)
            .with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Hexalyzer",
        options,
        Box::new(|_cc| Ok(Box::new(HexViewerApp::default()))),
    )
}

impl eframe::App for HexViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(debug_assertions)]
        {
            // Track FPS
            let dt = ctx.input(|i| i.stable_dt);
            let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
            println!("FPS: {fps:.1}");

            // Display dimensions of UI elements on hover.
            // Uncomment if needed.
            // ctx.set_debug_on_hover(true);
        }

        self.show_menu_bar(ctx);

        if self.error.borrow().is_some() {
            self.popup.active = true;
            self.popup.ptype = Some(PopupType::Error);
        }

        self.show_side_panel(ctx);
        self.show_tabs(ctx);

        self.handle_drag_and_drop(ctx);

        // If pop active - show it and return (don't display the hex bytes)
        if self.popup.active {
            self.show_popup(ctx);
            return;
        }

        // Show the content of the active session
        if let Some(index) = self.active_index {
            if let Some(curr_session) = self.sessions.get_mut(index) {
                curr_session.show_central_panel(ctx, self.bytes_per_row);
            }
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label("Drop a file or click '+' to start hexing!");
                });
            });
        }
    }
}

fn load_icon() -> egui::IconData {
    #[cfg(target_os = "macos")]
    const ICON_RGBA: &[u8] = include_bytes!("../assets/icon_mac.rgba");
    #[cfg(not(target_os = "macos"))]
    const ICON_RGBA: &[u8] = include_bytes!("../assets/icon_win.rgba");

    egui::IconData {
        rgba: ICON_RGBA.to_vec(),
        width: 128,
        height: 128,
    }
}
