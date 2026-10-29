use crate::{
    agent::service::{AgentCmd, AgentEvent, AgentService},
    agent::status_bar::AgentStatusBar,
    connect::AgentConnectSettings,
    media::VideoRenderer,
    room::participants::ParticipantsPanel,
    room::status_bar::StatusBarActions,
    service::PublishState,
    ui::{
        chat_message::ChatMessage,
        placeholder_tile::{PlaceholderTile, placeholder_texture},
        status_badge::StatusBadge,
        video_tile::VideoTile,
    },
};
use livekit_session::{AgentState, Message, MessageOrigin};
use std::collections::BTreeMap;

const AVATAR_HEIGHT: f32 = 200.0;
const SEND_BUTTON_WIDTH: f32 = 60.0;

/// State and UI of a single agent session window. The session starts
/// immediately on creation with the settings handed over by the connect screen.
pub struct AgentWindow {
    runtime: tokio::runtime::Handle,
    render_state: egui_wgpu::RenderState,
    service: AgentService,
    reuses_room: bool,
    /// The feed keyed by [`Message::seq`], which a growing transcript keeps.
    messages: BTreeMap<u64, Message>,
    draft: String,
    avatar: Option<VideoRenderer>,
    mic: PublishState,
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
            mic: PublishState::default(),
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
            AgentEvent::Messages(messages) => {
                self.messages = messages
                    .into_iter()
                    .map(|message| (message.seq, message))
                    .collect();
            }
            AgentEvent::SendFailed(err) => self.send_failure = Some(err),
            AgentEvent::Mic(state) => self.mic = state,
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

        let snapshot = self.service.snapshot();
        let room = self.service.room();
        let mut actions = StatusBarActions::default();
        let bar_frame =
            egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(10, 6));

        egui::Panel::top("top_panel")
            .frame(bar_frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let published = self.mic == PublishState::Published;
                    let enabled = room.is_some() && self.mic != PublishState::Pending;
                    let toggle = ui
                        .add_enabled(
                            enabled,
                            egui::Button::selectable(published, "🎤 Microphone"),
                        )
                        .on_hover_text("Publish or unpublish the microphone");
                    if toggle.clicked() {
                        self.service.send(AgentCmd::ToggleMic);
                    }
                });
            });

        egui::Panel::bottom("status_bar")
            .frame(bar_frame)
            .show(ui, |ui| {
                ui.add(AgentStatusBar {
                    snapshot,
                    room: room.as_deref(),
                    connecting: self.connecting,
                    connection_failure: self.connection_failure.as_deref(),
                    reuses_room: self.reuses_room,
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
            .show(ui, |ui| self.composer(ui, snapshot.is_connected()));

        egui::CentralPanel::default().show(ui, |ui| {
            let speaking = snapshot.agent_state == AgentState::Speaking;
            ui.allocate_ui(
                egui::vec2(ui.available_width(), AVATAR_HEIGHT),
                |ui| match &self.avatar {
                    Some(renderer) => {
                        ui.add(VideoTile::new(renderer.texture_id(), "Avatar", speaking))
                    }
                    None => {
                        let texture = placeholder_texture(ui.ctx());
                        ui.add(PlaceholderTile::new(texture.id(), "Agent", speaking))
                    }
                },
            );
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("agent_feed")
                .stick_to_bottom(true)
                .auto_shrink(false)
                .show(ui, |ui| {
                    if self.messages.is_empty() {
                        ui.weak("No messages yet");
                    }
                    for message in self.messages.values() {
                        ui.push_id((&message.id, message.participant_identity.as_str()), |ui| {
                            ui.add(
                                ChatMessage::new(speaker(message), &message.text)
                                    .local(is_local(message))
                                    .pending(!message.is_final),
                            );
                        });
                        ui.add_space(4.0);
                    }
                });
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
        ui.horizontal(|ui| {
            let width = ui.available_width() - SEND_BUTTON_WIDTH - ui.spacing().item_spacing.x;
            let edit = ui.add_enabled(
                connected,
                egui::TextEdit::singleline(&mut self.draft)
                    .hint_text("Message the agent")
                    .desired_width(width),
            );
            let submitted = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let has_text = !self.draft.trim().is_empty();
            let send = egui::Button::new("Send").min_size(egui::vec2(SEND_BUTTON_WIDTH, 0.0));
            let clicked = ui.add_enabled(connected && has_text, send).clicked();
            if (submitted || clicked) && has_text {
                let text = self.draft.trim().to_owned();
                self.draft.clear();
                self.send_failure = None;
                self.service.send(AgentCmd::SendText(text));
                edit.request_focus();
            }
        });
        if let Some(err) = &self.send_failure {
            ui.add(StatusBadge::error(format!("Failed to send: {err}")));
        }
    }
}

fn speaker(message: &Message) -> &str {
    match message.origin {
        MessageOrigin::AgentTranscript => "Agent",
        MessageOrigin::UserTranscript => "You (transcribed)",
        MessageOrigin::UserInput => "You",
        _ => message.participant_identity.as_str(),
    }
}

fn is_local(message: &Message) -> bool {
    matches!(
        message.origin,
        MessageOrigin::UserTranscript | MessageOrigin::UserInput
    )
}
