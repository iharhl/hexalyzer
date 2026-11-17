use eframe::egui;

#[derive(Default, Clone, Copy)]
pub struct PointerState {
    pub(crate) pointer_hover: Option<egui::Pos2>,
    pub(crate) pointer_down: bool,
}

#[derive(Default, Clone)]
pub struct EventState {
    pub(crate) hex_chars: Vec<char>,
    pub(crate) pointer_state: PointerState,
    pub(crate) escape_pressed: bool,
    pub(crate) shift_down: bool,
    /// Arrow key presses this frame (key-repeats included)
    pub(crate) arrow_keys_pressed: Vec<egui::Key>,
}

/// Collect events once per frame from an `egui::Ui` reference and return an aggregated state.
/// **Note**: currently unused; might be useful in the future.
#[allow(dead_code)]
pub fn collect_ui_events(ui: &egui::Ui) -> EventState {
    let text_edit_focused = ui.ctx().text_edit_focused();
    ui.input(|i| collect_inner(i, text_edit_focused))
}

/// Collect events once per frame from an `egui::Context` reference and return an aggregated state.
pub fn collect_ui_events_ctx(ctx: &egui::Context) -> EventState {
    let text_edit_focused = ctx.text_edit_focused();
    ctx.input(|i| collect_inner(i, text_edit_focused))
}

