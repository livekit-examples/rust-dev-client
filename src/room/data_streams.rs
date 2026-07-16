use crate::room::RoomContext;
use crate::service::{AsyncCmd, DataStreamPayload, LkService};
use crate::ui::status_badge::StatusBadge;
use egui::RichText;
use egui::collapsing_header::CollapsingState;
use futures::StreamExt;
use livekit::prelude::*;
use livekit::{ByteStreamReader, StreamReader, TakeCell, TextStreamReader};
use parking_lot::Mutex;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RECEIVED: usize = 100;
const PREVIEW_CHARS: usize = 256;
const PREVIEW_BYTES: usize = 64;

static NEXT_SEND_ID: AtomicU64 = AtomicU64::new(1);

/// Whether a stream carries text or raw bytes. Used for both sending and subscribing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StreamKind {
    Text,
    Bytes,
}

impl StreamKind {
    fn label(self) -> &'static str {
        match self {
            StreamKind::Text => "text",
            StreamKind::Bytes => "bytes",
        }
    }
}

/// Progress/lifecycle of a received stream as it is read in the background.
enum StreamStatus {
    InProgress,
    Complete,
    Error(String),
}

/// What a received stream renders as, filled in as the payload arrives.
enum StreamContent {
    /// Text, or a non-file byte stream: an inline hex/utf8 preview string.
    Preview(String),
    /// A file-like byte stream, streamed to an anonymous temp file. The `file`
    /// handle is set once the download completes; dropping it lets the OS reclaim
    /// the (already-unlinked) temp file.
    File { file: Arc<std::fs::File> },
}

/// A single received stream, rendered as a row (metadata line + preview/file box).
/// A row is created as soon as the stream opens and updated live from the read task.
struct ReceivedStream {
    n: u64,
    sender: String,
    received_at: SystemTime,
    /// Bytes processed so far (or final size once complete).
    size: u64,
    /// Completion fraction (`0.0..=1.0`) from [`StreamProgress::percentage`], or
    /// `None` for streams of unknown total size.
    progress: Option<f32>,
    status: StreamStatus,
    /// Decided at open time from `info()`: file-like byte streams render as a file box.
    is_file: bool,
    /// File name / mime type from `info()` (used by the file box and the Save default).
    file_name: String,
    mime_type: String,
    content: Option<StreamContent>,
    /// Feedback from the most recent Save: `Ok(path)` or `Err(message)`.
    save: Option<Result<String, String>>,
}

/// Accumulates streams received for one (topic, kind) subscription.
struct TopicEntry {
    topic: String,
    kind: StreamKind,
    received: VecDeque<ReceivedStream>,
    count: u64,
}

/// Persistent state for the Data Streams tab. The send half mirrors the RPC tab:
/// the send goes through the service (`AsyncCmd::DataStreamSend`) and the result
/// comes back via `handle_send_result`, tracked by `send_in_flight`. The receive
/// half keeps a shared `Arc<Mutex<TopicEntry>>` per subscription, written from a
/// spawned read task (same pattern the RPC handler invocations use).
pub struct DataStreamsUiState {
    // Send section
    send_kind: StreamKind,
    send_topic: String,
    send_destination: Option<ParticipantIdentity>,
    send_content: String,
    send_hex: bool,
    send_in_flight: Option<u64>,
    send_result: Option<Result<String, String>>,

    // Subscribe section
    sub_topic: String,
    sub_kind: StreamKind,
    subscriptions: BTreeMap<(String, StreamKind), Arc<Mutex<TopicEntry>>>,
}

impl Default for DataStreamsUiState {
    fn default() -> Self {
        Self {
            send_kind: StreamKind::Text,
            send_topic: String::new(),
            send_destination: None,
            send_content: String::new(),
            send_hex: false,
            send_in_flight: None,
            send_result: None,
            sub_topic: String::new(),
            sub_kind: StreamKind::Text,
            subscriptions: BTreeMap::new(),
        }
    }
}

