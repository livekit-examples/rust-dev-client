use crate::connect::AgentConnectSettings;
use crate::room::participants::{TrackCmd, TrackCommands};
use livekit::prelude::*;
use livekit_session::{Message, Session, SessionEvent, SessionState};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

#[derive(Debug)]
pub enum AgentCmd {
    Start,
    End,
    SendText(String),
    Track(TrackCmd),
}

#[derive(Debug)]
pub enum AgentEvent {
    /// `Err` is a human-readable message.
    StartResult(Result<(), String>),
    /// A message was added, or a transcript grew.
    Message(Message),
    SendFailed(String),
    /// The agent's avatar video, or `None` once it is gone.
    Avatar(Option<RemoteVideoTrack>),
}

/// The UI-side handle of an agent session: commands go to [`AgentActor`],
/// events come back. State that is cheap to read synchronously (the session
/// state and the room) is read from the session directly.
pub struct AgentService {
    session: Session,
    state: watch::Receiver<SessionState>,
    cmd_tx: mpsc::UnboundedSender<AgentCmd>,
    ui_rx: mpsc::UnboundedReceiver<AgentEvent>,
}

impl AgentService {
    pub fn new(runtime: &tokio::runtime::Handle, settings: &AgentConnectSettings) -> Self {
        let session = Session::from(settings);

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (ui_tx, ui_rx) = mpsc::unbounded_channel();
        let actor = AgentActor {
            session: session.clone(),
            ui_tx,
            avatar_sid: None,
        };
        runtime.spawn(actor.run(cmd_rx));

        Self {
            state: session.state(),
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

    pub fn state(&self) -> SessionState {
        *self.state.borrow()
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

enum Input {
    Cmd(Option<AgentCmd>),
    Feed(SessionEvent),
    StateChanged,
}

/// Owns the session's async side: starting and ending it, and forwarding the
/// message feed and avatar changes to the UI. The microphone and the agent's
/// audio are the session's own (`SessionOptions::platform_audio`).
struct AgentActor {
    session: Session,
    ui_tx: mpsc::UnboundedSender<AgentEvent>,
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
                Ok(()) = state.changed() => Input::StateChanged,
            };
            match input {
                Input::Cmd(None) => break,
                Input::Cmd(Some(cmd)) => self.handle(cmd).await,
                Input::Feed(SessionEvent::MessageUpserted(message)) => {
                    self.emit(AgentEvent::Message(message));
                }
                // Only `MessagesRestored`, which follows `Session::restore_messages`;
                // this client never restores a feed.
                Input::Feed(_) => {}
                Input::StateChanged => {
                    let has_avatar = state.borrow_and_update().agent_tracks.avatar_video;
                    self.refresh_avatar(has_avatar);
                }
            }
        }

        self.session.end().await;
    }

    async fn handle(&mut self, cmd: AgentCmd) {
        match cmd {
            AgentCmd::Start => self.start().await,
            AgentCmd::End => self.session.end().await,
            AgentCmd::SendText(text) => {
                if let Err(err) = self.session.send_text(&text).await {
                    self.emit(AgentEvent::SendFailed(err.to_string()));
                }
            }
            AgentCmd::Track(cmd) => cmd.apply(),
        }
    }

    async fn start(&mut self) {
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
        self.emit(AgentEvent::StartResult(Ok(())));
    }

    /// Reports the avatar's video track when it changes. Compared by sid, as a
    /// replaced track can leave the flag set.
    fn refresh_avatar(&mut self, has_avatar: bool) {
        let track = has_avatar
            .then(|| self.session.agent_avatar_video_track())
            .flatten();
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
