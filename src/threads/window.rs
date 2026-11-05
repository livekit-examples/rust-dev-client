use crate::{
    agent::{avatar_panel::AvatarPanel, feed::MessageFeed},
    connect::ThreadConnectSettings,
    media::VideoRenderer,
    threads::service::{OpenedThread, ThreadsCmd, ThreadsEvent, ThreadsService},
    ui::{chat_composer::ChatComposer, status_badge::StatusBadge},
};
use livekit_thread::{Message, ThreadInfo, ThreadState, TurnState};
use std::collections::BTreeMap;
use tokio::sync::watch;

const CALL_BUTTON_WIDTH: f32 = 100.0;

/// The threads of one scope on the left, the selected thread's chat and call on
/// the right. The list loads immediately on creation.
pub struct ThreadsWindow {
    runtime: tokio::runtime::Handle,
    render_state: egui_wgpu::RenderState,
    service: ThreadsService,
    scope: String,
    threads: Vec<ThreadRow>,
    /// Whether the list has a page left to load.
    has_more: bool,
    listing: bool,
    list_failure: Option<String>,
    /// The thread last clicked or opened, highlighted in the list.
    selected: Option<String>,
    chat: Chat,
}

/// A thread of the list, formatted once when the list arrives.
struct ThreadRow {
    id: String,
    title: String,
    last_active: String,
}

