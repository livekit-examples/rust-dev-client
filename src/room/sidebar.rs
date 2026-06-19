use crate::room::RoomContext;
use crate::ui::status_badge::StatusBadge;
use livekit::prelude::*;

/// Connect/disconnect intents raised by the sidebar. Its widgets only borrow
/// `&RoomCtx`, so they can't drive the window's own connect/disconnect (which
/// need `&mut RoomWindow`); they record the intent here and the window
/// applies it after the borrow ends.
#[derive(Default)]
pub struct SidebarActions {
    pub reconnect: bool,
    pub disconnect: bool,
}

/// The left panel: connection URL, connect/disconnect controls, and room info.
pub struct Sidebar<'a> {
    pub ctx: &'a RoomContext<'a>,
    pub url: &'a str,
    pub connecting: bool,
    pub connection_failure: Option<&'a str>,
    pub actions: &'a mut SidebarActions,
}

impl egui::Widget for Sidebar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let Sidebar {
            ctx,
            url,
            connecting,
            connection_failure,
            actions,
        } = self;

        ui.scope(|ui| {
            ui.add_space(8.0);
            ui.monospace(url);
            ui.add_space(8.0);
            ui.add(RoomControls {
                ctx,
                connecting,
                connection_failure,
                actions,
            });
            if let Some(room) = ctx.room {
                ui.add(RoomInfo { room });
            }
            ui.separator();
        })
        .response
    }
}

/// Room connection controls: connecting spinner / Disconnect / Reconnect, the
/// E2EE key-ratchet button, and the last connection failure (if any).
struct RoomControls<'a> {
    ctx: &'a RoomContext<'a>,
    connecting: bool,
    connection_failure: Option<&'a str>,
    actions: &'a mut SidebarActions,
}

impl egui::Widget for RoomControls<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let RoomControls {
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

            if let Some(err) = connection_failure {
                ui.add(StatusBadge::error(err));
            }
        })
        .response
    }
}

/// Read-only room facts: name, connection state, participant count.
struct RoomInfo<'a> {
    room: &'a Room,
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
