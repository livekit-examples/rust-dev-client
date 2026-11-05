use crate::connect::ThreadConnectSettings;
use livekit::prelude::*;
use livekit_thread::{
    DirectoryError, Message, PageToken, Thread, ThreadDirectory, ThreadEvent, ThreadInfo,
    ThreadState, TurnOutcome,
};
use tokio::sync::{mpsc, watch};

pub enum ThreadsCmd {
    /// Lists the first page of threads again.
    Refresh,
    /// Lists the page after the last one listed.
    LoadMore,
    /// Ends the open thread and opens a new one.
    Create,
    /// Ends the open thread and opens the one with this id.
    Open(String),
    SendText(String),
    StartCall,
    EndCall,
    /// Ends the open thread and its call.
    Close,
}

/// `Err` is always a human-readable message.
pub enum ThreadsEvent {
    /// A page of the scope's threads, newest first.
    Listed(Result<ListedPage, String>),
    /// The current record of a thread a turn just ended in, which makes it
    /// the newest.
    Updated(ThreadInfo),
    Opened(Result<OpenedThread, String>),
    /// A message of the open thread was added, or grew.
    Message(Message),
    SendFailed(String),
    TurnFailed(String),
    /// The name of the call's room, once it connected.
    CallStarted(Result<String, String>),
    /// The agent's avatar video, or `None` once it is gone.
    Avatar(Option<RemoteVideoTrack>),
}

pub struct ListedPage {
    pub threads: Vec<ThreadInfo>,
    /// Whether the page continues the list, or starts it over.
    pub continues: bool,
    /// Whether [`ThreadsCmd::LoadMore`] fetches another page after it.
    pub has_more: bool,
}

pub struct OpenedThread {
    pub info: ThreadInfo,
    /// The feed so far; [`ThreadsEvent::Message`] follows with its changes.
    pub messages: Vec<Message>,
    /// The turns and the call.
    pub state: watch::Receiver<ThreadState>,
}

/// The UI-side handle of a thread directory: commands go to [`ThreadsActor`],
/// events come back.
pub struct ThreadsService {
    cmd_tx: mpsc::UnboundedSender<ThreadsCmd>,
    ui_rx: mpsc::UnboundedReceiver<ThreadsEvent>,
}

impl ThreadsService {
    pub fn new(runtime: &tokio::runtime::Handle, settings: ThreadConnectSettings) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (ui_tx, ui_rx) = mpsc::unbounded_channel();
        runtime.spawn(async move {
            let actor = ThreadsActor {
                directory: ThreadDirectory::from(&settings),
                ui_tx,
                open: None,
                avatar_sid: None,
                next_page: None,
            };
            actor.run(cmd_rx).await;
        });
        Self { cmd_tx, ui_rx }
    }

    pub fn send(&self, cmd: ThreadsCmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn try_recv(&mut self) -> Option<ThreadsEvent> {
        self.ui_rx.try_recv().ok()
    }
}

enum Input {
    Cmd(Option<ThreadsCmd>),
    Thread(ThreadEvent),
    StateChanged,
    Room(RoomEvent),
}

/// The open thread with what the actor receives from it.
struct Open {
    thread: Thread,
    events: mpsc::UnboundedReceiver<ThreadEvent>,
    state: watch::Receiver<ThreadState>,
    /// The call's room events, `None` while the call has no room.
    room_events: Option<mpsc::UnboundedReceiver<RoomEvent>>,
}

impl Open {
    /// The next input of the thread or its call's room.
    async fn next(&mut self) -> Input {
        tokio::select! {
            Some(event) = self.events.recv() => Input::Thread(event),
            Ok(()) = self.state.changed() => Input::StateChanged,
            Some(event) = recv(&mut self.room_events) => Input::Room(event),
            else => std::future::pending().await,
        }
    }
}

/// Owns the directory and the open thread: lists, opens and creates threads,
/// and forwards the open thread's feed and avatar to the UI. One thread is open
/// at a time; opening another ends it and its call.
struct ThreadsActor {
    directory: ThreadDirectory,
    ui_tx: mpsc::UnboundedSender<ThreadsEvent>,
    open: Option<Open>,
    avatar_sid: Option<TrackSid>,
    /// Where the next page of the list starts, `None` once it is complete.
    next_page: Option<PageToken>,
}

impl ThreadsActor {
    async fn run(mut self, mut cmd_rx: mpsc::UnboundedReceiver<ThreadsCmd>) {
        self.list(None).await;
        loop {
            let input = tokio::select! {
                cmd = cmd_rx.recv() => Input::Cmd(cmd),
                input = next(&mut self.open) => input,
            };
            match input {
                Input::Cmd(None) => break,
                Input::Cmd(Some(cmd)) => self.handle(cmd).await,
                Input::Thread(event) => self.on_thread_event(event).await,
                Input::StateChanged => self.sync_room(),
                Input::Room(event) => self.on_room_event(event),
            }
        }
        self.close().await;
    }

