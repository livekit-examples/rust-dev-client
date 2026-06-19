use super::{SendResult, SendState, preview_response};
use crate::connection::ConnCtx;
use crate::service::AsyncCmd;
use crate::ui::{labeled_field::LabeledTextEdit, status_badge::StatusBadge};
use livekit::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SEND_ID: AtomicU64 = AtomicU64::new(1);

/// Widget: the "Send RPC" section — destination combo, method, payload editor,
/// the Send button, and the in-flight / result status.
pub struct RpcSendForm<'a> {
    pub state: &'a mut SendState,
    pub ctx: &'a ConnCtx<'a>,
    pub room: &'a Room,
}

impl egui::Widget for RpcSendForm<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let RpcSendForm { state, ctx, room } = self;
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Send RPC").strong());

            let participants = room.remote_participants();
            let mut idents: Vec<ParticipantIdentity> = participants.keys().cloned().collect();
            idents.sort_by(|a, b| a.as_str().cmp(b.as_str()));

            // Drop a stale selection if that participant has left.
            if let Some(sel) = state.destination.as_ref()
                && !participants.contains_key(sel)
            {
                state.destination = None;
            }

            ui.horizontal(|ui| {
                ui.label("To:");
                let combo_label = state
                    .destination
                    .as_ref()
                    .map(|i| i.as_str().to_string())
                    .unwrap_or_else(|| {
                        if idents.is_empty() {
                            "(no remote participants)".to_string()
                        } else {
                            "(select)".to_string()
                        }
                    });
                egui::ComboBox::from_id_salt(ctx.id.with("rpc_dest_combo"))
                    .selected_text(combo_label)
                    .show_ui(ui, |ui| {
                        for ident in &idents {
                            ui.selectable_value(
                                &mut state.destination,
                                Some(ident.clone()),
                                ident.as_str(),
                            );
                        }
                    });
            });

            ui.add(
                LabeledTextEdit::singleline("Method:", &mut state.method)
                    .horizontal()
                    .desired_width(f32::INFINITY),
            );

            ui.horizontal(|ui| {
                ui.label("Payload:");
                if ui.small_button("Hello").clicked() {
                    state.payload = "Hello world".to_string();
                }
                if ui.small_button("20k").clicked() {
                    state.payload = "X".repeat(20_000);
                }
            });
            let max_h = ui.text_style_height(&egui::TextStyle::Body) * 5.0 + 8.0;
            egui::ScrollArea::vertical()
                .id_salt(ctx.id.with("rpc_send_payload_scroll"))
                .max_height(max_h)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut state.payload)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                });

            let can_send = state.in_flight.is_none()
                && state.destination.is_some()
                && !state.method.trim().is_empty();

            ui.horizontal(|ui| {
                ui.add_enabled_ui(can_send, |ui| {
                    if ui.button("Send").clicked() {
                        let dest = state.destination.as_ref().unwrap().0.clone();
                        let method = state.method.clone();
                        let payload = state.payload.clone();
                        let request_id = NEXT_SEND_ID.fetch_add(1, Ordering::Relaxed);
                        state.in_flight = Some(request_id);
                        state.result = None;
                        let _ = ctx.service.send(AsyncCmd::RpcSendRequest {
                            destination: dest,
                            method,
                            payload,
                            request_id,
                        });
                    }
                });
                if state.in_flight.is_some() {
                    ui.spinner();
                }
            });

            if state.in_flight.is_some() {
                let dest = state
                    .destination
                    .as_ref()
                    .map(|i| i.as_str().to_string())
                    .unwrap_or_default();
                ui.add(StatusBadge::neutral(format!(
                    "Sending to {} {}...",
                    dest, state.method
                )));
            } else {
                match &state.result {
                    Some(SendResult::Ok(s)) => {
                        ui.add(StatusBadge::ok(format!("OK: {}", preview_response(s))));
                    }
                    Some(SendResult::Err { code, message }) => {
                        ui.add(StatusBadge::error(format!("Error {}: {}", code, message)));
                    }
                    None => {}
                }
            }
        })
        .response
    }
}
