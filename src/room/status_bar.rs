use crate::ui::status_badge::StatusBadge;
use livekit::prelude::*;

/// Fixed width for the action button so it doesn't resize as its label changes
/// between Disconnect / Reconnect / Connecting.
const BUTTON_WIDTH: f32 = 100.0;

/// Connect/disconnect intents raised by the status bar. The bar borrows the
/// window's state, so it records the intent here and the window applies it
/// (with `&mut self`) after the borrow ends.
#[derive(Default)]
pub struct StatusBarActions {
    pub reconnect: bool,
    pub disconnect: bool,
}

/// Bottom status bar: the connection status on the left, an optional detail
/// next to it, and a connect/disconnect button floated to the right. Shared by
/// the room and agent windows.
pub struct StatusBar<'a> {
    pub room: Option<&'a Room>,
    pub connecting: bool,
    pub connection_failure: Option<&'a str>,
    /// Shown after the connection status, e.g. the agent's state.
    pub detail: Option<StatusBadge>,
    /// Hover text for the Reconnect button.
    pub reconnect_hint: Option<&'a str>,
    pub actions: &'a mut StatusBarActions,
}

impl egui::Widget for StatusBar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let StatusBar {
            room,
            connecting,
            connection_failure,
            detail,
            reconnect_hint,
            actions,
        } = self;

        let state = room.map(Room::connection_state);

        ui.horizontal(|ui| {
            if let Some(err) = connection_failure {
                ui.add(StatusBadge::error(format!("Connection failed: {err}")));
            } else if connecting {
                ui.label("Connecting...");
            } else {
                match room.zip(state) {
                    Some((room, ConnectionState::Connected)) => {
                        ui.label(format!(
                            "Connected to '{}' as '{}'",
                            room.name(),
                            room.local_participant().identity()
                        ));
                    }
                    Some((_, ConnectionState::Reconnecting)) => {
                        ui.label("Reconnecting...");
                    }
                    _ => {
                        ui.label("Disconnected");
                    }
                }
            }
            if let Some(detail) = detail {
                ui.separator();
                ui.add(detail);
            }

            ui.add(ConnectionButton {
                connecting,
                connected: matches!(
                    state,
                    Some(ConnectionState::Connected | ConnectionState::Reconnecting)
                ),
                reconnect_hint,
                actions,
            });
        })
        .response
    }
}

/// The Connecting / Disconnect / Reconnect button, floated to the right of the
/// status bar.
struct ConnectionButton<'a> {
    connecting: bool,
    connected: bool,
    reconnect_hint: Option<&'a str>,
    actions: &'a mut StatusBarActions,
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
