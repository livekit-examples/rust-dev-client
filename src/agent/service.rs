use crate::connect::AgentConnectSettings;
use crate::media::MicTrack;
use crate::room::participants::{TrackCmd, TrackCommands};
use crate::service::PublishState;
use livekit::{
    e2ee::{E2eeOptions, EncryptionType, key_provider::*},
    prelude::*,
};
use livekit_session::{
    Message, Session, SessionConnectionState, SessionEvent, SessionOptions, SessionSnapshot,
};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AgentCmd {
    Start,
    End,
    SendText(String),
    ToggleMic,
    Track(TrackCmd),
}

#[derive(Debug)]
pub enum AgentEvent {
    /// `Err` is a human-readable message.
    StartResult(Result<(), String>),
    /// A message was added, or a transcript grew.
    Message(Message),
    /// The whole feed, after it was replaced.
    Messages(Vec<Message>),
    SendFailed(String),
    Mic(PublishState),
    /// The agent's avatar video, or `None` once it is gone.
    Avatar(Option<RemoteVideoTrack>),
}

/// The UI-side handle of an agent session: commands go to [`AgentActor`],
/// events come back. State that is cheap to read synchronously (the snapshot
/// and the room) is read from the session directly.
pub struct AgentService {
    session: Session,
    cmd_tx: mpsc::UnboundedSender<AgentCmd>,
    ui_rx: mpsc::UnboundedReceiver<AgentEvent>,
}

impl AgentService {
    pub fn new(runtime: &tokio::runtime::Handle, settings: &AgentConnectSettings) -> Self {
        let mut room_options = RoomOptions::default();
        room_options.encryption = settings.enable_e2ee.then(|| E2eeOptions {
            encryption_type: EncryptionType::Gcm,
            key_provider: KeyProvider::with_shared_key(
                KeyProviderOptions::default(),
                settings.key.clone().into_bytes(),
            ),
        });
        let options = SessionOptions {
            agent_connect_timeout: settings.agent_connect_timeout,
            room_options,
        };
        let session = Session::new(settings.auth.clone(), options);

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (ui_tx, ui_rx) = mpsc::unbounded_channel();
        let actor = AgentActor {
            session: session.clone(),
            voice: settings.voice,
            ui_tx,
            connection: None,
            avatar_sid: None,
        };
        runtime.spawn(actor.run(cmd_rx));

        Self {
            session,
            cmd_tx,
            ui_rx,
        }
    }

    pub fn send(&self, cmd: AgentCmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn try_recv(&mut self) -> Option<AgentEvent> {
        self.ui_rx.try_recv().ok()
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        self.session.snapshot()
    }

    pub fn room(&self) -> Option<Arc<Room>> {
        self.session.room()
    }
}

impl TrackCommands for AgentService {
    fn track_cmd(&self, cmd: TrackCmd) {
        self.send(AgentCmd::Track(cmd));
    }
}

/// What the actor holds while the session's room is connected.
struct Connection {
    room_events: mpsc::UnboundedReceiver<RoomEvent>,
    // Drives playout of the agent's audio and is shared with the mic for
    // capture. It must exist before the room connects.
    platform_audio: Option<PlatformAudio>,
    mic: MicTrack,
}

enum Input {
    Cmd(Option<AgentCmd>),
    Feed(SessionEvent),
    Room(RoomEvent),
    StateChanged,
}

/// Owns the session's async side: starting and ending it, the microphone,
/// and forwarding the message feed and avatar changes to the UI.
struct AgentActor {
    session: Session,
    voice: bool,
    ui_tx: mpsc::UnboundedSender<AgentEvent>,
    connection: Option<Connection>,
    avatar_sid: Option<TrackSid>,
}

impl AgentActor {
    async fn run(mut self, mut cmd_rx: mpsc::UnboundedReceiver<AgentCmd>) {
        let mut feed = self.session.subscribe();
        let mut state = self.session.state();

        loop {
            let input = tokio::select! {
                cmd = cmd_rx.recv() => Input::Cmd(cmd),
                Some(event) = feed.recv() => Input::Feed(event),
                Some(event) = recv_room_event(&mut self.connection) => Input::Room(event),
                Ok(()) = state.changed() => Input::StateChanged,
            };
            match input {
                Input::Cmd(None) => break,
                Input::Cmd(Some(cmd)) => self.handle(cmd).await,
                Input::Feed(SessionEvent::MessageUpserted(message)) => {
                    self.emit(AgentEvent::Message(message));
                }
                Input::Feed(_) => self.emit(AgentEvent::Messages(self.session.messages())),
                Input::Room(event) => self.on_room_event(event),
                Input::StateChanged => {
                    let connection_state = state.borrow_and_update().connection_state;
                    if connection_state == SessionConnectionState::Disconnected
                        && self.connection.is_some()
                    {
                        self.release();
                    }
                }
            }
        }

        // Fails only when the session already ended.
        let _ = self.session.end().await;
    }

