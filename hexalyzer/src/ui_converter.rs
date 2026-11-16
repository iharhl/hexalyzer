use crate::hex_input::{
    filter_digits, parse_hex_pattern, sanitize_hex_pattern_input, strip_radix_prefixes,
};
use eframe::egui;

/// Converter input field
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Hex,
    Dec,
    Bin,
    Ascii,
}

impl Field {
    const ALL: [Self; 4] = [Self::Hex, Self::Dec, Self::Bin, Self::Ascii];

    const fn label(self) -> &'static str {
        match self {
            Self::Hex => "Hex:",
            Self::Dec => "Dec:",
            Self::Bin => "Bin:",
            Self::Ascii => "ASCII:",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Hex => "Hex",
            Self::Dec => "Dec",
            Self::Bin => "Bin",
            Self::Ascii => "ASCII",
        }
    }

    const fn hint(self) -> &'static str {
        match self {
            Self::Hex => "F1 BC CA E2",
            Self::Dec => "-1234",
            Self::Bin => "1010 0101",
            Self::Ascii => "text, \\x0A",
        }
    }

    /// Clean up text typed or pasted into this field
    fn filter(self, s: &mut String) {
        match self {
            Self::Hex => sanitize_hex_pattern_input(s),
            Self::Bin => *s = filter_digits(&strip_radix_prefixes(s, 'b'), 2),
            // A leading sign is kept: it switches `Signed` (see `apply_dec_sign`)
            Self::Dec => {
                let sign = s
                    .trim_start()
                    .chars()
                    .next()
                    .filter(|c| matches!(c, '-' | '+'));
                s.retain(|c| c.is_ascii_digit());
                if let Some(sign) = sign {
                    s.insert(0, sign);
                }
            }
            Self::Ascii => {}
        }
    }
}

/// Byte order used to read the bytes as a number
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Endian {
    #[default]
    Big,
    Little,
}

/// Message shown under the input rows
enum Status {
    Error(String),
    Warning(String),
}

impl Status {
    /// Prefix the message with the name of the field it is about
    fn for_field(self, field: Field) -> Self {
        match self {
            Self::Error(msg) => Self::Error(format!("{}: {msg}", field.name())),
            Self::Warning(msg) => Self::Warning(format!("{}: {msg}", field.name())),
        }
    }
}

// TODO: make it a standalone window that can go outside bounds
//       and indicates that it is in focus

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
    /// Field last edited by the user, re-converted when the options change
    source: Option<Field>,
    status: Option<Status>,
    /// Whether the last click landed in the converter window. Stays set until the user
    /// clicks elsewhere, so keys typed into the converter never reach the main view.
    focused: bool,
}