impl DataStreamsUiState {
    /// Resolves a pending send dispatched via [`AsyncCmd::DataStreamSend`].
    pub fn handle_send_result(&mut self, request_id: u64, result: Result<String, String>) {
        if self.send_in_flight == Some(request_id) {
            self.send_in_flight = None;
        }
        self.send_result = Some(result);
    }

    pub fn on_disconnect(&mut self) {
        // Subscriptions are pure UI filters (no room registration), so we keep them but clear
        // the streams received during the previous session.
        for entry in self.subscriptions.values() {
            let mut g = entry.lock();
            g.received.clear();
            g.count = 0;
        }
        self.send_in_flight = None;
        self.send_result = None;
        self.send_destination = None;
    }

    /// Routes an incoming text stream to a matching subscription (if any), reading it in the
    /// background. Unmatched streams are dropped (the reader is never taken).
    pub fn on_text_stream(
        &mut self,
        reader: TakeCell<TextStreamReader>,
        topic: String,
        identity: ParticipantIdentity,
        service: &LkService,
    ) {
        let Some(entry) = self.subscriptions.get(&(topic, StreamKind::Text)).cloned() else {
            return;
        };
        let Some(reader) = reader.take() else {
            return;
        };
        // Text streams always render as an inline preview.
        let total = reader.info().total_length;
        let n = push_pending(
            &entry,
            identity.as_str(),
            total,
            false,
            String::new(),
            String::new(),
        );
        service.runtime().spawn(async move {
            // We drive the reader as a `Stream` and derive the percentage from chunk lengths
            // (the same fraction as `StreamProgress::percentage`). `progress()` is not used
            // here: as a return-position `impl Trait` in a trait it borrows `&reader` for the
            // life of the progress stream, which is incompatible with concurrently consuming
            // the reader — and consuming it is exactly what lets us stream without buffering.
            let mut reader = reader;
            let mut text = String::new();
            let mut processed = 0u64;
            let mut error = None;
            while let Some(chunk) = reader.next().await {
                match chunk {
                    Ok(s) => {
                        processed += s.len() as u64;
                        text.push_str(&s);
                        update_progress(&entry, n, processed, total);
                    }
                    Err(e) => {
                        error = Some(e.to_string());
                        break;
                    }
                }
            }
            match error {
                Some(e) => finish_error(&entry, n, e),
                None => {
                    let preview = truncate_chars(&text, PREVIEW_CHARS);
                    update_row(&entry, n, |r| {
                        r.size = processed;
                        r.content = Some(StreamContent::Preview(preview));
                        r.status = StreamStatus::Complete;
                    });
                }
            }
        });
    }

    /// Routes an incoming byte stream to a matching subscription (if any). File-like byte
    /// streams (see [`is_file_stream`]) are streamed to an anonymous temp file so a whole
    /// file never needs to sit in memory; everything else keeps the inline hex/utf8 preview.
    pub fn on_byte_stream(
        &mut self,
        reader: TakeCell<ByteStreamReader>,
        topic: String,
        identity: ParticipantIdentity,
        service: &LkService,
    ) {
        let Some(entry) = self.subscriptions.get(&(topic, StreamKind::Bytes)).cloned() else {
            return;
        };
        let Some(reader) = reader.take() else {
            return;
        };
        let info = reader.info();
        let total = info.total_length;
        let is_file = is_file_stream(&info.name, &info.mime_type);
        let file_name = default_file_name(&info.name, &info.id);
        let mime_type = info.mime_type.clone();
        let n = push_pending(
            &entry,
            identity.as_str(),
            total,
            is_file,
            file_name,
            mime_type,
        );

        service.runtime().spawn(async move {
            // See the note in `on_text_stream`: we consume the reader directly and compute
            // progress from chunk lengths rather than using `progress()`.
            let mut reader = reader;
            if is_file {
                // File-like: stream chunks straight to an anonymous temp file so a whole file
                // never sits in memory.
                match stream_to_temp_file(&mut reader, &entry, n, total).await {
                    Ok(file) => update_row(&entry, n, |r| {
                        r.content = Some(StreamContent::File {
                            file: Arc::new(file),
                        });
                        r.status = StreamStatus::Complete;
                    }),
                    Err(e) => finish_error(&entry, n, e),
                }
            } else {
                // Non-file bytes: small dev payloads, kept in memory for an inline preview.
                let mut buf = Vec::new();
                let mut processed = 0u64;
                let mut error = None;
                while let Some(chunk) = reader.next().await {
                    match chunk {
                        Ok(b) => {
                            processed += b.len() as u64;
                            buf.extend_from_slice(&b);
                            update_progress(&entry, n, processed, total);
                        }
                        Err(e) => {
                            error = Some(e.to_string());
                            break;
                        }
                    }
                }
                match error {
                    Some(e) => finish_error(&entry, n, e),
                    None => update_row(&entry, n, |r| {
                        r.size = processed;
                        r.content = Some(StreamContent::Preview(bytes_preview(&buf)));
                        r.status = StreamStatus::Complete;
                    }),
                }
            }
        });
    }