fn collect_inner(i: &egui::InputState, text_edit_focused: bool) -> EventState {
    let mut state = EventState {
        pointer_state: PointerState {
            pointer_hover: i.pointer.hover_pos(),
            pointer_down: i.pointer.primary_down(),
        },
        shift_down: i.modifiers.shift,
        ..Default::default()
    };

    // Whether the last key press had a shortcut modifier held
    let mut shortcut_key_pressed = false;

    for event in &i.events {
        if let egui::Event::Key {
            pressed: true,
            modifiers,
            ..
        } = event
        {
            shortcut_key_pressed = modifiers.alt || modifiers.ctrl || modifiers.command;
        }

        match event {
            // Skip text typed with Alt / Ctrl / Cmd held, or into a text field
            // (e.g. the jump-to or search field)
            egui::Event::Text(text) if !shortcut_key_pressed && !text_edit_focused => {
                state
                    .hex_chars
                    .extend(text.chars().filter(char::is_ascii_hexdigit));
            }
            // Arrow key presses (key-repeats included, so holding a key keeps moving).
            // Skipped while a text field has focus (arrows move its text cursor there)
            // and when combined with Cmd / Ctrl / Alt (reserved for shortcuts).
            egui::Event::Key {
                key:
                    key @ (egui::Key::ArrowLeft
                    | egui::Key::ArrowRight
                    | egui::Key::ArrowUp
                    | egui::Key::ArrowDown),
                pressed: true,
                modifiers,
                ..
            } if !text_edit_focused && !modifiers.command && !modifiers.ctrl && !modifiers.alt => {
                state.arrow_keys_pressed.push(*key);
            }
            _ => {}
        }
    }

    // Direct query for Escape pressed this frame
    state.escape_pressed = i.key_pressed(egui::Key::Escape);

    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers};

    /// A key press event
    const fn key_press(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Collect the events of one frame with `modifiers` held
    fn collect(events: Vec<Event>, modifiers: Modifiers, text_edit_focused: bool) -> EventState {
        let ctx = egui::Context::default();
        let mut all_events = vec![Event::ModifiersChanged(modifiers)];
        all_events.extend(events);
        ctx.begin_pass(egui::RawInput {
            events: all_events,
            ..Default::default()
        });
        let state = ctx.input(|i| collect_inner(i, text_edit_focused));
        // Font textures are not uploaded anywhere (egui asserts they were handled)
        ctx.end_pass().textures_delta.clear();
        state
    }

    /// A key press followed by the text it types, as egui-winit sends them
    fn typed(key: Key, modifiers: Modifiers, text: &str) -> [Event; 2] {
        [key_press(key, modifiers), Event::Text(text.to_string())]
    }

    // ============================== hex chars =============================

    #[test]
    fn test_hex_chars_typed() {
        // Arrange
        let events = [
            typed(Key::A, Modifiers::NONE, "a"),
            typed(Key::Num1, Modifiers::NONE, "1"),
        ]
        .concat();

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert_eq!(state.hex_chars, ['a', '1']);
    }

    #[test]
    fn test_non_hex_chars_dropped() {
        // Arrange
        let events = vec![Event::Text("g z-A.".to_string())];

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert_eq!(state.hex_chars, ['A']);
    }

    #[test]
    fn test_shift_typed_chars_kept() {
        // Arrange
        let events = typed(Key::F, Modifiers::SHIFT, "F").to_vec();

        // Act
        let state = collect(events, Modifiers::SHIFT, false);

        // Assert
        assert_eq!(state.hex_chars, ['F']);
    }

    #[test]
    fn test_shortcut_text_dropped() {
        // Arrange: e.g. Alt+C, Ctrl+A, Cmd+B also produce text on some platforms
        let events = [
            typed(Key::C, Modifiers::ALT, "c"),
            typed(Key::A, Modifiers::CTRL, "a"),
            typed(Key::B, Modifiers::COMMAND, "b"),
        ]
        .concat();

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert_eq!(state.hex_chars, Vec::<char>::new());
    }

    #[test]
    fn test_text_after_shortcut_kept() {
        // Arrange: a shortcut, then a plain key in the same frame
        let events = [
            typed(Key::C, Modifiers::ALT, "c"),
            typed(Key::B, Modifiers::NONE, "b"),
        ]
        .concat();

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert_eq!(state.hex_chars, ['b']);
    }

    #[test]
    fn test_text_into_focused_text_field_dropped() {
        // Arrange
        let events = typed(Key::A, Modifiers::NONE, "a").to_vec();

        // Act
        let state = collect(events, Modifiers::NONE, true);

        // Assert
        assert_eq!(state.hex_chars, Vec::<char>::new());
    }

    // ============================= arrow keys =============================

    #[test]
    fn test_arrow_keys_in_order_with_repeats() {
        // Arrange
        let repeat = Event::Key {
            key: Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        };
        let events = vec![
            key_press(Key::ArrowRight, Modifiers::NONE),
            repeat,
            key_press(Key::ArrowDown, Modifiers::NONE),
            key_press(Key::ArrowLeft, Modifiers::NONE),
            key_press(Key::ArrowUp, Modifiers::NONE),
        ];

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert_eq!(
            state.arrow_keys_pressed,
            [
                Key::ArrowRight,
                Key::ArrowRight,
                Key::ArrowDown,
                Key::ArrowLeft,
                Key::ArrowUp
            ]
        );
    }

    #[test]
    fn test_arrow_key_release_ignored() {
        // Arrange
        let events = vec![Event::Key {
            key: Key::ArrowLeft,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }];

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert_eq!(state.arrow_keys_pressed, Vec::<Key>::new());
    }

    #[test]
    fn test_arrow_keys_with_shortcut_modifier_ignored() {
        // Arrange
        let events = [Modifiers::ALT, Modifiers::CTRL, Modifiers::COMMAND]
            .map(|mods| key_press(Key::ArrowLeft, mods))
            .to_vec();

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert_eq!(state.arrow_keys_pressed, Vec::<Key>::new());
    }

    #[test]
    fn test_arrow_keys_with_shift_kept() {
        // Arrange
        let events = vec![key_press(Key::ArrowLeft, Modifiers::SHIFT)];

        // Act
        let state = collect(events, Modifiers::SHIFT, false);

        // Assert
        assert_eq!(state.arrow_keys_pressed, [Key::ArrowLeft]);
    }

    #[test]
    fn test_arrow_keys_in_focused_text_field_ignored() {
        // Arrange
        let events = vec![key_press(Key::ArrowLeft, Modifiers::NONE)];

        // Act
        let state = collect(events, Modifiers::NONE, true);

        // Assert
        assert_eq!(state.arrow_keys_pressed, Vec::<Key>::new());
    }

    // ========================= escape, shift, pointer =====================

    #[test]
    fn test_escape_pressed() {
        // Arrange
        let events = vec![key_press(Key::Escape, Modifiers::NONE)];

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert!(state.escape_pressed);
    }

    #[test]
    fn test_no_input() {
        // Arrange
        let events = Vec::new();

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert_eq!(state.hex_chars, Vec::<char>::new());
        assert_eq!(state.arrow_keys_pressed, Vec::<Key>::new());
        assert!(!state.escape_pressed);
        assert!(!state.shift_down);
        assert!(!state.pointer_state.pointer_down);
    }

    #[test]
    fn test_shift_down() {
        // Arrange
        let events = Vec::new();

        // Act
        let state = collect(events, Modifiers::SHIFT, false);

        // Assert
        assert!(state.shift_down);
    }

    #[test]
    fn test_pointer_hover_and_down() {
        // Arrange
        let pos = egui::pos2(12.0, 34.0);
        let events = vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ];

        // Act
        let state = collect(events, Modifiers::NONE, false);

        // Assert
        assert_eq!(state.pointer_state.pointer_hover, Some(pos));
        assert!(state.pointer_state.pointer_down);
    }
}
