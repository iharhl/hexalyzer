use crate::app::{Endianness, HexSession};
use eframe::egui;
use eframe::egui::Ui;

#[allow(clippy::needless_pass_by_value)]
/// Format the number so that it has separators (for readability)
pub fn format_with_separators<T: ToString>(n: T) -> String {
    let s = n.to_string();
    let mut result = String::new();

    // Consider negative sign in front of digits
    let (sign, digits) = s
        .strip_prefix('-')
        .map_or(("", s.as_str()), |stripped| ("-", stripped));

    for (idx, ch) in digits.chars().rev().enumerate() {
        if idx != 0 && idx % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, ch);
    }
    format!("{sign}{result}")
}

/// Format the float so that it is nicely presented
fn format_float<T: Into<f64>>(float_value: T) -> String {
    let f = float_value.into();

    // Decide between normal or scientific notation
    let formatted = if f.abs() >= 1e6 || (f != 0.0 && f.abs() < 1e-5) {
        format!("{f:.17e}") // scientific notation
    } else {
        format!("{f:.17}") // fixed decimal format
    };

    // Trim trailing zeros and possible trailing decimal point
    let trimmed = formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();

    // Split into integer + fractional parts
    let mut parts = trimmed.split('.');
    let int_part = parts.next().unwrap_or("0");
    let frac_part = parts.next().unwrap_or("");

    if frac_part.is_empty() {
        return format_with_separators(int_part);
    }
    format!("{}.{}", format_with_separators(int_part), frac_part)
}

impl HexSession {
    #[allow(clippy::similar_names, clippy::too_many_lines)]
    /// Displays the inspector panel for the selected data.
    pub(crate) fn show_data_inspector_contents(&mut self, ui: &mut Ui) {
        ui.radio_value(&mut self.endianness, Endianness::Little, "Little Endian");
        ui.radio_value(&mut self.endianness, Endianness::Big, "Big Endian");

        ui.add_space(5.0);
        ui.separator();

        egui::Grid::new("data_inspector_grid")
            .num_columns(2) // two columns: label & value
            .spacing([20.0, 4.0]) // horizontal & vertical spacing
            .show(ui, |ui| {
                ui.label("Type");
                ui.label("Value");
                ui.end_row();

                let Some(sel) = self.selection.range else {
                    ui.label("--");
                    ui.label("--");
                    ui.end_row();
                    return;
                };

                let Some(&min) = sel.iter().min() else {
                    return;
                };
                let Some(&max) = sel.iter().max() else {
                    return;
                };

                let mut bytes: Vec<u8> = Vec::new();
                for addr in min..=max {
                    if let Some(b) = self.ih.read_byte(addr) {
                        bytes.push(b);
                    }
                }

                if self.endianness == Endianness::Big && bytes.len() > 1 {
                    bytes.reverse();
                }

                match bytes.len() {
                    1 => {
                        let val_u8 = u8::from_le_bytes([bytes[0]]);
                        ui.label("u8");
                        ui.label(val_u8.to_string());
                        ui.end_row();
                        let val_i8 = i8::from_le_bytes([bytes[0]]);
                        ui.label("i8");
                        ui.label(val_i8.to_string());
                        ui.end_row();
                        let val_bin = format!("{val_u8:08b}");
                        ui.label("bin");
                        ui.label(val_bin);
                    }
                    2 => {
                        let val_u16 =
                            u16::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("u16");
                        ui.label(format_with_separators(val_u16));
                        ui.end_row();
                        let val_i16 =
                            i16::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("i16");
                        ui.label(format_with_separators(val_i16));
                        ui.end_row();
                        let val_bin = format!("{val_u16:016b}");
                        ui.label("bin");
                        ui.label(val_bin);
                    }
                    4 => {
                        let val_u32 =
                            u32::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("u32");
                        ui.label(format_with_separators(val_u32));
                        ui.end_row();
                        let val_i32 =
                            i32::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("i32");
                        ui.label(format_with_separators(val_i32));
                        ui.end_row();
                        let val_f32 =
                            f32::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("f32");
                        ui.label(format_float(val_f32));
                        ui.end_row();
                        let val_bin = format!("{val_u32:032b}");
                        let multiline = format!("{}\n{}", &val_bin[0..24], &val_bin[24..32]);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                            ui.label("bin");
                        });
                        ui.label(multiline);
                    }
                    8 => {
                        let val_u64 =
                            u64::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("u64");
                        ui.label(format_with_separators(val_u64));
                        ui.end_row();
                        let val_i64 =
                            i64::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("i64");
                        ui.label(format_with_separators(val_i64));
                        ui.end_row();
                        let val_f64 =
                            f64::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("f64");
                        ui.label(format_float(val_f64));
                        ui.end_row();
                        let val_bin = format!("{val_u64:064b}");
                        let multiline = format!(
                            "{}\n{}\n{}",
                            &val_bin[0..24],
                            &val_bin[24..48],
                            &val_bin[48..64]
                        );
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                            ui.label("bin");
                        });
                        ui.label(multiline);
                    }
                    _ => {
                        ui.label("--");
                        ui.label("--");
                        ui.end_row();
                    }
                }
            });
    }
}
