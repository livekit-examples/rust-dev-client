use crate::room::RoomContext;
use crate::service::{AsyncCmd, DataStreamPayload, LkService};
use crate::style::Palette;
use crate::ui::status_badge::StatusBadge;
use egui::{RichText, Theme};
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

/// A single received stream, rendered as a card.
struct ReceivedStream {
    n: u64,
    sender: String,
    received_at: SystemTime,
    size: usize,
    preview: String,
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
    sub_error: Option<String>,
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
            sub_error: None,
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
        service.runtime().spawn(async move {
            let (size, preview) = match reader.read_all().await {
                Ok(text) => (text.len(), truncate_chars(&text, PREVIEW_CHARS)),
                Err(e) => (0, format!("<error: {}>", e)),
            };
            push_received(&entry, identity.as_str(), size, preview);
        });
    }

    /// Routes an incoming byte stream to a matching subscription (if any).
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
        service.runtime().spawn(async move {
            let (size, preview) = match reader.read_all().await {
                Ok(data) => (data.len(), bytes_preview(data.as_ref())),
                Err(e) => (0, format!("<error: {}>", e)),
            };
            push_received(&entry, identity.as_str(), size, preview);
        });
    }

    fn show_send(&mut self, ui: &mut egui::Ui, ctx: &RoomContext, room: &Room) {
        ui.label(RichText::new("Send Data Stream").strong());

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

    fn show_subscribe(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Subscriptions").strong());

        let mut do_add = false;
        ui.horizontal(|ui| {
            ui.label("Topic:");
            let resp = ui.add(egui::TextEdit::singleline(&mut self.sub_topic).desired_width(120.0));
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                do_add = true;
            }
            ui.radio_value(&mut self.sub_kind, StreamKind::Text, "Text");
            ui.radio_value(&mut self.sub_kind, StreamKind::Bytes, "Bytes");
            if ui.button("Add").clicked() {
                do_add = true;
            }
        });

        if do_add {
            self.sub_error = None;
            let topic = self.sub_topic.trim().to_string();
            let key = (topic.clone(), self.sub_kind);
            if topic.is_empty() {
                self.sub_error = Some("Topic is empty".to_string());
            } else if self.subscriptions.contains_key(&key) {
                self.sub_error =
                    Some(format!("Already subscribed to '{}' ({})", topic, self.sub_kind.label()));
            } else {
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
        }

        if let Some(err) = &self.sub_error {
            ui.add(StatusBadge::error(err.clone()));
        }
    }

    fn show_subscription_cards(&mut self, ui: &mut egui::Ui, ctx: &RoomContext) {
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
                                let meta = format!(
                                    "#{} | {} | {} | {}",
                                    r.n,
                                    r.sender,
                                    format_size(r.size),
                                    format_ts(r.received_at),
                                );
                                ui.weak(RichText::new(meta).small());
                                ui.add(egui::Label::new(RichText::new(&r.preview).monospace()));
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
                    state.show_send(ui, ctx, room);
                    ui.add_space(8.0);
                    ui.separator();
                    state.show_subscribe(ui);
                    state.show_subscription_cards(ui, ctx);
                });
        })
        .response
    }
}

fn push_received(
    entry: &Arc<Mutex<TopicEntry>>,
    sender: &str,
    size: usize,
    preview: String,
) {
    let mut g = entry.lock();
    g.count += 1;
    let n = g.count;
    g.received.push_back(ReceivedStream {
        n,
        sender: sender.to_string(),
        received_at: SystemTime::now(),
        size,
        preview,
    });
    while g.received.len() > MAX_RECEIVED {
        g.received.pop_front();
    }
}

/// Parses a hex string into bytes, ignoring whitespace, commas and colons.
fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String =
        s.chars().filter(|c| !c.is_whitespace() && *c != ',' && *c != ':').collect();
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
    let ellipsis = if data.len() > PREVIEW_BYTES { "..." } else { "" };
    let text = String::from_utf8_lossy(shown);
    format!("hex: {}{}\nutf8: {}{}", hex.trim_end(), ellipsis, text, ellipsis)
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

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else {
        format!("{:.2}KB", bytes as f64 / 1024.0)
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
