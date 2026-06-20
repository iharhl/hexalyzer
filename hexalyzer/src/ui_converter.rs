use eframe::egui;

/// Which field to convert from
#[derive(Default, Clone, Copy, PartialEq)]
enum Field {
    #[default]
    Hex,
    Dec,
    Bin,
    Ascii,
}

/// Byte order for multi-byte numeric fields
#[derive(Default, Clone, Copy, PartialEq)]
enum Endian {
    #[default]
    Big,
    Little,
}

/// Standalone hex/dec/bin/ascii converter tool window
#[derive(Default)]
pub struct HexConverter {
    pub active: bool,
    hex: String,
    dec: String,
    bin: String,
    ascii: String,
    endian: Endian,
    signed: bool,
    /// Whether the converter window or its widgets had focus this frame
    focused: bool,
}

impl HexConverter {
    /// Returns true if the converter window was hovered or had keyboard focus
    /// during the last `show()` call
    pub const fn has_focus(&self) -> bool {
        self.focused
    }

    /// Render the converter window (if active)
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.active {
            self.focused = false;
            return;
        }

        let mut is_open = self.active;
        let response = egui::Window::new("Hex Converter")
            .open(&mut is_open)
            .collapsible(false)
            .resizable(true)
            .default_size([420.0, 210.0])
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        self.show_contents(ui);
                    });
            });
        self.active = is_open;
        self.focused = response.is_some_and(|r| r.response.hovered() || r.response.has_focus());
    }

    fn show_contents(&mut self, ui: &mut egui::Ui) {
        self.show_options_row(ui);
        ui.add_space(4.0);
        self.show_input_rows(ui);
    }

    fn show_options_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Endian:");
            ui.radio_value(&mut self.endian, Endian::Big, "Big");
            ui.radio_value(&mut self.endian, Endian::Little, "Little");
            ui.add_space(16.0);
            ui.checkbox(&mut self.signed, "Signed");
        });
    }

    fn show_input_rows(&mut self, ui: &mut egui::Ui) {
        let row_width = ui.available_width();
        let label_w = 50.0;
        let btn_w = 28.0;
        let row_h = ui.spacing().interact_size.y;
        let spacing = ui.spacing().item_spacing.x;
        let text_w = spacing.mul_add(-3.0, row_width - label_w) - btn_w * 2.0;
        let copy_icon = "\u{1f4cb}";
        let convert_icon = ">>";

        macro_rules! input_row {
            ($label:expr, $field:expr, $text:expr, $filter:expr) => {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [label_w, row_h],
                        egui::Label::new($label).halign(egui::Align::RIGHT),
                    );
                    let resp = ui.add_sized(
                        [text_w, row_h],
                        egui::TextEdit::singleline($text).font(egui::TextStyle::Monospace),
                    );
                    if resp.changed() {
                        $filter($text);
                    }
                    if ui
                        .add_sized([btn_w, row_h], egui::Button::new(copy_icon))
                        .on_hover_text("Copy")
                        .clicked()
                    {
                        ui.ctx().copy_text($text.clone());
                    }
                    if ui
                        .add_sized([btn_w, row_h], egui::Button::new(convert_icon))
                        .clicked()
                    {
                        self.convert_from($field);
                    }
                });
            };
        }

        input_row!("Hex:", Field::Hex, &mut self.hex, |s: &mut String| {
            s.retain(|c| c.is_ascii_hexdigit());
        });
        input_row!("Dec:", Field::Dec, &mut self.dec, |s: &mut String| {
            let has_minus = s.starts_with('-');
            s.retain(|c| c.is_ascii_digit());
            if has_minus {
                s.insert(0, '-');
            }
        });
        input_row!("Bin:", Field::Bin, &mut self.bin, |s: &mut String| {
            s.retain(|c| c == '0' || c == '1');
        });
        input_row!(
            "ASCII:",
            Field::Ascii,
            &mut self.ascii,
            |_s: &mut String| {}
        );
    }

    /// Parse `source`, convert to the other three fields using current
    /// endian/signed settings
    fn convert_from(&mut self, source: Field) {
        if source == Field::Dec && self.dec.trim_start().starts_with('-') {
            self.signed = true;
        }
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
            return;
        }

        let bytes = match source {
            Field::Hex => parse_hex(&self.hex),
            Field::Dec => parse_dec(&self.dec),
            Field::Bin => parse_bin(&self.bin),
            Field::Ascii => Some(self.ascii.bytes().collect()),
        };
        let Some(bytes) = bytes else { return };

        if source != Field::Hex {
            self.hex = format_as_hex(&bytes);
        }
        if source != Field::Dec {
            self.dec = format_as_dec(&bytes, self.endian, self.signed);
        }
        if source != Field::Bin {
            self.bin = format_as_bin(&bytes, self.endian);
        }
        if source != Field::Ascii {
            self.ascii = format_as_ascii(&bytes);
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing: string -> byte vec (big-endian order, no endian param)
// ---------------------------------------------------------------------------

/// Parse a hex string (e.g. "FF", "1a2B") into bytes.
/// Odd-length strings are padded with a leading "0".
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
/// Supports an optional leading '-' for negative values.
fn parse_dec(s: &str) -> Option<Vec<u8>> {
    let trimmed = s.trim();
    let (negative, digits) = trimmed
        .strip_prefix('-')
        .map_or((false, trimmed), |rest| (true, rest));
    let clean: String = digits.chars().filter(char::is_ascii_digit).collect();
    if clean.is_empty() {
        return None;
    }
    if negative {
        let val: i128 = clean.parse::<u128>().ok()?.try_into().ok()?;
        let val = -val;
        let bytes = val.to_be_bytes();
        Some(trim_negative_be_bytes(&bytes))
    } else {
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
}

/// Parse a binary string into big-endian bytes
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

/// Strip leading sign-extension bytes (`0xFF`) from a negative big-endian
/// two's-complement byte slice, keeping just enough bytes to preserve the sign.
fn trim_negative_be_bytes(bytes: &[u8]) -> Vec<u8> {
    let first_non_ff = bytes.iter().position(|&b| b != 0xFF).unwrap_or(bytes.len());
    if first_non_ff == bytes.len() {
        return vec![0xFF];
    }
    // Keep one extra 0xFF when the first non-sign byte has bit 7 set,
    // otherwise two, so `be_bytes_to_i128` reconstructs the correct value.
    let extra = if bytes[first_non_ff] & 0x80 != 0 {
        1
    } else {
        2
    };
    let keep = first_non_ff.saturating_sub(extra);
    bytes[keep..].to_vec()
}

// ---------------------------------------------------------------------------
// Formatting: byte vec -> string
// ---------------------------------------------------------------------------

/// Format bytes as uppercase hex pairs
fn format_as_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            let _ = write!(acc, "{b:02X}");
            acc
        })
}

