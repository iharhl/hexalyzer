use crate::app::{Endianness, HexSession};
use eframe::egui;
use eframe::egui::Ui;
use std::time::{Duration, Instant};

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

/// Format float for clipboard: no commas or thousands separators.
/// Uses scientific notation for extreme values.
fn format_float_plain<T: Into<f64>>(float_value: T) -> String {
    let f = float_value.into();
    let formatted = if f.abs() >= 1e8 || (f != 0.0 && f.abs() < 1e-7) {
        format!("{f:.17e}")
    } else {
        format!("{f:.17}")
    };
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

/// Format the float for inspector view
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

/// Format the unix timestamp (seconds since epoch) as a UTC string.
fn format_unix_timestamp(seconds: i64) -> Option<String> {
    // Limit range to years 1 to 9999:
    // Year 1-01-01 00:00:00 UTC is -62,135,596,800 seconds.
    // Year 9999-12-31 23:59:59 UTC is 253,402,300,800 seconds.
    if !(-62_135_596_800..=253_402_300_800).contains(&seconds) {
        return None;
    }

    let (secs, year_offset) = if seconds < 0 {
        let cycle_secs = 146_097 * 86_400; // 12,622,780,800
        let cycles = (-seconds / cycle_secs) + 1;
        (seconds + cycles * cycle_secs, -cycles * 400)
    } else {
        (seconds, 0)
    };

    let secs_in_day = 86_400;
    let days = secs / secs_in_day;
    let seconds_of_day = secs % secs_in_day;

    let hours = seconds_of_day / 3_600;
    let minutes = (seconds_of_day % 3_600) / 60;
    let s = seconds_of_day % 60;

    let mut year = 1970_i64;
    let mut remaining_days = days;

    loop {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = if is_leap {
        [31_i64, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31_i64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &d in &month_days {
        if remaining_days < d {
            break;
        }
        remaining_days -= d;
        month += 1;
    }

    let day = remaining_days + 1;
    let final_year = year + year_offset;

    Some(format!(
        "{final_year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{s:02} UTC"
    ))
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
                        let v = val_u8.to_string();
                        ui.label("u8");
                        copyable_value(ui, "u8", &v, &v);
                        ui.end_row();

                        let val_i8 = i8::from_le_bytes([bytes[0]]);
                        let v = val_i8.to_string();
                        ui.label("i8");
                        copyable_value(ui, "i8", &v, &v);
                        ui.end_row();

                        let val_bin = format!("{val_u8:08b}");
                        ui.label("bin");
                        copyable_value(ui, "bin", &val_bin, &val_bin);
                    }
                    2 => {
                        let val_u16 =
                            u16::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("u16");
                        copyable_value(
                            ui,
                            "u16",
                            &format_with_separators(val_u16),
                            &val_u16.to_string(),
                        );
                        ui.end_row();

                        let val_i16 =
                            i16::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("i16");
                        copyable_value(
                            ui,
                            "i16",
                            &format_with_separators(val_i16),
                            &val_i16.to_string(),
                        );
                        ui.end_row();

                        let val_bin = format!("{val_u16:016b}");
                        ui.label("bin");
                        copyable_value(ui, "bin", &val_bin, &val_bin);
                    }
                    4 => {
                        let val_u32 =
                            u32::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("u32");
                        copyable_value(
                            ui,
                            "u32",
                            &format_with_separators(val_u32),
                            &val_u32.to_string(),
                        );
                        ui.end_row();

                        let val_i32 =
                            i32::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("i32");
                        copyable_value(
                            ui,
                            "i32",
                            &format_with_separators(val_i32),
                            &val_i32.to_string(),
                        );
                        ui.end_row();

                        let val_f32 =
                            f32::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("f32");
                        copyable_value(
                            ui,
                            "f32",
                            &format_float(val_f32),
                            &format_float_plain(val_f32),
                        );
                        ui.end_row();

                        if val_i32 < 0 {
                            let ts_u32 =
                                format_unix_timestamp(i64::from(val_u32)).unwrap_or_default();
                            ui.label("epoch\n(u32)");
                            copyable_value(
                                ui,
                                "epoch (u32)",
                                &ts_u32.replacen(' ', "\n", 1),
                                ts_u32.trim_end_matches(" UTC"),
                            );
                            ui.end_row();

                            let ts_i32 =
                                format_unix_timestamp(i64::from(val_i32)).unwrap_or_default();
                            ui.label("epoch\n(i32)");
                            copyable_value(
                                ui,
                                "epoch (i32)",
                                &ts_i32.replacen(' ', "\n", 1),
                                ts_i32.trim_end_matches(" UTC"),
                            );
                        } else {
                            let ts = format_unix_timestamp(i64::from(val_u32)).unwrap_or_default();
                            ui.label("epoch");
                            copyable_value(ui, "epoch", &ts, ts.trim_end_matches(" UTC"));
                        }
                        ui.end_row();

                        let val_bin = format!("{val_u32:032b}");
                        let multiline = format!("{}\n{}", &val_bin[0..24], &val_bin[24..32]);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                            ui.label("bin");
                        });
                        copyable_value(ui, "bin", &multiline, &val_bin);
                    }
                    8 => {
                        let val_u64 =
                            u64::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("u64");
                        copyable_value(
                            ui,
                            "u64",
                            &format_with_separators(val_u64),
                            &val_u64.to_string(),
                        );
                        ui.end_row();

                        let val_i64 =
                            i64::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("i64");
                        copyable_value(
                            ui,
                            "i64",
                            &format_with_separators(val_i64),
                            &val_i64.to_string(),
                        );
                        ui.end_row();

                        let val_f64 =
                            f64::from_le_bytes(bytes.as_slice().try_into().unwrap_or_default());
                        ui.label("f64");
                        copyable_value(
                            ui,
                            "f64",
                            &format_float(val_f64),
                            &format_float_plain(val_f64),
                        );
                        ui.end_row();

                        let epoch_u64 = if val_u64 <= 253_402_300_800 {
                            i64::try_from(val_u64).ok().and_then(format_unix_timestamp)
                        } else {
                            None
                        };
                        let epoch_i64 = format_unix_timestamp(val_i64);

                        if let Some(u_str) = epoch_u64 {
                            ui.label("epoch");
                            copyable_value(ui, "epoch", &u_str, u_str.trim_end_matches(" UTC"));
                            ui.end_row();
                        } else if let Some(i_str) = epoch_i64 {
                            ui.label("epoch");
                            copyable_value(ui, "epoch", &i_str, i_str.trim_end_matches(" UTC"));
                            ui.end_row();
                        }

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
                        copyable_value(ui, "bin", &multiline, &val_bin);
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

/// Renders a clickable label that copies `copy` text to clipboard on click.
/// Displays a "Copied!" tooltip for 1.2 seconds after clicking, or "Click to copy" on hover.
fn copyable_value(ui: &mut Ui, type_label: &str, display: &str, copy: &str) {
    let response = ui.add(
        egui::Label::new(display)
            .selectable(false)
            .sense(egui::Sense::click()),
    );

    let clicked = response.clicked();

    let id = ui.id().with(type_label).with("copied");
    let copied_at: Option<Instant> = ui.data(|d| d.get_temp(id));
    let is_recently_copied = copied_at.is_some_and(|t| t.elapsed() < Duration::from_secs_f32(1.2));

    if is_recently_copied {
        response.show_tooltip_text("Copied!");
        response.on_hover_cursor(egui::CursorIcon::PointingHand);
    } else {
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Click to copy");
    }

    if clicked {
        ui.ctx().copy_text(copy.to_string());
        ui.data_mut(|d| d.insert_temp(id, Instant::now()));
        ui.ctx().request_repaint_after(Duration::from_secs_f32(1.2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_unix_timestamp() {
        assert_eq!(
            format_unix_timestamp(0),
            Some("1970-01-01 00:00:00 UTC".to_string())
        );
        assert_eq!(
            format_unix_timestamp(86_400),
            Some("1970-01-02 00:00:00 UTC".to_string())
        );
        assert_eq!(
            format_unix_timestamp(31_536_000),
            Some("1971-01-01 00:00:00 UTC".to_string())
        );
        assert_eq!(
            format_unix_timestamp(63_072_000),
            Some("1972-01-01 00:00:00 UTC".to_string())
        );
        assert_eq!(
            format_unix_timestamp(2_147_483_647),
            Some("2038-01-19 03:14:07 UTC".to_string())
        );
        assert_eq!(
            format_unix_timestamp(4_294_967_295),
            Some("2106-02-07 06:28:15 UTC".to_string())
        );
        assert_eq!(
            format_unix_timestamp(-2_147_483_648),
            Some("1901-12-13 20:45:52 UTC".to_string())
        );
        assert_eq!(
            format_unix_timestamp(-1),
            Some("1969-12-31 23:59:59 UTC".to_string())
        );
        assert_eq!(
            format_unix_timestamp(1_718_150_400),
            Some("2024-06-12 00:00:00 UTC".to_string())
        );
        assert_eq!(format_unix_timestamp(-62_135_596_801), None);
        assert_eq!(format_unix_timestamp(253_402_300_801), None);
    }
}
