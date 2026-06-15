use eframe::egui;

/// Which field the user last edited (source of conversion)
#[derive(Default, Clone, Copy, PartialEq)]
enum Field {
    #[default]
    Hex,
    Dec,
    Bin,
    Ascii,
}

/// Standalone hex/dec/bin/ascii converter tool window.
#[derive(Default)]
pub struct HexConverter {
    pub active: bool,
    hex: String,
    dec: String,
    bin: String,
    ascii: String,
}

impl HexConverter {
    /// Render the converter window (if active)
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.active {
            return;
        }

        let mut is_open = self.active;
        egui::Window::new("Hex Converter")
            .open(&mut is_open)
            .collapsible(false)
            .resizable(true)
            .default_size([380.0, 170.0])
            .show(ctx, |ui| {
                self.show_contents(ui);
            });
        self.active = is_open;
    }

    /// Render the four input rows + handle conversion on edit.
    fn show_contents(&mut self, ui: &mut egui::Ui) {
        // Track which field the user typed in this frame.
        let mut changed_field: Option<Field> = None;

        ui.add_space(4.0);

        // Layout per row: [label 50px] [spacing] [text field] [spacing] [copy button 28px]
        let row_width = ui.available_width();
        let label_w = 50.0;
        let btn_w = 28.0;
        let spacing = ui.spacing().item_spacing.x;
        let text_w = spacing.mul_add(-2.0, row_width - label_w - btn_w);
        let copy_btn = "📋";

        // HEX ROW
        ui.horizontal(|ui| {
            ui.add_sized([label_w, 0.0], egui::Label::new("Hex:"));
            let resp = ui.add_sized(
                [text_w, 0.0],
                egui::TextEdit::singleline(&mut self.hex).font(egui::TextStyle::Monospace),
            );
            if resp.changed() {
                // Strip non-hex characters
                self.hex.retain(|c| c.is_ascii_hexdigit());
                changed_field = Some(Field::Hex);
            }
            let btn_resp = ui.add_sized([btn_w, 0.0], egui::Button::new(copy_btn));
            if btn_resp.clicked() {
                ui.ctx().copy_text(self.hex.clone());
            }
        });

        // DEC ROW
        ui.horizontal(|ui| {
            ui.add_sized([label_w, 0.0], egui::Label::new("Dec:"));
            let resp = ui.add_sized(
                [text_w, 0.0],
                egui::TextEdit::singleline(&mut self.dec).font(egui::TextStyle::Monospace),
            );
            if resp.changed() && changed_field.is_none() {
                changed_field = Some(Field::Dec);
            }
            let btn_resp = ui.add_sized([btn_w, 0.0], egui::Button::new(copy_btn));
            if btn_resp.clicked() {
                ui.ctx().copy_text(self.dec.clone());
            }
        });

        // BIN ROW
        ui.horizontal(|ui| {
            ui.add_sized([label_w, 0.0], egui::Label::new("Bin:"));
            let resp = ui.add_sized(
                [text_w, 0.0],
                egui::TextEdit::singleline(&mut self.bin).font(egui::TextStyle::Monospace),
            );
            if resp.changed() && changed_field.is_none() {
                changed_field = Some(Field::Bin);
            }
            let btn_resp = ui.add_sized([btn_w, 0.0], egui::Button::new(copy_btn));
            if btn_resp.clicked() {
                ui.ctx().copy_text(self.bin.clone());
            }
        });

        // ASCII ROW
        ui.horizontal(|ui| {
            ui.add_sized([label_w, 0.0], egui::Label::new("ASCII:"));
            let resp = ui.add_sized(
                [text_w, 0.0],
                egui::TextEdit::singleline(&mut self.ascii).font(egui::TextStyle::Monospace),
            );
            if resp.changed() && changed_field.is_none() {
                changed_field = Some(Field::Ascii);
            }
            let btn_resp = ui.add_sized([btn_w, 0.0], egui::Button::new(copy_btn));
            if btn_resp.clicked() {
                ui.ctx().copy_text(self.ascii.clone());
            }
        });

        //
        if let Some(source) = changed_field {
            // If the edited field was completely cleared, reset every field
            let empty = match source {
                Field::Hex => self.hex.is_empty(),
                Field::Dec => self.dec.is_empty(),
                Field::Bin => self.bin.is_empty(),
                Field::Ascii => self.ascii.is_empty(),
            };
            if empty {
                self.hex.clear();
                self.dec.clear();
                self.bin.clear();
                self.ascii.clear();
            } else if let Some(bytes) = match source {
                Field::Hex => parse_hex(&self.hex),
                Field::Dec => parse_dec(&self.dec),
                Field::Bin => parse_bin(&self.bin),
                Field::Ascii => Some(self.ascii.bytes().collect()),
            } {
                // Update every field except the one the user is actively editing,
                // so their input isn't disrupted by reformatting.
                if source != Field::Hex {
                    self.hex = format_as_hex(&bytes);
                }
                if source != Field::Dec {
                    self.dec = format_as_dec(&bytes);
                }
                if source != Field::Bin {
                    self.bin = format_as_bin(&bytes);
                }
                if source != Field::Ascii {
                    self.ascii = format_as_ascii(&bytes);
                }
            }
            // If parsing fails (invalid input), leave all fields untouched.
        }
    }
}

