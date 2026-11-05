use egui::{Button, Key, Response, TextEdit, Ui, Widget};

const SEND_BUTTON_WIDTH: f32 = 60.0;

/// Widget: a single-line message input with a Send button. Enter or Send moves
/// the trimmed draft into `submitted` and keeps the input focused.
pub struct ChatComposer<'a> {
    draft: &'a mut String,
    submitted: &'a mut Option<String>,
    enabled: bool,
    hint_text: &'a str,
}

impl<'a> ChatComposer<'a> {
    pub fn new(draft: &'a mut String, submitted: &'a mut Option<String>) -> Self {
        Self {
            draft,
            submitted,
            enabled: true,
            hint_text: "",
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn hint_text(mut self, hint_text: &'a str) -> Self {
        self.hint_text = hint_text;
        self
    }
}

impl Widget for ChatComposer<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.horizontal(|ui| {
            let width = ui.available_width() - SEND_BUTTON_WIDTH - ui.spacing().item_spacing.x;
            let edit = ui.add_enabled(
                self.enabled,
                TextEdit::singleline(self.draft)
                    .hint_text(self.hint_text)
                    .desired_width(width),
            );
            let entered = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let has_text = !self.draft.trim().is_empty();
            let send = Button::new("Send").min_size(egui::vec2(SEND_BUTTON_WIDTH, 0.0));
            let clicked = ui.add_enabled(self.enabled && has_text, send).clicked();
            if (entered || clicked) && has_text {
                *self.submitted = Some(self.draft.trim().to_owned());
                self.draft.clear();
                edit.request_focus();
            }
        })
        .response
    }
}
