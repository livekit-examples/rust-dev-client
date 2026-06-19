use egui::{Button, Color32, Response, RichText, Ui, Widget, vec2};

/// Widget: a filled, full-width call-to-action button (white text on a blue
/// fill). Use [`Self::enabled`] to gate it; when disabled egui dims it.
pub struct ProminentButton {
    text: String,
    enabled: bool,
}

impl ProminentButton {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            enabled: true,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for ProminentButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let button = Button::new(RichText::new(self.text).color(Color32::WHITE))
            .fill(Color32::from_rgb(40, 120, 200))
            .min_size(vec2(ui.available_width(), 32.0));
        ui.add_enabled(self.enabled, button)
    }
}