    fn show_send(&mut self, ui: &mut egui::Ui, ctx: &RoomContext, room: &Room) {
        ui.horizontal(|ui| {
            ui.label("Kind:");
            ui.radio_value(&mut self.send_kind, StreamKind::Text, "Text");
            ui.radio_value(&mut self.send_kind, StreamKind::Bytes, "Bytes");
        });

        ui.horizontal(|ui| {
            ui.label("Topic:");
            ui.add(egui::TextEdit::singleline(&mut self.send_topic).desired_width(f32::INFINITY));
        });

        // Destination picker: broadcast (None) or a specific remote participant.
        let participants = room.remote_participants();
        let mut idents: Vec<ParticipantIdentity> = participants.keys().cloned().collect();
        idents.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        if let Some(sel) = self.send_destination.as_ref()
            && !participants.contains_key(sel)
        {
            self.send_destination = None;
        }
        ui.horizontal(|ui| {
            ui.label("To:");
            let combo_label = self
                .send_destination
                .as_ref()
                .map(|i| i.as_str().to_string())
                .unwrap_or_else(|| "Everyone (broadcast)".to_string());
            egui::ComboBox::from_id_salt(ctx.id.with("ds_dest_combo"))
                .selected_text(combo_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.send_destination, None, "Everyone (broadcast)");
                    for ident in &idents {
                        ui.selectable_value(
                            &mut self.send_destination,
                            Some(ident.clone()),
                            ident.as_str(),
                        );
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Content:");
            if self.send_kind == StreamKind::Text {
                if ui.small_button("Hello").clicked() {
                    self.send_content = "Hello world".to_string();
                }
                if ui.small_button("20k").clicked() {
                    self.send_content = "X".repeat(20_000);
                }
            } else {
                ui.checkbox(&mut self.send_hex, "Hex");
            }
        });
        let max_h = ui.text_style_height(&egui::TextStyle::Body) * 5.0 + 8.0;
        egui::ScrollArea::vertical()
            .id_salt(ctx.id.with("ds_send_content_scroll"))
            .max_height(max_h)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.send_content)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
            });

        let can_send = self.send_in_flight.is_none() && !self.send_topic.trim().is_empty();

        ui.horizontal(|ui| {
            ui.add_enabled_ui(can_send, |ui| {
                if ui.button("Send").clicked() {
                    self.dispatch_send(ctx);
                }
            });
            if self.send_in_flight.is_some() {
                ui.spinner();
            }
        });

        if self.send_in_flight.is_some() {
            ui.add(StatusBadge::neutral("Sending..."));
        } else {
            match &self.send_result {
                Some(Ok(id)) => {
                    ui.add(StatusBadge::ok(format!("OK: stream {}", short_id(id))));
                }
                Some(Err(e)) => {
                    ui.add(StatusBadge::error(format!("Error: {}", e)));
                }
                None => {}
            }
        }
    }

    fn dispatch_send(&mut self, ctx: &RoomContext) {
        let topic = self.send_topic.trim().to_string();
        let payload = match self.send_kind {
            StreamKind::Text => DataStreamPayload::Text(self.send_content.clone()),
            StreamKind::Bytes => {
                if self.send_hex {
                    match parse_hex(&self.send_content) {
                        Ok(b) => DataStreamPayload::Bytes(b),
                        Err(e) => {
                            self.send_result = Some(Err(format!("invalid hex: {}", e)));
                            return;
                        }
                    }
                } else {
                    DataStreamPayload::Bytes(self.send_content.as_bytes().to_vec())
                }
            }
        };
        let request_id = NEXT_SEND_ID.fetch_add(1, Ordering::Relaxed);
        self.send_in_flight = Some(request_id);
        self.send_result = None;
        let _ = ctx.service.send(AsyncCmd::DataStreamSend {
            request_id,
            topic,
            destination: self.send_destination.clone(),
            payload,
        });
    }

    /// The "Subscriptions" collapsible: a header with an "add" button that opens
    /// the add popup, and a body listing one card per subscription. Mirrors
    /// [`crate::room::rpc`]'s `HandlersState::show`.
    fn show_subscriptions(&mut self, ui: &mut egui::Ui, ctx: &RoomContext) {
        let header_id = ctx.id.with("ds_subscriptions_section");
        let header = CollapsingState::load_with_default_open(ui.ctx(), header_id, true)
            .show_header(ui, |ui| {
                ui.label(format!("Subscriptions ({})", self.subscriptions.len()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let add = ui.button("➕").on_hover_text("Add subscription");
                    // The popup body only renders while open, so a click on the
                    // button here means it was just opened — focus the field then.
                    let just_opened = add.clicked();
                    egui::Popup::from_toggle_button_response(&add)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| self.add_form(ui, just_opened));
                });
            });
        // The add is applied inside the popup, before the cards render, so both of
        // egui's layout passes see the same subscription set on the frame one is
        // added (deferring it to after `body` would render N cards in pass 1 and
        // N+1 in pass 2, tripping the id-stability check).
        header.body(|ui| self.subscription_cards(ui, ctx));
    }

    /// The add popup's contents: a topic field, the Text/Bytes kind, and an Add
    /// button (disabled while the topic is empty or already subscribed).
    fn add_form(&mut self, ui: &mut egui::Ui, focus_topic: bool) {
        let mut do_add = false;
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.sub_topic)
                    .desired_width(120.0)
                    .hint_text("Topic"),
            );
            if focus_topic {
                resp.request_focus();
            }
            ui.radio_value(&mut self.sub_kind, StreamKind::Text, "Text");
            ui.radio_value(&mut self.sub_kind, StreamKind::Bytes, "Bytes");

            let topic = self.sub_topic.trim();
            let can_add = !topic.is_empty()
                && !self
                    .subscriptions
                    .contains_key(&(topic.to_string(), self.sub_kind));
            if can_add && resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                do_add = true;
            }
            if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                do_add = true;
            }
        });
        if do_add {
            self.add_subscription();
            ui.close();
        }
    }

    fn add_subscription(&mut self) {
        let topic = self.sub_topic.trim().to_string();
        let key = (topic.clone(), self.sub_kind);
        // The Add button is disabled for empty or already-subscribed topics, so a
        // duplicate here is a no-op rather than an error.
        if topic.is_empty() || self.subscriptions.contains_key(&key) {
            return;
        }
        self.subscriptions.insert(
            key,
            Arc::new(Mutex::new(TopicEntry {
                topic,
                kind: self.sub_kind,
                received: VecDeque::new(),
                count: 0,
            })),
        );
        self.sub_topic.clear();
    }

    fn subscription_cards(&mut self, ui: &mut egui::Ui, ctx: &RoomContext) {
        let keys: Vec<(String, StreamKind)> = self.subscriptions.keys().cloned().collect();
        let mut to_remove: Option<(String, StreamKind)> = None;

        for key in keys {
            let entry = self.subscriptions.get(&key).unwrap().clone();
            ui.add_space(6.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let guard = entry.lock();
                ui.horizontal(|ui| {
                    ui.monospace(RichText::new(&guard.topic).strong());
                    ui.weak(RichText::new(format!("[{}]", guard.kind.label())).small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Remove").clicked() {
                            to_remove = Some(key.clone());
                        }
                    });
                });

                ui.label(format!("Received ({})", guard.count));
                if guard.received.is_empty() {
                    ui.weak("Nothing received yet");
                } else {
                    let max_h = ui.text_style_height(&egui::TextStyle::Body) * 10.0 + 8.0;
                    egui::ScrollArea::vertical()
                        .id_salt(ctx.id.with((
                            "ds_recv_scroll",
                            guard.topic.as_str(),
                            guard.kind.label(),
                        )))
                        .max_height(max_h)
                        .show(ui, |ui| {
                            for r in guard.received.iter() {
                                ui.push_id(r.n, |ui| show_received(ui, ctx.service, &entry, r));
                                ui.separator();
                            }
                        });
                }
            });
        }

        if let Some(key) = to_remove {
            self.subscriptions.remove(&key);
        }
    }
}

