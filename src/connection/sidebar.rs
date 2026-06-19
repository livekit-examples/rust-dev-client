use crate::connection::ConnCtx;
use crate::service::AsyncCmd;
use crate::ui::status_badge::StatusBadge;
use livekit::prelude::*;

/// Connect/disconnect intents raised by [`ConnectionControls`]. The widget only
/// borrows `&ConnCtx`, so it can't drive the window's own connect/disconnect
/// (which need `&mut ConnectionWindow`); it records the intent here and the
/// window applies it after the borrow ends.
#[derive(Default)]
pub struct SidebarActions {
    pub reconnect: bool,
    pub disconnect: bool,
}

/// Connection controls: connecting spinner / Disconnect / Reconnect, the E2EE
/// key-ratchet button, and the last connection failure (if any).
pub struct ConnectionControls<'a> {
    pub ctx: &'a ConnCtx<'a>,
    pub connecting: bool,
    pub connection_failure: Option<&'a str>,
    pub actions: &'a mut SidebarActions,
}

impl egui::Widget for ConnectionControls<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let ConnectionControls {
            ctx,
            connecting,
            connection_failure,
            actions,
        } = self;

        let connected = ctx
            .room
            .is_some_and(|r| r.connection_state() == ConnectionState::Connected);

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if connecting {
                    ui.spinner();
                    ui.label("Connecting...");
                } else if connected {
                    if ui.button("Disconnect").clicked() {
                        actions.disconnect = true;
                    }
                } else if ui.button("Reconnect").clicked() {
                    actions.reconnect = true;
                }
            });

            if ui.button("E2eeKeyRatchet").clicked() {
                let _ = ctx.service.send(AsyncCmd::E2eeKeyRatchet);
            }

            if let Some(err) = connection_failure {
                ui.add(StatusBadge::error(err));
            }
        })
        .response
    }
}

/// Read-only room facts: name, connection state, participant count.
pub struct RoomInfo<'a> {
    pub room: &'a Room,
}

impl egui::Widget for RoomInfo<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let room = self.room;
        ui.vertical(|ui| {
            ui.label(format!("Name: {}", room.name()));
            ui.label(format!("ConnectionState: {:?}", room.connection_state()));
            ui.label(format!(
                "ParticipantCount: {:?}",
                room.remote_participants().len() + 1
            ));
        })
        .response
    }
}