impl From<ThreadInfo> for ThreadRow {
    fn from(info: ThreadInfo) -> Self {
        let last_active = info
            .last_active
            .map(|time| {
                time.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        Self {
            title: title(&info).to_owned(),
            id: info.id,
            last_active,
        }
    }
}

enum Chat {
    Empty,
    Opening,
    Failed(String),
    Open(Box<OpenThread>),
}

/// The open thread's feed, composer and call.
struct OpenThread {
    id: String,
    /// The `title` attribute, or the id while the thread has none.
    title: String,
    /// The feed keyed by [`Message::seq`], which a growing message keeps.
    messages: BTreeMap<u64, Message>,
    state: watch::Receiver<ThreadState>,
    draft: String,
    send_failure: Option<String>,
    turn_failure: Option<String>,
    call: CallView,
}

#[derive(Default)]
struct CallView {
    starting: bool,
    failure: Option<String>,
    /// The call's room, once it started.
    room: Option<String>,
    avatar: Option<VideoRenderer>,
}

impl From<OpenedThread> for OpenThread {
    fn from(opened: OpenedThread) -> Self {
        Self {
            title: title(&opened.info).to_owned(),
            id: opened.info.id,
            messages: opened
                .messages
                .into_iter()
                .map(|message| (message.seq, message))
                .collect(),
            state: opened.state,
            draft: String::new(),
            send_failure: None,
            turn_failure: None,
            call: CallView::default(),
        }
    }
}

impl ThreadsWindow {
    pub fn new(
        runtime: tokio::runtime::Handle,
        render_state: egui_wgpu::RenderState,
        settings: ThreadConnectSettings,
    ) -> Self {
        Self {
            scope: settings.directory.scope.clone(),
            service: ThreadsService::new(&runtime, settings),
            runtime,
            render_state,
            threads: Vec::new(),
            has_more: false,
            listing: true,
            list_failure: None,
            selected: None,
            chat: Chat::Empty,
        }
    }

    /// Ends the open thread and its call.
    pub fn disconnect(&self) {
        self.service.send(ThreadsCmd::Close);
    }

    fn refresh(&mut self) {
        self.listing = true;
        self.service.send(ThreadsCmd::Refresh);
    }

    fn load_more(&mut self) {
        self.listing = true;
        self.service.send(ThreadsCmd::LoadMore);
    }

    fn open(&mut self, id: String) {
        self.selected = Some(id.clone());
        self.chat = Chat::Opening;
        self.service.send(ThreadsCmd::Open(id));
    }

    fn create(&mut self) {
        self.selected = None;
        self.chat = Chat::Opening;
        self.service.send(ThreadsCmd::Create);
    }

    /// The open thread; events for it are stale while another one opens.
    fn open_thread(&mut self) -> Option<&mut OpenThread> {
        match &mut self.chat {
            Chat::Open(thread) => Some(thread),
            _ => None,
        }
    }

    fn event(&mut self, event: ThreadsEvent) {
        match event {
            ThreadsEvent::Listed(listed) => {
                self.listing = false;
                match listed {
                    Ok(page) => {
                        if !page.continues {
                            self.threads.clear();
                        }
                        self.threads
                            .extend(page.threads.into_iter().map(ThreadRow::from));
                        self.has_more = page.has_more;
                        self.list_failure = None;
                    }
                    Err(err) => self.list_failure = Some(err),
                }
            }
            ThreadsEvent::Updated(info) => {
                if let Some(thread) = self.open_thread().filter(|thread| thread.id == info.id) {
                    thread.title = title(&info).to_owned();
                }
                // The newest thread comes first.
                self.threads.retain(|row| row.id != info.id);
                self.threads.insert(0, ThreadRow::from(info));
            }
            ThreadsEvent::Opened(Ok(opened)) => {
                self.selected = Some(opened.info.id.clone());
                self.chat = Chat::Open(Box::new(OpenThread::from(opened)));
            }
            ThreadsEvent::Opened(Err(err)) => self.chat = Chat::Failed(err),
            ThreadsEvent::Message(message) => {
                if let Some(thread) = self.open_thread() {
                    thread.messages.insert(message.seq, message);
                }
            }
            ThreadsEvent::SendFailed(err) => {
                if let Some(thread) = self.open_thread() {
                    thread.send_failure = Some(err);
                }
            }
            ThreadsEvent::TurnFailed(err) => {
                if let Some(thread) = self.open_thread() {
                    thread.turn_failure = Some(err);
                }
            }
            ThreadsEvent::CallStarted(started) => {
                let Some(thread) = self.open_thread() else {
                    return;
                };
                thread.call.starting = false;
                match started {
                    Ok(room) => thread.call.room = Some(room),
                    Err(err) => thread.call.failure = Some(err),
                }
            }
            ThreadsEvent::Avatar(track) => {
                let avatar = track.map(|track| {
                    VideoRenderer::new(&self.runtime, self.render_state.clone(), track.rtc_track())
                });
                if let Some(thread) = self.open_thread() {
                    thread.call.avatar = avatar;
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        while let Some(event) = self.service.try_recv() {
            self.event(event);
        }

        let half = ui.available_width() / 2.0;
        egui::Panel::left("thread_list")
            .resizable(true)
            .default_size(half)
            .show(ui, |ui| self.thread_list(ui));

        egui::CentralPanel::default().show(ui, |ui| match &mut self.chat {
            Chat::Empty => {
                ui.centered_and_justified(|ui| {
                    ui.weak("Select a thread, or start a new one");
                });
            }
            Chat::Opening => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Opening thread...");
                });
            }
            Chat::Failed(err) => {
                ui.add(StatusBadge::error(format!("Failed to open thread: {err}")));
            }
            Chat::Open(thread) => {
                if let Some(cmd) = thread.ui(ui) {
                    self.service.send(cmd);
                }
            }
        });

        ui.ctx().request_repaint();
    }

    fn thread_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Threads");
            egui::Aligned::new(egui::Align2::RIGHT_CENTER)
                .id_salt("thread_list_actions")
                .show(ui, |ui| {
                    if ui
                        .button("New thread")
                        .on_hover_text("Create a thread and open it")
                        .clicked()
                    {
                        self.create();
                    }
                    let reload = ui
                        .add_enabled(!self.listing, egui::Button::new("⟳"))
                        .on_hover_text("Reload the threads");
                    if reload.clicked() {
                        self.refresh();
                    }
                });
        });
        ui.weak(format!("Scope: {}", self.scope));
        ui.separator();

        if let Some(err) = &self.list_failure {
            ui.add(StatusBadge::error(format!("Failed to list threads: {err}")));
        } else if self.threads.is_empty() {
            ui.weak(if self.listing {
                "Loading..."
            } else {
                "No threads yet"
            });
        }