/// Widget: the Data Streams tab (send form + topic subscriptions) inside a
/// scroll area. Mirrors [`crate::room::rpc::RpcPanel`].
pub struct DataStreamsPanel<'a> {
    pub state: &'a mut DataStreamsUiState,
    pub ctx: &'a RoomContext<'a>,
}

impl egui::Widget for DataStreamsPanel<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let DataStreamsPanel { state, ctx } = self;
        ui.scope(|ui| {
            egui::ScrollArea::vertical()
                .id_salt(ctx.id.with("data_streams_scroll"))
                .show(ui, |ui| {
                    let Some(room) = ctx.room else {
                        ui.label("Not connected");
                        return;
                    };
                    egui::CollapsingHeader::new("Send Data Stream")
                        .default_open(true)
                        .show(ui, |ui| state.show_send(ui, ctx, room));
                    state.show_subscriptions(ui, ctx);
                });
        })
        .response
    }
}

/// Pushes an `InProgress` row for a freshly opened stream and returns its sequence number
/// `n`, which the background read task uses to update the same row via [`update_row`].
fn push_pending(
    entry: &Arc<Mutex<TopicEntry>>,
    sender: &str,
    total: Option<u64>,
    is_file: bool,
    file_name: String,
    mime_type: String,
) -> u64 {
    let mut g = entry.lock();
    g.count += 1;
    let n = g.count;
    g.received.push_back(ReceivedStream {
        n,
        sender: sender.to_string(),
        received_at: SystemTime::now(),
        size: 0,
        // A known total starts at 0%; an unknown total has no percentage.
        progress: total.map(|_| 0.0),
        status: StreamStatus::InProgress,
        is_file,
        file_name,
        mime_type,
        content: None,
        save: None,
    });
    while g.received.len() > MAX_RECEIVED {
        g.received.pop_front();
    }
    n
}