/// Parse a hex string (e.g. "FF", "1a2B") into bytes.
/// Odd-length strings are padded with a leading "0" (e.g. "F" -> "0F").
/// Non-hex characters are ignored.
fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let clean: String = s.chars().filter(char::is_ascii_hexdigit).collect();
    if clean.is_empty() {
        return None;
    }
    let padded = if clean.len().is_multiple_of(2) {
        clean
    } else {
        format!("0{clean}")
    };
    (0..padded.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&padded[i..i + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

/// Parse a decimal string into big-endian bytes.
/// Non-digit characters are ignored.
fn parse_dec(s: &str) -> Option<Vec<u8>> {
    let clean: String = s.chars().filter(char::is_ascii_digit).collect();
    if clean.is_empty() {
        return None;
    }
    let val: u128 = clean.parse().ok()?;
    if val == 0 {
        return Some(vec![0]);
    }
    let mut v = val;
    let mut bytes = Vec::new();
    while v > 0 {
        bytes.push((v & 0xFF) as u8);
        v >>= 8;
    }
    bytes.reverse();
    Some(bytes)
}

/// Parse a binary string into big-endian bytes.
/// Non-0/1 characters are ignored.
fn parse_bin(s: &str) -> Option<Vec<u8>> {
    let clean: String = s.chars().filter(|c| *c == '0' || *c == '1').collect();
    if clean.is_empty() {
        return None;
    }
    let val = u128::from_str_radix(&clean, 2).ok()?;
    if val == 0 {
        return Some(vec![0]);
    }
    let mut v = val;
    let mut bytes = Vec::new();
    while v > 0 {
        bytes.push((v & 0xFF) as u8);
        v >>= 8;
    }
    bytes.reverse();
    Some(bytes)
}

/// Format bytes as uppercase hex pairs (e.g. [0xFF, 0x01] -> "FF01").
fn format_as_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            let _ = write!(acc, "{b:02X}");
            acc
        })
}

/// Interpret up to 16 bytes as a big-endian u128 (used by dec/bin formatting)
fn bytes_to_u128_be(bytes: &[u8]) -> Option<u128> {
    if bytes.len() > 16 {
        return None;
    }
    let mut buf = [0u8; 16];
    let start = 16 - bytes.len();
    buf[start..].copy_from_slice(bytes);
    Some(u128::from_be_bytes(buf))
}

/// Format bytes as a decimal string (big-endian u128, up to 16 bytes)
fn format_as_dec(bytes: &[u8]) -> String {
    bytes_to_u128_be(bytes).map_or_else(String::new, |v| v.to_string())
}

/// Format bytes as a binary string (big-endian u128, up to 16 bytes)
fn format_as_bin(bytes: &[u8]) -> String {
    bytes_to_u128_be(bytes).map_or_else(String::new, |v| format!("{v:b}"))
}

/// Format bytes as an ASCII string; non-printable characters become '.'
fn format_as_ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}