/// Interpret `bytes` (big-endian) as a numeric value in the given
/// byte order, then format as decimal
fn format_as_dec(bytes: &[u8], endian: Endian, signed: bool) -> String {
    if bytes.is_empty() || bytes.len() > 16 {
        return String::new();
    }
    if signed {
        let val = be_bytes_to_i128(bytes, endian);
        return val.to_string();
    }
    be_bytes_to_u128(bytes, endian).map_or_else(String::new, |v| v.to_string())
}

/// Interpret `bytes` (big-endian) as a numeric value in the given
/// byte order, then format as binary
fn format_as_bin(bytes: &[u8], endian: Endian) -> String {
    be_bytes_to_u128(bytes, endian).map_or_else(String::new, |v| format!("{v:b}"))
}

/// Format bytes as an ASCII string; non-printable characters become '·'
fn format_as_ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '·'
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Numeric helpers: big-endian bytes <-> u128/i128
// ---------------------------------------------------------------------------

/// Convert big-endian canonical bytes to `u128` respecting endianness
fn be_bytes_to_u128(bytes: &[u8], endian: Endian) -> Option<u128> {
    if bytes.len() > 16 {
        return None;
    }
    let mut buf = [0u8; 16];
    match endian {
        Endian::Big => {
            buf[16 - bytes.len()..].copy_from_slice(bytes);
            Some(u128::from_be_bytes(buf))
        }
        Endian::Little => {
            // LE: first canonical byte is MSB -> goes at the high end.
            buf[..bytes.len()].copy_from_slice(bytes);
            Some(u128::from_le_bytes(buf))
        }
    }
}

/// Convert big-endian canonical bytes to `i128` respecting endianness
fn be_bytes_to_i128(bytes: &[u8], endian: Endian) -> i128 {
    let mut buf = [0u8; 16];
    // Fill leading bytes with 0xFF if MSB has bit 7 set, otherwise 0x00
    let sign = if bytes[0] & 0x80 != 0 { 0xFF } else { 0x00 };
    buf.fill(sign);
    match endian {
        Endian::Big => {
            buf[16 - bytes.len()..].copy_from_slice(bytes);
            i128::from_be_bytes(buf)
        }
        Endian::Little => {
            buf[..bytes.len()].copy_from_slice(bytes);
            i128::from_le_bytes(buf)
        }
    }
}