/// Locks `entry` and applies `f` to the row with sequence number `n`, if it still exists.
/// Rows can be evicted (`MAX_RECEIVED`) while a read is in flight; in that case this is a
/// no-op and the read result is simply dropped.
fn update_row(entry: &Arc<Mutex<TopicEntry>>, n: u64, f: impl FnOnce(&mut ReceivedStream)) {
    let mut g = entry.lock();
    if let Some(row) = g.received.iter_mut().find(|r| r.n == n) {
        f(row);
    }
}

/// Updates a row's byte count and completion fraction from bytes-processed-so-far. The
/// fraction mirrors [`livekit::StreamProgress::percentage`]: `processed / total`, or `None`
/// when the total size is unknown.
fn update_progress(entry: &Arc<Mutex<TopicEntry>>, n: u64, processed: u64, total: Option<u64>) {
    update_row(entry, n, |r| {
        r.size = processed;
        r.progress = total.map(|t| {
            if t == 0 {
                1.0
            } else {
                processed as f32 / t as f32
            }
        });
    });
}

fn finish_error(entry: &Arc<Mutex<TopicEntry>>, n: u64, message: String) {
    update_row(entry, n, |r| r.status = StreamStatus::Error(message));
}

/// A byte stream is treated as a downloadable file when it carries a name, or a meaningful
/// mime type (the generic `application/octet-stream` default does not count).
fn is_file_stream(name: &str, mime_type: &str) -> bool {
    !name.is_empty() || (!mime_type.is_empty() && mime_type != "application/octet-stream")
}

