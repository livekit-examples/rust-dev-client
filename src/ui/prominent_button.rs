use crate::style::Palette;
use egui::{Button, Response, RichText, Ui, Widget, vec2};

/// Widget: a filled, full-width call-to-action button — the accent token fill
/// with knockout (background-colored) text. Use [`Self::enabled`] to gate it;
/// when disabled egui dims it.
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
        let palette = Palette::for_theme(ui.theme());
        let button = Button::new(RichText::new(self.text).color(palette.bg_1))
            .fill(palette.fg_accent)
            .min_size(vec2(ui.available_width(), 32.0));
        ui.add_enabled(self.enabled, button)
    }
}
