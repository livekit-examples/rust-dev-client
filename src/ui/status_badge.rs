use egui::{Color32, Response, RichText, Ui, Widget};

/// Semantic kind of a [`StatusBadge`]. Each kind owns its color so call sites
/// pick meaning (`ok`/`error`/`muted`/`neutral`) rather than a raw `Color32`,
/// which also consolidates the old GREEN-vs-LIGHT_GREEN / RED-vs-LIGHT_RED
/// inconsistencies without needing a separate theme module.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Error,
    Muted,
    Neutral,
}

impl Status {
    fn color(self) -> Color32 {
        match self {
            Status::Ok => Color32::GREEN,
            Status::Error => Color32::RED,
            Status::Muted => Color32::DARK_GRAY,
            Status::Neutral => Color32::GRAY,
        }
    }
}

/// Widget: a short colored status label (e.g. "Subscribed", "Error 5: ...").
pub struct StatusBadge {
    status: Status,
    text: RichText,
}

impl StatusBadge {
    /// Positive / healthy state (subscribed, encrypted, RPC success).
    pub fn ok(text: impl Into<RichText>) -> Self {
        Self::new(Status::Ok, text)
    }

    /// Failure / negative state (unsubscribed, unencrypted, errors).
    pub fn error(text: impl Into<RichText>) -> Self {
        Self::new(Status::Error, text)
    }

    /// A disabled/muted indicator.
    pub fn muted(text: impl Into<RichText>) -> Self {
        Self::new(Status::Muted, text)
    }

    /// Informational, no judgement (in-progress, "none yet").
    pub fn neutral(text: impl Into<RichText>) -> Self {
        Self::new(Status::Neutral, text)
    }

    fn new(status: Status, text: impl Into<RichText>) -> Self {
        Self {
            status,
            text: text.into(),
        }
    }
}

impl Widget for StatusBadge {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.label(self.text.color(self.status.color()))
    }
}
