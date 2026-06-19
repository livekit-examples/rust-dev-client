use egui::{Response, TextEdit, Ui, Widget, WidgetText};

/// Widget: a text label paired with a single-line [`TextEdit`], wired together
/// with `labelled_by` for accessibility. By default the label sits above the
/// field (form style); [`Self::horizontal`] places it to the left.
///
/// Returns the *edit's* [`Response`] so callers can react to `changed()` /
/// `lost_focus()`.
pub struct LabeledTextEdit<'a> {
    label: WidgetText,
    text: &'a mut String,
    horizontal: bool,
    desired_width: Option<f32>,
    enabled: bool,
}

impl<'a> LabeledTextEdit<'a> {
    pub fn singleline(label: impl Into<WidgetText>, text: &'a mut String) -> Self {
        Self {
            label: label.into(),
            text,
            horizontal: false,
            desired_width: None,
            enabled: true,
        }
    }

    /// Lay the label out to the left of the field instead of above it.
    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    pub fn desired_width(mut self, width: f32) -> Self {
        self.desired_width = Some(width);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for LabeledTextEdit<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let add_contents = move |ui: &mut Ui| {
            let label = ui.label(self.label.small());
            let edit = TextEdit::singleline(self.text)
                .desired_width(self.desired_width.unwrap_or(f32::INFINITY));
            ui.add_enabled(self.enabled, edit).labelled_by(label.id)
        };

        if self.horizontal {
            ui.horizontal(add_contents).inner
        } else {
            ui.vertical(add_contents).inner
        }
    }
}
