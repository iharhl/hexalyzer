use eframe::egui;

pub(crate) struct EventManager {}

impl EventManager {
    /// Helper for mapping keys to hex chars
    fn key_to_hex_char(key: egui::Key) -> Option<char> {
        use egui::Key::*;
        Some(match key {
            Num0 => '0',
            Num1 => '1',
            Num2 => '2',
            Num3 => '3',
            Num4 => '4',
            Num5 => '5',
            Num6 => '6',
            Num7 => '7',
            Num8 => '8',
            Num9 => '9',
            A => 'A',
            B => 'B',
            C => 'C',
            D => 'D',
            E => 'E',
            F => 'F',
            _ => return None,
        })
    }

    pub(crate) fn get_keyboard_input_char(ui: &egui::Ui) -> Option<char> {
        ui.input(|i| {
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: false,
                    ..
                } = event
                    && let Some(ch) = Self::key_to_hex_char(*key)
                {
                    return Some(ch);
                }
            }
            None
        })
    }

    pub(crate) fn get_keyboard_input_key(ui: &egui::Ui) -> Option<egui::Key> {
        ui.input(|i| {
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: false,
                    ..
                } = event
                {
                    return Some(*key);
                }
            }
            None
        })
    }

    pub(crate) fn is_pointer_down(ui: &egui::Ui) -> bool {
        ui.input(|i| i.pointer.primary_down())
    }

    pub(crate) fn get_pointer_hover(ui: &egui::Ui) -> Option<egui::Pos2> {
        ui.input(|i| i.pointer.hover_pos())
    }
}
