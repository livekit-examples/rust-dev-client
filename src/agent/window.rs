use crate::{
    agent::avatar_panel::AvatarPanel,
    agent::feed::MessageFeed,
    agent::service::{AgentCmd, AgentEvent, AgentService},
    connect::AgentConnectSettings,
    media::VideoRenderer,
    room::participants::ParticipantsPanel,
    room::status_bar::{StatusBar, StatusBarActions},
    ui::{chat_composer::ChatComposer, status_badge::StatusBadge},
};
use livekit_session::{AgentActivity, AgentFailure, AgentState, Message};
use std::collections::BTreeMap;

/// State and UI of a single agent session window. The session starts
/// immediately on creation with the settings handed over by the connect screen.
pub struct AgentWindow {
    runtime: tokio::runtime::Handle,
    render_state: egui_wgpu::RenderState,
    service: AgentService,
    /// Reconnecting rejoins the same room, where the agent may not be
    /// dispatched again.
    reuses_room: bool,
    /// The feed keyed by [`Message::seq`], which a growing transcript keeps.
    messages: BTreeMap<u64, Message>,
    draft: String,
    avatar: Option<VideoRenderer>,
    connecting: bool,
    connection_failure: Option<String>,
    send_failure: Option<String>,
}

impl AgentWindow {
    pub fn new(
        runtime: tokio::runtime::Handle,
        render_state: egui_wgpu::RenderState,
        settings: &AgentConnectSettings,
    ) -> Self {
        let mut window = Self {
            service: AgentService::new(&runtime, settings),
            runtime,
            render_state,
            reuses_room: settings.auth.reuses_room(),
            messages: BTreeMap::new(),
            draft: String::new(),
            avatar: None,
            connecting: false,
            connection_failure: None,
            send_failure: None,
        };
        window.connect();
        window
    }

    fn connect(&mut self) {
        self.connecting = true;
        self.connection_failure = None;
        self.service.send(AgentCmd::Start);
    }

    pub fn disconnect(&self) {
        self.service.send(AgentCmd::End);
    }

    fn event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::StartResult(result) => {
                self.connecting = false;
                self.connection_failure = result.err();
            }
            AgentEvent::Message(message) => {
                self.messages.insert(message.seq, message);
            }
            AgentEvent::SendFailed(err) => self.send_failure = Some(err),
            AgentEvent::Avatar(track) => {
                self.avatar = track.map(|track| {
                    VideoRenderer::new(&self.runtime, self.render_state.clone(), track.rtc_track())
                });
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        while let Some(event) = self.service.try_recv() {
            self.event(event);
        }

        let state = self.service.state();
        let room = self.service.room();
        let mut actions = StatusBarActions::default();
        let bar_frame =
            egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(10, 6));

        egui::Panel::bottom("status_bar")
            .frame(bar_frame)
            .show(ui, |ui| {
                ui.add(StatusBar {
                    room: room.as_deref(),
                    connecting: self.connecting,
                    connection_failure: self.connection_failure.as_deref(),
                    detail: Some(StatusBadge::from(state.agent)),
                    reconnect_hint: self.reuses_room.then_some(
                        "A pre-generated token rejoins the same room, where the agent may not \
                         be dispatched again",
                    ),
                    actions: &mut actions,
                });
            });

        egui::Panel::right("right_panel")
            .frame(egui::Frame::central_panel(ui.style()).outer_margin(0.))
            .resizable(true)
            .size_range(20.0..=360.0)
            .show(ui, |ui| {
                ui.heading("Participants");
                ui.separator();
                ui.add(ParticipantsPanel {
                    room: room.as_deref(),
                    commands: &self.service,
                });
            });

        egui::Panel::bottom("composer")
            .frame(bar_frame)
            .show(ui, |ui| self.composer(ui, state.connection.is_connected()));

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add(AvatarPanel {
                state: state.agent,
                avatar: self.avatar.as_ref(),
            });
            ui.separator();
            ui.add(MessageFeed::new(&self.messages, "agent"));
        });

        if actions.disconnect {
            self.disconnect();
        } else if actions.reconnect {
            self.connect();
        }

        ui.ctx().request_repaint();
    }

    /// The text input with Enter-to-send, and the last send failure.
    fn composer(&mut self, ui: &mut egui::Ui, connected: bool) {
        let mut submitted = None;
        ui.add(
            ChatComposer::new(&mut self.draft, &mut submitted)
                .enabled(connected)
                .hint_text("Message the agent"),
        );
        if let Some(text) = submitted {
            self.send_failure = None;
            self.service.send(AgentCmd::SendText(text));
        }
        if let Some(err) = &self.send_failure {
            ui.add(StatusBadge::error(format!("Failed to send: {err}")));
        }
    }
}

impl From<AgentState> for StatusBadge {
    fn from(state: AgentState) -> Self {
        match state {
            AgentState::Disconnected => StatusBadge::muted("Agent disconnected"),
            AgentState::Connecting => StatusBadge::neutral("Waiting for agent..."),
            AgentState::Connected(activity) => StatusBadge::from(activity),
            AgentState::Failed(AgentFailure::Timeout) => {
                StatusBadge::error("Agent did not connect in time")
            }
            AgentState::Failed(AgentFailure::Left) => StatusBadge::error("Agent left"),
            state => StatusBadge::neutral(format!("Agent {state:?}")),
        }
    }
}

impl From<AgentActivity> for StatusBadge {
    fn from(activity: AgentActivity) -> Self {
        match activity {
            AgentActivity::Initializing => StatusBadge::neutral("Agent initializing"),
            AgentActivity::Idle => StatusBadge::neutral("Agent idle"),
            AgentActivity::Listening => StatusBadge::ok("Agent listening"),
            AgentActivity::Thinking => StatusBadge::ok("Agent thinking"),
            AgentActivity::Speaking => StatusBadge::ok("Agent speaking"),
            activity => StatusBadge::neutral(format!("Agent {activity:?}")),
        }
    }
}
