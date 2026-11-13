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
    pub(crate) arrow_key_released: Option<egui::Key>,
}

/// Collect events once per frame from an `egui::Ui` reference and return an aggregated state.
/// **Note**: currently unused; might be useful in the future.
#[allow(dead_code)]
pub fn collect_ui_events(ui: &egui::Ui) -> EventState {
    ui.input(collect_inner)
}

/// Collect events once per frame from an `egui::Context` reference and return an aggregated state.
pub fn collect_ui_events_ctx(ctx: &egui::Context) -> EventState {
    ctx.input(collect_inner)
}

fn collect_inner(i: &egui::InputState) -> EventState {
    let mut state = EventState {
        pointer_state: PointerState {
            pointer_hover: i.pointer.hover_pos(),
            pointer_down: i.pointer.primary_down(),
        },
        shift_down: i.modifiers.shift,
        ..Default::default()
    };

    for event in &i.events {
        match event {
            // Take hex chars from text input (not raw keys), so that shortcuts like
            // Cmd+C never count as typing, even if Cmd is released before C
            egui::Event::Text(text) => {
                state
                    .hex_chars
                    .extend(text.chars().filter(char::is_ascii_hexdigit));
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

                // Store arrow keys release directly
                if matches!(
                    *key,
                    egui::Key::ArrowLeft
                        | egui::Key::ArrowRight
                        | egui::Key::ArrowUp
                        | egui::Key::ArrowDown
                ) {
                    state.arrow_key_released = Some(*key);
                }
            }
            _ => {}
        }
    }

    // Direct query for Escape pressed this frame
    state.escape_pressed = i.key_pressed(egui::Key::Escape);

    state
}
