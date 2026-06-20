use crate::room::RoomContext;
use crate::service::LkService;
use egui::collapsing_header::CollapsingState;
use livekit::prelude::*;
use parking_lot::Mutex;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

mod handler_card;
mod send_form;

use handler_card::RpcHandlerCard;
use send_form::RpcSendForm;

const MAX_INVOCATIONS: usize = 200;
const PAYLOAD_PREVIEW_CHARS: usize = 256;
const RESPONSE_PREVIEW_CHARS: usize = 40;

/// Persistent state for the RPC tab, split into the two disjoint halves the
/// sub-widgets borrow independently: the send form and the registered handlers.
#[derive(Default)]
pub struct RpcUiState {
    send: SendState,
    handlers: HandlersState,
}

#[derive(Default)]
struct SendState {
    destination: Option<ParticipantIdentity>,
    method: String,
    payload: String,
    in_flight: Option<u64>,
    result: Option<SendResult>,
}

#[derive(Default)]
struct HandlersState {
    register_method: String,
    entries: BTreeMap<String, Arc<Mutex<HandlerEntry>>>,
}

struct HandlerEntry {
    method: String,
    reply: String,
    invocations: VecDeque<Invocation>,
    invocation_count: u64,
}

struct Invocation {
    n: u64,
    caller: String,
    payload_len: usize,
    received_at: SystemTime,
    payload_preview: String,
}

enum SendResult {
    Ok(String),
    Err { code: u32, message: String },
}

impl RpcUiState {
    pub fn handle_send_result(&mut self, request_id: u64, result: Result<String, RpcError>) {
        if self.send.in_flight == Some(request_id) {
            self.send.in_flight = None;
        }
        self.send.result = Some(match result {
            Ok(s) => SendResult::Ok(s),
            Err(e) => SendResult::Err {
                code: e.code,
                message: e.message,
            },
        });
    }

    pub fn on_disconnect(&mut self) {
        self.handlers.entries.clear();
        self.send.in_flight = None;
        self.send.destination = None;
    }
}