impl HexConverter {
    /// Returns true if the last click landed in the converter window
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
            .default_size([420.0, 230.0])
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        self.show_contents(ui);
                    });
            });
        self.active = is_open;

        // Focus follows the last click: inside the window -> converter, elsewhere -> main app
        if let Some(press_pos) = ctx.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.press_origin())
                .flatten()
        }) {
            self.focused =
                response.is_some_and(|r| ctx.layer_id_at(press_pos) == Some(r.response.layer_id));
        }
        if !self.active {
            self.focused = false;
        }
    }

    fn show_contents(&mut self, ui: &mut egui::Ui) {
        if self.show_options_row(ui)
            && let Some(source) = self.source
        {
            self.convert_from(source);
        }
        ui.add_space(4.0);
        self.show_input_rows(ui);
        self.show_status_row(ui);
    }

    /// Returns true if an option changed
    fn show_options_row(&mut self, ui: &mut egui::Ui) -> bool {
        ui.horizontal(|ui| {
            let mut changed = false;
            ui.label("Endian:").on_hover_text(
                "Byte order of the number in Dec and Bin.\n\
                     Little: Hex FF 00 = Dec 255",
            );
            changed |= ui
                .radio_value(&mut self.endian, Endian::Big, "Big")
                .changed();
            changed |= ui
                .radio_value(&mut self.endian, Endian::Little, "Little")
                .changed();
            ui.add_space(16.0);
            changed |= ui
                .checkbox(&mut self.signed, "Signed")
                .on_hover_text(
                    "Dec is a two's complement number.\n\
                     Hex FF = Dec -1, and Dec 200 = Hex 00 C8.\n\
                     Type - or + in front of Dec to switch it on or off.",
                )
                .changed();
            changed
        })
        .inner
    }

    fn show_input_rows(&mut self, ui: &mut egui::Ui) {
        let label_w = 50.0;
        let row_h = ui.spacing().interact_size.y;
        let btn_size = egui::vec2(32.0, row_h);
        let copy_icon = "\u{1f5d0}";
        let mut edited = None;

        for field in Field::ALL {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [label_w, row_h],
                    egui::Label::new(field.label()).halign(egui::Align::RIGHT),
                );
                let text = self.text_mut(field);
                let button_w = btn_size.x + ui.spacing().item_spacing.x;
                let resp = ui.add_sized(
                    [ui.available_width() - button_w, row_h],
                    egui::TextEdit::singleline(text)
                        .font(egui::TextStyle::Monospace)
                        .hint_text(field.hint()),
                );
                if resp.changed() {
                    field.filter(text);
                    edited = Some(field);
                }
                if ui
                    .add_enabled_ui(!text.is_empty(), |ui| {
                        ui.add_sized(btn_size, egui::Button::new(copy_icon))
                            .on_hover_text("Copy")
                    })
                    .inner
                    .clicked()
                {
                    ui.ctx().copy_text(text.clone());
                }
            });
        }

        if let Some(field) = edited {
            if field == Field::Dec {
                self.apply_dec_sign();
            }
            self.source = Some(field);
            self.convert_from(field);
        }
    }

    /// A sign typed in front of Dec switches `Signed`: `-` on, `+` off.
    /// The `+` is only a switch, so it is removed from the field.
    fn apply_dec_sign(&mut self) {
        if let Some(rest) = self.dec.strip_prefix('+') {
            self.dec = rest.to_string();
            self.signed = false;
        } else if self.dec.starts_with('-') {
            self.signed = true;
        }
    }

    fn show_status_row(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        // Reserve the row height, so the window does not jump when a message appears
        ui.horizontal(|ui| {
            ui.set_min_height(ui.spacing().interact_size.y);
            match &self.status {
                Some(Status::Error(msg)) => {
                    ui.colored_label(ui.visuals().error_fg_color, msg);
                }
                Some(Status::Warning(msg)) => {
                    ui.colored_label(ui.visuals().warn_fg_color, msg);
                }
                None => {}
            }
        });
    }

    const fn text_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Hex => &mut self.hex,
            Field::Dec => &mut self.dec,
            Field::Bin => &mut self.bin,
            Field::Ascii => &mut self.ascii,
        }
    }

    /// Parse the `source` field and fill in the other three using the current
    /// endian/signed settings. On invalid input the other fields are cleared and an
    /// error is shown, so they never display a stale value.
    fn convert_from(&mut self, source: Field) {
        self.status = None;

        let text = self.text_mut(source).clone();
        // Nothing to convert yet ('-' is a negative number being typed)
        let blank = match source {
            Field::Ascii => text.is_empty(),
            Field::Dec => matches!(text.trim(), "" | "-"),
            Field::Hex | Field::Bin => text.trim().is_empty(),
        };
        let parsed = if blank {
            Ok(None)
        } else {
            match source {
                // The filter leaves only digits and spaces: odd-length groups are the
                // only invalid input, and they are common while typing -> a warning
                Field::Hex => parse_hex_pattern(&text).ok_or_else(|| {
                    Status::Warning("odd-length group is ambiguous (ABC: 0A BC or AB 0C?)".into())
                }),
                Field::Ascii => parse_ascii(&text).map_err(Status::Error),
                // Numbers are parsed MSB first, then put in memory order
                Field::Dec => parse_dec(&text, self.signed)
                    .map(|b| to_endian(b, self.endian))
                    .map_err(Status::Error),
                Field::Bin => parse_bin(&text)
                    .map(|b| to_endian(b, self.endian))
                    .map_err(Status::Error),
            }
            .map(Some)
        };

        let bytes = match parsed {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                self.clear_except(source);
                return;
            }
            Err(status) => {
                self.clear_except(source);
                self.status = Some(status.for_field(source));
                return;
            }
        };

        // The number the bytes form, MSB first
        let numeric = to_endian(bytes.clone(), self.endian);
        if source != Field::Hex {
            self.hex = format_as_hex(&bytes);
        }
        if source != Field::Dec {
            if let Some(dec) = format_as_dec(&numeric, self.signed) {
                self.dec = dec;
            } else {
                self.dec.clear();
                self.status = Some(Status::Warning(format!(
                    "Dec: {} bytes exceed the 16-byte limit",
                    bytes.len()
                )));
            }
        }
        if source != Field::Bin {
            self.bin = format_as_bin(&numeric);
        }
        if source != Field::Ascii {
            self.ascii = format_as_ascii(&bytes);
        }
    }

    fn clear_except(&mut self, source: Field) {
        for field in Field::ALL {
            if field != source {
                self.text_mut(field).clear();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing: string -> bytes
// ---------------------------------------------------------------------------

/// Parse a decimal number into its shortest big-endian (MSB first) representation:
/// unsigned, or two's complement when `signed`.
fn parse_dec(s: &str, signed: bool) -> Result<Vec<u8>, String> {
    let trimmed = s.trim();
    let (negative, digits) = trimmed
        .strip_prefix('-')
        .map_or((false, trimmed), |rest| (true, rest));
    if digits.is_empty() {
        return Err("no digits".to_string());
    }
    if let Some(c) = digits.chars().find(|c| !c.is_ascii_digit()) {
        return Err(format!("invalid digit '{c}'"));
    }
    if negative && !signed {
        return Err("negative values need 'Signed'".to_string());
    }
    let magnitude: u128 = digits
        .parse()
        .map_err(|_| "value exceeds 128 bits".to_string())?;

    if !signed {
        return Ok(trim_unsigned_be_bytes(&magnitude.to_be_bytes()));
    }
    let value = if negative {
        0i128.checked_sub_unsigned(magnitude)
    } else {
        i128::try_from(magnitude).ok()
    }
    .ok_or_else(|| "value exceeds signed 128 bits".to_string())?;
    Ok(trim_signed_be_bytes(&value.to_be_bytes()))
}

/// Parse binary into big-endian (MSB first) bytes. Whitespace is ignored and the
/// width is kept: `"0000 0001 0000 0000"` -> `[01, 00]`, `"1"` -> `[01]`.
fn parse_bin(s: &str) -> Result<Vec<u8>, String> {
    let bits: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(c) = bits.iter().find(|c| !matches!(c, '0' | '1')) {
        return Err(format!("invalid binary digit '{c}'"));
    }
    // Leading bits of an incomplete byte form the first byte
    let head = bits.len() % 8;
    let mut bytes = Vec::with_capacity(bits.len().div_ceil(8));
    let mut chunks: Vec<&[char]> = Vec::new();
    if head != 0 {
        chunks.push(&bits[..head]);
    }
    chunks.extend(bits[head..].chunks(8));
    for chunk in chunks {
        let byte = chunk
            .iter()
            .fold(0u8, |acc, &c| (acc << 1) | u8::from(c == '1'));
        bytes.push(byte);
    }
    Ok(bytes)
}

/// Parse ASCII text into bytes. Escapes: `\xNN` (any byte), `\\`, `\n`, `\r`, `\t`, `\0`.
fn parse_ascii(s: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            if !c.is_ascii() {
                let mut utf8 = [0; 4];
                let escapes = format_as_ascii(c.encode_utf8(&mut utf8).as_bytes());
                return Err(format!("'{c}' is not ASCII, its UTF-8 bytes are {escapes}"));
            }
            bytes.push(c as u8);
            continue;
        }
        let byte = match chars.next() {
            Some('\\') => b'\\',
            Some('n') => b'\n',
            Some('r') => b'\r',
            Some('t') => b'\t',
            Some('0') => 0,
            Some('x') => {
                let digits: String = chars.by_ref().take(2).collect();
                if digits.len() != 2 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err("\\x needs two hex digits, e.g. \\x0A".to_string());
                }
                u8::from_str_radix(&digits, 16).map_err(|e| e.to_string())?
            }
            Some(other) => return Err(format!("unknown escape '\\{other}'")),
            None => return Err("incomplete escape '\\'".to_string()),
        };
        bytes.push(byte);
    }
    Ok(bytes)
}

