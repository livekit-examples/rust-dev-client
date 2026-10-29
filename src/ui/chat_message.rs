use crate::style::Palette;
use egui::{Align, Frame, Layout, Response, RichText, Ui, Widget};

/// Widget: one chat message, the speaker above the text. The local user's
/// messages are right-aligned on the accent fill; pending text, still being
/// received (e.g. a growing transcript), is shown weak.
pub struct ChatMessage<'a> {
    speaker: &'a str,
    text: &'a str,
    is_local: bool,
    pending: bool,
}

impl<'a> ChatMessage<'a> {
    pub fn new(speaker: &'a str, text: &'a str) -> Self {
        Self {
            speaker,
            text,
            is_local: false,
            pending: false,
        }
    }

    pub fn local(mut self, is_local: bool) -> Self {
        self.is_local = is_local;
        self
    }

    /// Whether more text is still to come. Defaults to `false`.
    pub fn pending(mut self, pending: bool) -> Self {
        self.pending = pending;
        self
    }
}

impl Widget for ChatMessage<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let palette = Palette::for_theme(ui.theme());
        let (align, fill) = if self.is_local {
            (Align::Max, palette.bg_accent)
        } else {
            (Align::Min, palette.bg_3)
        };
        let max_width = ui.available_width() * 0.8;

        ui.with_layout(Layout::top_down(align), |ui| {
            Frame::new()
                .fill(fill)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_max_width(max_width);
                    ui.label(RichText::new(self.speaker).small().weak());
                    let text = RichText::new(self.text);
                    ui.label(if self.pending { text.weak() } else { text });
                });
        })
        .response
    }
}
