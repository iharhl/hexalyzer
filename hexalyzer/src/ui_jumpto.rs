use crate::app::{EnterAction, HexSession, colors};
use crate::events::EventState;
use eframe::egui;

#[derive(Default)]
pub struct JumpTo {
    /// Address to jump to
    pub(crate) addr: Option<usize>,
    /// User input string
    input: String,
    /// Force to loose focus from the text field
    loose_focus: bool,
    /// Whether the last submitted input could not be parsed as an address
    invalid: bool,
}

/// Parse a user-typed hex address, e.g. `1F00`, `0x1F00`, ` 0X1f00 `
fn parse_hex_addr(input: &str) -> Option<usize> {
    let s = input.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    // `from_str_radix` accepts a leading sign -> not a valid address
    if s.starts_with(['+', '-']) {
        return None;
    }
    usize::from_str_radix(s, 16).ok()
}

impl JumpTo {
    pub(crate) const fn loose_focus(&mut self) {
        self.loose_focus = true;
    }
}

impl HexSession {
    /// Displays the `JumpTo` panel for jumping to a specific address.
    pub(crate) fn show_jumpto_contents(&mut self, ui: &mut egui::Ui, events: &EventState) {
        let textedit = ui.add(
            egui::TextEdit::singleline(&mut self.jump_to.input)
                .desired_width(ui.available_width() - 30.0),
        );

        ui.add_space(5.0);

        // Hide the error as soon as the user edits the input
        if textedit.changed() {
            self.jump_to.invalid = false;
        }

        // Display the parse error, or the current address / selection range
        if self.jump_to.invalid {
            ui.label(egui::RichText::new("Invalid address").color(colors::WARNING));
        } else if let Some(sel) = self.selection.range {
            if sel[0] == sel[1] {
                ui.label(format!("Current address: 0x{:X}", sel[0]));
            } else {
                let sel_min = sel[0].min(sel[1]);
                let sel_max = sel[0].max(sel[1]);
                ui.label(format!("Selected: 0x{sel_min:X} - 0x{sel_max:X}"));
            }
        } else {
            ui.label("Current address: --");
        }

        if self.jump_to.loose_focus {
            textedit.surrender_focus();
            self.jump_to.loose_focus = false;
        }

        if textedit.has_focus() {
            self.enter_action = Some(EnterAction::JumpTo);

            // Clear the selection to avoid modifying bytes
            // while typing in the jumpto area
            self.selection.clear();
        }

        if events.enter_released && self.enter_action == Some(EnterAction::JumpTo) {
            self.jump_to.addr = parse_hex_addr(&self.jump_to.input);
            self.jump_to.invalid =
                self.jump_to.addr.is_none() && !self.jump_to.input.trim().is_empty();

            // Select the byte we just jumped to
            if let Some(addr) = self.jump_to.addr
                && self.ih.read_byte(addr).is_some()
            {
                self.selection.update(addr);
            }
        }
    }
}