/// Drop leading zero bytes from a big-endian unsigned number, keeping at least one byte
fn trim_unsigned_be_bytes(bytes: &[u8]) -> Vec<u8> {
    let start = bytes
        .iter()
        .position(|&b| b != 0)
        .unwrap_or_else(|| bytes.len().saturating_sub(1));
    bytes[start..].to_vec()
}

/// Drop leading sign-extension bytes (`0x00` / `0xFF`) from a big-endian two's
/// complement number, keeping the sign bit so the value reads back the same
fn trim_signed_be_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut start = 0;
    while start + 1 < bytes.len()
        && matches!(bytes[start], 0x00 | 0xFF)
        && (bytes[start + 1] & 0x80) == (bytes[start] & 0x80)
    {
        start += 1;
    }
    bytes[start..].to_vec()
}

/// Convert between memory order and MSB-first order (the same swap both ways)
fn to_endian(mut bytes: Vec<u8>, endian: Endian) -> Vec<u8> {
    if endian == Endian::Little {
        bytes.reverse();
    }
    bytes
}

// ---------------------------------------------------------------------------
// Formatting: bytes -> string
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

/// Format a big-endian (MSB first) number as decimal, or `None` if it does not fit
/// in 128 bits
fn format_as_dec(numeric: &[u8], signed: bool) -> Option<String> {
    if numeric.len() > 16 {
        return None;
    }
    let negative = signed && numeric.first().is_some_and(|b| b & 0x80 != 0);
    let mut buf = [if negative { 0xFF } else { 0x00 }; 16];
    buf[16 - numeric.len()..].copy_from_slice(numeric);
    Some(if signed {
        i128::from_be_bytes(buf).to_string()
    } else {
        u128::from_be_bytes(buf).to_string()
    })
}

/// Format a big-endian (MSB first) number as binary, 8 digits per byte
fn format_as_bin(numeric: &[u8]) -> String {
    numeric
        .iter()
        .map(|b| format!("{b:08b}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format bytes as ASCII text; `\` and non-printable bytes are escaped as `\\` / `\xNN`
fn format_as_ascii(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut acc, &b| {
        match b {
            b'\\' => acc.push_str("\\\\"),
            b if b.is_ascii_graphic() || b == b' ' => acc.push(b as char),
            b => {
                let _ = write!(acc, "\\x{b:02X}");
            }
        }
        acc
    })
}