        let mut clicked = None;
        let mut load_more = false;
        let row_height = ui.spacing().interact_size.y;
        // The row after the last thread loads the next page.
        let rows = self.threads.len() + usize::from(self.has_more);
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            row_height,
            rows,
            |ui, rows| {
                for index in rows {
                    let Some(row) = self.threads.get(index) else {
                        let button = egui::Button::new("Load more")
                            .min_size(egui::vec2(ui.available_width(), row_height));
                        load_more = ui
                            .add_enabled(!self.listing, button)
                            .on_hover_text("List the next page of threads")
                            .clicked();
                        continue;
                    };
                    let selected = self.selected.as_deref() == Some(row.id.as_str());
                    let button = egui::Button::selectable(selected, row.title.as_str())
                        .right_text(egui::RichText::new(row.last_active.as_str()).weak())
                        .truncate()
                        .min_size(egui::vec2(ui.available_width(), row_height));
                    let response = ui
                        .push_id(&row.id, |ui| ui.add(button))
                        .inner
                        .on_hover_text(&row.id);
                    if response.clicked() && !selected {
                        clicked = Some(row.id.clone());
                    }
                }
            },
        );
        if let Some(id) = clicked {
            self.open(id);
        }
        if load_more {
            self.load_more();
        }
    }
}

impl OpenThread {
    /// Returns the command a click or a sent message asks for.
    fn ui(&mut self, ui: &mut egui::Ui) -> Option<ThreadsCmd> {
        let mut cmd = None;
        let state = *self.state.borrow();
        let in_call = state.call.connection.is_connected();

        egui::Panel::top("thread_header")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(&self.title).on_hover_text(&self.id);
                    if state.turn == TurnState::Working {
                        ui.spinner();
                        ui.weak("Agent working...");
                    }
                    egui::Aligned::new(egui::Align2::RIGHT_CENTER)
                        .id_salt("call_button")
                        .show(ui, |ui| {
                            if let Some(call_cmd) = self.call_button(ui, in_call) {
                                cmd = Some(call_cmd);
                            }
                        });
                });
                if let Some(err) = &self.call.failure {
                    ui.add(StatusBadge::error(format!("Call failed: {err}")));
                }
            });

        egui::Panel::bottom("thread_composer")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                let mut submitted = None;
                ui.add(
                    ChatComposer::new(&mut self.draft, &mut submitted)
                        .hint_text("Message the agent"),
                );
                if let Some(text) = submitted {
                    self.send_failure = None;
                    self.turn_failure = None;
                    cmd = Some(ThreadsCmd::SendText(text));
                }
                if let Some(err) = &self.send_failure {
                    ui.add(StatusBadge::error(format!("Failed to send: {err}")));
                }
                if let Some(err) = &self.turn_failure {
                    ui.add(StatusBadge::error(format!("Agent turn failed: {err}")));
                }
            });

        if in_call {
            ui.add(AvatarPanel {
                state: state.call.agent,
                avatar: self.call.avatar.as_ref(),
            });
            ui.separator();
        }
        ui.add(MessageFeed::new(&self.messages, &self.id));
        cmd
    }

    /// Starting / End call / Start call.
    fn call_button(&mut self, ui: &mut egui::Ui, in_call: bool) -> Option<ThreadsCmd> {
        let size = egui::vec2(CALL_BUTTON_WIDTH, 0.0);
        if self.call.starting {
            ui.add_enabled(false, egui::Button::new("Starting...").min_size(size));
            return None;
        }
        if in_call {
            let room = self.call.room.as_deref().unwrap_or_default();
            let end = ui
                .add(egui::Button::new("End call").min_size(size))
                .on_hover_text(format!("Leave the room '{room}'"));
            return end.clicked().then_some(ThreadsCmd::EndCall);
        }
        let start = ui
            .add(egui::Button::new("Start call").min_size(size))
            .on_hover_text("Join a new room with the agent, continuing this thread");
        if !start.clicked() {
            return None;
        }
        self.call.starting = true;
        self.call.failure = None;
        Some(ThreadsCmd::StartCall)
    }
}

/// A thread's title, or its id while it has none.
fn title(info: &ThreadInfo) -> &str {
    info.title.as_deref().unwrap_or(&info.id)
}