impl HandlersState {
    /// The "Handlers" collapsible: a header with an "add" button that opens the
    /// register popup, and a body listing one [`RpcHandlerCard`] per handler.
    fn show(&mut self, ui: &mut egui::Ui, ctx: &RoomContext, room: &Room) {
        let header_id = ctx.id.with("rpc_handlers_section");
        let header = CollapsingState::load_with_default_open(ui.ctx(), header_id, true)
            .show_header(ui, |ui| {
                ui.label(format!("Handlers ({})", self.entries.len()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let add = ui.small_button("➕").on_hover_text("Register handler");
                    // The popup body only renders while open, so a click on the
                    // button here means it was just opened — focus the field then.
                    let just_opened = add.clicked();
                    egui::Popup::from_toggle_button_response(&add)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| self.register_form(ui, just_opened))
                        .is_some_and(|r| r.inner)
                })
                .inner
            });
        // Apply the registration only after the cards render, so the handler set
        // changes between frames at a single point (matching unregister) — this
        // keeps widget ids stable across egui's layout passes.
        let (_, header_ret, _) = header.body(|ui| self.handler_cards(ui, ctx, room));
        if header_ret.inner {
            self.register(ctx.service, room);
        }
    }

    /// The register popup's contents: a topic field and a Register button
    /// (disabled while the topic is empty or already registered). Returns whether
    /// a registration was requested; the caller applies it after rendering.
    fn register_form(&mut self, ui: &mut egui::Ui, focus_topic: bool) -> bool {
        let mut do_register = false;
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.register_method)
                    .desired_width(120.0)
                    .hint_text("Topic"),
            );
            if focus_topic {
                resp.request_focus();
            }
            let topic = self.register_method.trim();
            let can_register = !topic.is_empty() && !self.entries.contains_key(topic);
            if can_register && resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                do_register = true;
            }
            if ui
                .add_enabled(can_register, egui::Button::new("Register"))
                .clicked()
            {
                do_register = true;
            }
        });
        if do_register {
            ui.close();
        }
        do_register
    }

    /// One [`RpcHandlerCard`] per registered handler; applies any unregister
    /// raised by a card after the loop.
    fn handler_cards(&mut self, ui: &mut egui::Ui, ctx: &RoomContext, room: &Room) {
        let methods: Vec<String> = self.entries.keys().cloned().collect();
        let mut to_remove: Option<String> = None;
        for method in methods {
            let entry = self.entries.get(&method).unwrap().clone();
            // Scope each card's widget ids by its method so removing one doesn't
            // shift the auto-generated ids of the others (egui id-stability).
            ui.push_id(&method, |ui| {
                ui.add(RpcHandlerCard {
                    entry: &entry,
                    id: ctx.id,
                    to_remove: &mut to_remove,
                });
            });
        }

        if let Some(m) = to_remove {
            let _guard = ctx.service.runtime().enter();
            room.local_participant().unregister_rpc_method(m.clone());
            self.entries.remove(&m);
        }
    }

    fn register(&mut self, service: &LkService, room: &Room) {
        let method = self.register_method.trim().to_string();
        // The Register button is disabled for empty or already-registered topics,
        // so a duplicate here is a no-op rather than an error.
        let std::collections::btree_map::Entry::Vacant(slot) = self.entries.entry(method.clone())
        else {
            return;
        };
        let entry = Arc::new(Mutex::new(HandlerEntry {
            method: method.clone(),
            reply: String::new(),
            invocations: VecDeque::new(),
            invocation_count: 0,
        }));
        let entry_for_cb = entry.clone();
        let _guard = service.runtime().enter();
        room.local_participant()
            .register_rpc_method(method.clone(), move |data| {
                let entry_for_cb = entry_for_cb.clone();
                Box::pin(async move {
                    let reply = {
                        let mut g = entry_for_cb.lock();
                        push_invocation(&mut g, &data);
                        g.reply.clone()
                    };
                    Ok(reply)
                })
            });
        slot.insert(entry);
        self.register_method.clear();
    }
}

/// Widget: the RPC tab (send form, handler registration, invocation log) inside
/// a scroll area. Borrows the persistent [`RpcUiState`] plus the room context,
/// and composes [`RpcSendForm`] and [`RpcHandlerCard`].
pub struct RpcPanel<'a> {
    pub state: &'a mut RpcUiState,
    pub ctx: &'a RoomContext<'a>,
}

impl egui::Widget for RpcPanel<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let RpcPanel { state, ctx } = self;
        ui.scope(|ui| {
            egui::ScrollArea::vertical()
                .id_salt(ctx.id.with("rpc_scroll"))
                .show(ui, |ui| {
                    let Some(room) = ctx.room else {
                        ui.label("Not connected");
                        return;
                    };
                    egui::CollapsingHeader::new("Send RPC")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.add(RpcSendForm {
                                state: &mut state.send,
                                ctx,
                                room,
                            });
                        });
                    state.handlers.show(ui, ctx, room);
                });
        })
        .response
    }
}

fn push_invocation(entry: &mut HandlerEntry, data: &RpcInvocationData) {
    entry.invocation_count += 1;
    let payload_len = data.payload.len();
    let payload_preview = truncate_chars(&data.payload, PAYLOAD_PREVIEW_CHARS);
    entry.invocations.push_back(Invocation {
        n: entry.invocation_count,
        caller: data.caller_identity.as_str().to_string(),
        payload_len,
        received_at: SystemTime::now(),
        payload_preview,
    });
    while entry.invocations.len() > MAX_INVOCATIONS {
        entry.invocations.pop_front();
    }
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

fn preview_response(s: &str) -> String {
    let bytes = s.len();
    let mut iter = s.chars();
    let head: String = iter.by_ref().take(RESPONSE_PREVIEW_CHARS).collect();
    if iter.next().is_some() {
        format!("{}... ({}B)", head, bytes)
    } else {
        head
    }
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
