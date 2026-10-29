use crate::room::status_bar::{ConnectionButton, StatusBarActions};
use crate::ui::status_badge::StatusBadge;
use livekit::prelude::*;
use livekit_session::{AgentFailure, AgentState, SessionConnectionState, SessionSnapshot};

impl From<AgentState> for StatusBadge {
    fn from(state: AgentState) -> Self {
        match state {
            AgentState::Disconnected => StatusBadge::muted("Agent disconnected"),
            AgentState::Connecting => StatusBadge::neutral("Waiting for agent..."),
            AgentState::Initializing => StatusBadge::neutral("Agent initializing"),
            AgentState::Idle => StatusBadge::neutral("Agent idle"),
            AgentState::Listening => StatusBadge::ok("Agent listening"),
            AgentState::Thinking => StatusBadge::ok("Agent thinking"),
            AgentState::Speaking => StatusBadge::ok("Agent speaking"),
            AgentState::Failed(AgentFailure::Timeout) => {
                StatusBadge::error("Agent did not connect in time")
            }
            AgentState::Failed(AgentFailure::Left) => StatusBadge::error("Agent left"),
            state => StatusBadge::neutral(format!("Agent {state:?}")),
        }
    }
}

/// Bottom status bar of an agent window: the connection and agent state on the
/// left and a connect/disconnect button floated to the right.
pub struct AgentStatusBar<'a> {
    pub snapshot: SessionSnapshot,
    pub room: Option<&'a Room>,
    pub connecting: bool,
    pub connection_failure: Option<&'a str>,
    /// Reconnecting rejoins the same room, where the agent may not be
    /// dispatched again.
    pub reuses_room: bool,
    pub actions: &'a mut StatusBarActions,
}

impl egui::Widget for AgentStatusBar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let AgentStatusBar {
            snapshot,
            room,
            connecting,
            connection_failure,
            reuses_room,
            actions,
        } = self;

        ui.horizontal(|ui| {
            if let Some(err) = connection_failure {
                ui.add(StatusBadge::error(format!("Connection failed: {err}")));
            } else if connecting {
                ui.label("Connecting...");
            } else {
                match (snapshot.connection_state, room) {
                    (SessionConnectionState::Connected, Some(room)) => {
                        ui.label(format!(
                            "Connected to '{}' as '{}'",
                            room.name(),
                            room.local_participant().identity()
                        ));
                    }
                    (SessionConnectionState::Reconnecting, _) => {
                        ui.label("Reconnecting...");
                    }
                    _ => {
                        ui.label("Disconnected");
                    }
                }
            }
            ui.separator();
            ui.add(StatusBadge::from(snapshot.agent_state));

            ui.add(ConnectionButton {
                connecting,
                connected: snapshot.is_connected(),
                reconnect_hint: reuses_room.then_some(
                    "A pre-generated token rejoins the same room, where the agent may not be \
                     dispatched again",
                ),
                actions,
            });
        })
        .response
    }
}
