use eframe::egui;

#[derive(Default, Clone, Copy)]
pub struct PointerState {
    pub(crate) pointer_hover: Option<egui::Pos2>,
    pub(crate) pointer_down: bool,
}

#[derive(Default, Clone)]
pub struct EventState {
    pub(crate) last_key_released: Option<egui::Key>,
    pub(crate) hex_chars: Vec<char>,
    pub(crate) pointer_state: PointerState,
    pub(crate) escape_pressed: bool,
    pub(crate) enter_released: bool,
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
            // Skip text typed with Alt / Ctrl / Cmd held
            egui::Event::Text(text) if !shortcut_key_pressed => {
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
            } => {
                if !text_edit_focused && !modifiers.command && !modifiers.ctrl && !modifiers.alt {
                    state.arrow_keys_pressed.push(*key);
                }
            }
            // Key release events
            egui::Event::Key {
                key,
                pressed: false,
                ..
            } => {
                state.last_key_released = Some(*key);

                // Store Enter key release directly
                if state.last_key_released == Some(egui::Key::Enter) {
                    state.enter_released = true;
                }
            }
            _ => {}
        }
    }

    // Direct query for Escape pressed this frame
    state.escape_pressed = i.key_pressed(egui::Key::Escape);

    state
}