    async fn handle(&mut self, cmd: ThreadsCmd) {
        match cmd {
            ThreadsCmd::Refresh => self.list(None).await,
            ThreadsCmd::LoadMore => {
                if let Some(after) = self.next_page.clone() {
                    self.list(Some(after)).await;
                }
            }
            ThreadsCmd::Create => {
                self.close().await;
                let created = self.directory.create().await;
                self.adopt(created);
                self.list(None).await;
            }
            ThreadsCmd::Open(id) => {
                self.close().await;
                let opened = self.directory.open(&id).await;
                self.adopt(opened);
            }
            ThreadsCmd::SendText(text) => {
                let Some(open) = &self.open else {
                    return;
                };
                if let Err(err) = open.thread.send_text(&text).await {
                    self.emit(ThreadsEvent::SendFailed(err.to_string()));
                }
            }
            ThreadsCmd::StartCall => {
                let started = self.start_call().await;
                self.emit(ThreadsEvent::CallStarted(started));
                self.sync_room();
            }
            ThreadsCmd::EndCall => {
                if let Some(open) = &self.open {
                    open.thread.call().end().await;
                }
                self.sync_room();
            }
            ThreadsCmd::Close => self.close().await,
        }
    }

    /// Lists the first page, or the one after `after`, and remembers where
    /// the page after that starts.
    async fn list(&mut self, after: Option<PageToken>) {
        let listed = match self.directory.list(after.as_ref()).await {
            Ok(page) => {
                self.next_page = page.next;
                Ok(ListedPage {
                    threads: page.threads,
                    continues: after.is_some(),
                    has_more: self.next_page.is_some(),
                })
            }
            Err(err) => Err(err.to_string()),
        };
        self.emit(ThreadsEvent::Listed(listed));
    }

    /// Reports the open thread's current record, e.g. after a turn retitled it.
    async fn refresh_open(&self) {
        let Some(open) = &self.open else {
            return;
        };
        match self.directory.get(open.thread.id()).await {
            Ok(info) => self.emit(ThreadsEvent::Updated(info)),
            Err(err) => log::warn!("failed to refresh thread {}: {err}", open.thread.id()),
        }
    }

    /// Makes `thread` the open thread.
    fn adopt(&mut self, thread: Result<Thread, DirectoryError>) {
        let thread = match thread {
            Ok(thread) => thread,
            Err(err) => {
                log::error!("failed to open thread: {err}");
                self.emit(ThreadsEvent::Opened(Err(err.to_string())));
                return;
            }
        };
        // Taken together, so that no update is lost or delivered twice.
        let (messages, events) = thread.subscribe_with_messages();
        let opened = OpenedThread {
            info: thread.info().clone(),
            messages,
            state: thread.state(),
        };
        self.open = Some(Open {
            events,
            state: thread.state(),
            thread,
            room_events: None,
        });
        self.emit(ThreadsEvent::Opened(Ok(opened)));
    }

    /// Ends the open thread, which also ends its call.
    async fn close(&mut self) {
        let Some(open) = self.open.take() else {
            return;
        };
        open.thread.end().await;
        self.refresh_avatar();
    }

    /// Starts the open thread's call and returns its room's name.
    async fn start_call(&self) -> Result<String, String> {
        let open = self.open.as_ref().ok_or("no thread is open")?;
        let call = open.thread.call();
        call.start().await.map_err(|err| {
            log::error!("failed to start call: {err}");
            err.to_string()
        })?;
        let room = call.room().ok_or("call disconnected while starting")?;
        log::info!("thread call connected to room: {}", room.name());
        Ok(room.name())
    }

    async fn on_thread_event(&mut self, event: ThreadEvent) {
        match event {
            ThreadEvent::MessageUpserted(message) => self.emit(ThreadsEvent::Message(message)),
            ThreadEvent::TurnEnded(outcome) => {
                if let TurnOutcome::Failed { reason } = outcome {
                    self.emit(ThreadsEvent::TurnFailed(reason));
                }
                // A turn may have retitled the thread, and made it the newest.
                self.refresh_open().await;
            }
            _ => {}
        }
    }

    /// Follows the call's room: subscribes once it connects, and drops the
    /// subscription once it is gone. Every start joins a new room.
    fn sync_room(&mut self) {
        let Some(open) = &mut self.open else {
            return;
        };
        let connected = open
            .state
            .borrow_and_update()
            .call
            .connection
            .is_connected();
        match (connected, open.room_events.is_some()) {
            (true, false) => open.room_events = open.thread.call().room().map(|r| r.subscribe()),
            (false, true) => open.room_events = None,
            _ => return,
        }
        self.refresh_avatar();
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
            .open
            .as_ref()
            .filter(|open| open.room_events.is_some())
            .and_then(|open| open.thread.call().agent_avatar_video_track());
        let sid = track.as_ref().map(|track| track.sid());
        if sid != self.avatar_sid {
            self.avatar_sid = sid;
            self.emit(ThreadsEvent::Avatar(track));
        }
    }

    fn emit(&self, event: ThreadsEvent) {
        let _ = self.ui_tx.send(event);
    }
}

/// The next input of the open thread; pending while none is open.
async fn next(open: &mut Option<Open>) -> Input {
    match open {
        Some(open) => open.next().await,
        None => std::future::pending().await,
    }
}

/// The next message of `rx`; pending while there is none.
async fn recv<T>(rx: &mut Option<mpsc::UnboundedReceiver<T>>) -> Option<T> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}