/// The file name shown in the file box and offered as the Save default. Falls back to a
/// name derived from the stream id when the sender did not set one.
fn default_file_name(name: &str, id: &str) -> String {
    if name.is_empty() {
        format!("stream-{}", short_id(id))
    } else {
        name.to_string()
    }
}

/// Streams every chunk of a byte reader into an anonymous temp file, returning the open
/// handle. The file is unlinked at creation (`tempfile`), so the OS reclaims it when the
/// handle is dropped or the process exits — including on a panic/abort, where `Drop` would
/// not run (the release profile sets `panic = "abort"`).
async fn stream_to_temp_file(
    reader: &mut ByteStreamReader,
    entry: &Arc<Mutex<TopicEntry>>,
    n: u64,
    total: Option<u64>,
) -> Result<std::fs::File, String> {
    use tokio::io::AsyncWriteExt;
    let std_file = tempfile::tempfile_in(std::env::temp_dir()).map_err(|e| e.to_string())?;
    let mut file = tokio::fs::File::from_std(std_file);
    let mut processed = 0u64;
    while let Some(chunk) = reader.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        processed += chunk.len() as u64;
        update_progress(entry, n, processed, total);
    }
    file.flush().await.map_err(|e| e.to_string())?;
    Ok(file.into_std().await)
}

/// Renders a single received-stream row: the `#n | sender | size (pct%) | ts` metadata line
/// followed by either an inline preview or a downloadable file box.
fn show_received(
    ui: &mut egui::Ui,
    service: &LkService,
    entry: &Arc<Mutex<TopicEntry>>,
    r: &ReceivedStream,
) {
    let size = match r.progress {
        Some(p) => format!("{} ({}%)", format_size(r.size), (p * 100.0).round() as i64),
        None => format_size(r.size),
    };
    let meta = format!(
        "#{} | {} | {} | {}",
        r.n,
        r.sender,
        size,
        format_ts(r.received_at)
    );
    ui.weak(RichText::new(meta).small());

    if let StreamStatus::Error(e) = &r.status {
        ui.colored_label(ui.visuals().error_fg_color, format!("<error: {}>", e));
        return;
    }

    if r.is_file {
        show_file_box(ui, service, entry, r);
    } else if let Some(StreamContent::Preview(preview)) = &r.content {
        ui.add(egui::Label::new(RichText::new(preview).monospace()));
    } else {
        ui.weak("receiving…");
    }
}

/// The file box for a file-like byte stream: icon + name + mime/size, a progress bar while
/// receiving, and a Save… button once complete.
fn show_file_box(
    ui: &mut egui::Ui,
    service: &LkService,
    entry: &Arc<Mutex<TopicEntry>>,
    r: &ReceivedStream,
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("📄");
            ui.vertical(|ui| {
                ui.monospace(RichText::new(&r.file_name).strong());
                let mime = if r.mime_type.is_empty() {
                    "application/octet-stream"
                } else {
                    &r.mime_type
                };
                ui.weak(RichText::new(format!("{} · {}", mime, format_size(r.size))).small());
            });
        });

        match (&r.status, &r.content) {
            (StreamStatus::Complete, Some(StreamContent::File { file })) => {
                if ui.button("Save…").clicked() {
                    spawn_save(
                        service,
                        entry.clone(),
                        r.n,
                        r.file_name.clone(),
                        file.clone(),
                    );
                }
            }
            _ => {
                let bar = egui::ProgressBar::new(r.progress.unwrap_or(0.0));
                let bar = match r.progress {
                    Some(_) => bar.show_percentage(),
                    None => bar.animate(true),
                };
                ui.add(bar);
            }
        }

        match &r.save {
            Some(Ok(path)) => {
                ui.weak(RichText::new(format!("Saved to {}", path)).small());
            }
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, format!("Save failed: {}", e));
            }
            None => {}
        }
    });
}

