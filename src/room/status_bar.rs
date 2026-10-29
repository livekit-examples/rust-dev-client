use crate::room::RoomContext;
use crate::ui::status_badge::StatusBadge;
use livekit::prelude::*;

/// Fixed width for the action button so it doesn't resize as its label changes
/// between Disconnect / Reconnect / Connecting.
const BUTTON_WIDTH: f32 = 100.0;

/// Connect/disconnect intents raised by the status bar. It only holds
/// `&RoomContext`, so it records the intent here and the window applies it
/// (with `&mut self`) after the borrow ends.
#[derive(Default)]
pub struct StatusBarActions {
    pub reconnect: bool,
    pub disconnect: bool,
}

/// Bottom status bar: the current connection status on the left and a
/// connect/disconnect button floated to the right.
pub struct StatusBar<'a> {
    pub ctx: &'a RoomContext<'a>,
    pub connecting: bool,
    pub connection_failure: Option<&'a str>,
    pub actions: &'a mut StatusBarActions,
}

impl egui::Widget for StatusBar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let StatusBar {
            ctx,
            connecting,
            connection_failure,
            actions,
        } = self;

        let room = ctx
            .room
            .filter(|r| r.connection_state() == ConnectionState::Connected);

        ui.horizontal(|ui| {
            if let Some(err) = connection_failure {
                ui.add(StatusBadge::error(format!("Connection failed: {err}")));
            } else if connecting {
                ui.label("Connecting...");
            } else if let Some(room) = room {
                ui.label(format!(
                    "Connected to '{}' as '{}'",
                    room.name(),
                    room.local_participant().identity()
                ));
            } else {
                ui.label("Disconnected");
            }

            ui.add(ConnectionButton {
                connecting,
                connected: room.is_some(),
                reconnect_hint: None,
                actions,
            });
        })
        .response
    }
}

/// The Connecting / Disconnect / Reconnect button, floated to the right of a
/// status bar.
pub struct ConnectionButton<'a> {
    pub connecting: bool,
    pub connected: bool,
    /// Hover text for the Reconnect button.
    pub reconnect_hint: Option<&'a str>,
    pub actions: &'a mut StatusBarActions,
}

impl egui::Widget for ConnectionButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let ConnectionButton {
            connecting,
            connected,
            reconnect_hint,
            actions,
        } = self;
        egui::Aligned::new(egui::Align2::RIGHT_CENTER)
            .id_salt("status_actions")
            .show(ui, |ui| {
                let size = egui::vec2(BUTTON_WIDTH, 0.0);
                if connecting {
                    ui.add_enabled(false, egui::Button::new("Connecting...").min_size(size));
                } else if connected {
                    if ui
                        .add(egui::Button::new("Disconnect").min_size(size))
                        .clicked()
                    {
                        actions.disconnect = true;
                    }
                } else {
                    let mut reconnect = ui.add(egui::Button::new("Reconnect").min_size(size));
                    if let Some(hint) = reconnect_hint {
                        reconnect = reconnect.on_hover_text(hint);
                    }
                    if reconnect.clicked() {
                        actions.reconnect = true;
                    }
                }
            })
            .response
    }
}