    async fn handle(&mut self, cmd: AgentCmd) {
        match cmd {
            AgentCmd::Start => self.start().await,
            AgentCmd::End => self.end().await,
            AgentCmd::SendText(text) => {
                if let Err(err) = self.session.send_text(&text).await {
                    self.emit(AgentEvent::SendFailed(err.to_string()));
                }
            }
            AgentCmd::ToggleMic => self.toggle_mic().await,
            AgentCmd::Track(cmd) => cmd.apply(),
        }
    }

    async fn start(&mut self) {
        let platform_audio = match PlatformAudio::new() {
            Ok(audio) => Some(audio),
            Err(err) => {
                log::error!("failed to init platform audio (agent audio + mic disabled): {err}");
                None
            }
        };

        if let Err(err) = self.session.start().await {
            log::error!("failed to start agent session: {err}");
            self.emit(AgentEvent::StartResult(Err(err.to_string())));
            return;
        }
        let Some(room) = self.session.room() else {
            self.emit(AgentEvent::StartResult(Err(
                "session disconnected while starting".to_string(),
            )));
            return;
        };
        log::info!("agent session connected to room: {}", room.name());

        self.connection = Some(Connection {
            room_events: room.subscribe(),
            platform_audio,
            mic: MicTrack::new(room),
        });
        self.emit(AgentEvent::StartResult(Ok(())));
        self.refresh_avatar();
        if self.voice {
            self.toggle_mic().await;
        }
    }

    async fn end(&mut self) {
        if let Some(Connection {
            platform_audio: Some(platform_audio),
            mut mic,
            ..
        }) = self.connection.take()
            && mic.is_published()
            && let Err(err) = mic.unpublish(&platform_audio).await
        {
            log::warn!("failed to unpublish microphone: {err}");
        }
        if let Err(err) = self.session.end().await {
            log::warn!("failed to end agent session: {err}");
        }
        self.release();
    }

    /// Drops what belonged to the connection once the room is gone.
    fn release(&mut self) {
        self.connection = None;
        self.emit(AgentEvent::Mic(PublishState::Unpublished));
        self.refresh_avatar();
    }

    async fn toggle_mic(&mut self) {
        let Some(connection) = self.connection.as_mut() else {
            return;
        };
        let Some(platform_audio) = connection.platform_audio.as_ref() else {
            log::error!("cannot toggle microphone: platform audio unavailable");
            return;
        };
        let _ = self.ui_tx.send(AgentEvent::Mic(PublishState::Pending));
        let mic = &mut connection.mic;
        let result = if mic.is_published() {
            mic.unpublish(platform_audio).await
        } else {
            mic.publish(platform_audio).await
        };
        if let Err(err) = result {
            log::error!("failed to toggle microphone: {err}");
        }
        let state = if mic.is_published() {
            PublishState::Published
        } else {
            PublishState::Unpublished
        };
        self.emit(AgentEvent::Mic(state));
    }

    fn on_room_event(&mut self, event: RoomEvent) {
        if matches!(
            event,
            RoomEvent::TrackSubscribed { .. }
                | RoomEvent::TrackUnsubscribed { .. }
                | RoomEvent::ParticipantDisconnected(_)
        ) {
            self.refresh_avatar();
        }
    }

    /// Reports the avatar's video track when it changes.
    fn refresh_avatar(&mut self) {
        let track = self
            .connection
            .as_ref()
            .and_then(|_| self.session.agent_avatar_video_track());
        let sid = track.as_ref().map(|track| track.sid());
        if sid != self.avatar_sid {
            self.avatar_sid = sid;
            self.emit(AgentEvent::Avatar(track));
        }
    }

    fn emit(&self, event: AgentEvent) {
        let _ = self.ui_tx.send(event);
    }
}

/// The next event of the connected room; pending while there is none.
async fn recv_room_event(connection: &mut Option<Connection>) -> Option<RoomEvent> {
    match connection {
        Some(connection) => connection.room_events.recv().await,
        None => std::future::pending().await,
    }
}