/// Opens a native save dialog (defaulting to `name`) and copies the received temp file to the
/// chosen destination, recording the outcome back on the row.
fn spawn_save(
    service: &LkService,
    entry: Arc<Mutex<TopicEntry>>,
    n: u64,
    name: String,
    file: Arc<std::fs::File>,
) {
    service.runtime().spawn(async move {
        let Some(dest) = rfd::AsyncFileDialog::new()
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return; // user cancelled
        };
        let path = dest.path().to_path_buf();
        let result = tokio::task::spawn_blocking(move || copy_file(&file, &path))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        update_row(&entry, n, |r| r.save = Some(result));
    });
}

/// Copies the contents of an open (anonymous) temp file to `dest`. Reads through `&File` so
/// the shared handle is not consumed and the file can be saved more than once.
fn copy_file(file: &std::fs::File, dest: &std::path::Path) -> Result<String, String> {
    use std::io::{Seek, SeekFrom, Write};
    let mut src = file;
    src.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut out = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    std::io::copy(&mut src, &mut out).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    Ok(dest.display().to_string())
}

/// Parses a hex string into bytes, ignoring whitespace, commas and colons.
fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != ':')
        .collect();
    if !cleaned.len().is_multiple_of(2) {
        return Err("odd number of hex digits".to_string());
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let pair = &cleaned[i..i + 2];
        let byte =
            u8::from_str_radix(pair, 16).map_err(|_| format!("invalid hex byte '{}'", pair))?;
        out.push(byte);
        i += 2;
    }
    Ok(out)
}

fn bytes_preview(data: &[u8]) -> String {
    let shown = &data[..data.len().min(PREVIEW_BYTES)];
    let hex: String = shown.iter().map(|b| format!("{:02x} ", b)).collect();
    let ellipsis = if data.len() > PREVIEW_BYTES {
        "..."
    } else {
        ""
    };
    let text = String::from_utf8_lossy(shown);
    format!(
        "hex: {}{}\nutf8: {}{}",
        hex.trim_end(),
        ellipsis,
        text,
        ellipsis
    )
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut iter = s.chars();
    let head: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        format!("{}...", head)
    } else {
        head
    }
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.2}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn format_ts(ts: SystemTime) -> String {
    let d = ts.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total = d.as_secs();
    let h = (total / 3600) % 24;
    let m = (total / 60) % 60;
    let s = total % 60;
    let ms = d.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}Z", h, m, s, ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_box_trigger() {
        // A name always makes it a file.
        assert!(is_file_stream("photo.png", ""));
        assert!(is_file_stream("data.bin", "application/octet-stream"));
        // A meaningful mime type (without a name) makes it a file.
        assert!(is_file_stream("", "image/png"));
        // The generic octet-stream default alone does NOT: it falls back to inline preview.
        assert!(!is_file_stream("", "application/octet-stream"));
        // No name, no mime: inline preview (e.g. the hex/20k send test).
        assert!(!is_file_stream("", ""));
    }

    #[test]
    fn file_name_defaults_to_stream_id() {
        assert_eq!(default_file_name("report.pdf", "stream-abc"), "report.pdf");
        // Unnamed streams derive a name from the (shortened) stream id.
        assert_eq!(default_file_name("", "abcdef0123456789"), "stream-abcdef01");
    }

    #[test]
    fn human_readable_sizes() {
        assert_eq!(format_size(512), "512B");
        assert_eq!(format_size(2048), "2.00KB");
        assert_eq!(format_size(3 * 1024 * 1024), "3.00MB");
    }
}
