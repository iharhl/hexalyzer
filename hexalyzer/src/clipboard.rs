use crate::app::{HexSession, HexViewerApp};
use eframe::egui;
use std::fmt::Write;

impl HexViewerApp {
    /// Intercepts copy shortcuts (⌘C / Ctrl+C for hex, Shift+⌘C / Ctrl+Shift+C for ASCII)
    /// at the start of the frame, before any widget can consume them.
    pub(crate) fn handle_copy_shortcut(&self, ctx: &egui::Context) {
        let (copy_hex, copy_ascii) = ctx.input_mut(|i| {
            let mut hex = false;
            let mut ascii = false;

            // Check if the OS generated native Copy event
            let has_copy_event = i.events.iter().any(|e| matches!(e, egui::Event::Copy));

            if has_copy_event {
                // Look at the modifier keys active right now
                if i.modifiers.command && i.modifiers.shift {
                    ascii = true;
                } else if i.modifiers.command {
                    hex = true;
                }

                // Prevent the event from bubbling down to focused text inputs
                i.events.retain(|e| !matches!(e, egui::Event::Copy));
            }

            // Fallback: Catch raw key presses if the OS didn't detect it as Event::Copy
            if !hex && !ascii {
                let has_key = i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key {
                            key: egui::Key::C,
                            pressed: true,
                            ..
                        }
                    )
                });
                if has_key {
                    if i.modifiers.command && i.modifiers.shift {
                        ascii = true;
                    } else if i.modifiers.command {
                        hex = true;
                    }
                    // Clear the key press event
                    i.events.retain(|e| {
                        !matches!(
                            e,
                            egui::Event::Key {
                                key: egui::Key::C,
                                ..
                            }
                        )
                    });
                }
            }

            (hex, ascii)
        });

        if let Some(session) = self.get_curr_session() {
            if copy_hex && let Some(text) = session.selected_bytes_as_hex() {
                ctx.copy_text(text);
            } else if copy_ascii && let Some(text) = session.selected_bytes_as_ascii() {
                ctx.copy_text(text);
            }
        }
    }
}

impl HexSession {
    /// Returns the selected bytes formatted as a continuous hex string (e.g. `"48656C6C6F"`),
    /// or `None` if no selection exists or no bytes are present.
    pub(crate) fn selected_bytes_as_hex(&self) -> Option<String> {
        let sel = self.selection.range?;
        let min = *sel.iter().min()?;
        let max = *sel.iter().max()?;
        let mut hex = String::with_capacity((max - min + 1) * 2);
        for addr in min..=max {
            if let Some(b) = self.ih.read_byte(addr) {
                let _ = write!(hex, "{b:02X}");
            }
        }
        if hex.is_empty() { None } else { Some(hex) }
    }

    /// Returns the selected bytes as ASCII text (printable chars kept, non-printable → `.`,
    /// absent addresses → space), or `None` if no selection exists.
    pub(crate) fn selected_bytes_as_ascii(&self) -> Option<String> {
        let sel = self.selection.range?;
        let min = *sel.iter().min()?;
        let max = *sel.iter().max()?;
        let mut ascii = String::with_capacity(max - min + 1);
        for addr in min..=max {
            match self.ih.read_byte(addr) {
                Some(b) if b.is_ascii_graphic() || b == b' ' => ascii.push(b as char),
                Some(_) => ascii.push('.'),
                None => ascii.push(' '),
            }
        }
        if ascii.is_empty() { None } else { Some(ascii) }
    }
}
