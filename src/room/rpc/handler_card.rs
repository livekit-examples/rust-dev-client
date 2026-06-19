use super::{HandlerEntry, format_size, format_ts};
use crate::ui::status_badge::StatusBadge;
use egui::Color32;
use parking_lot::Mutex;
use std::sync::Arc;

/// Widget: one registered RPC handler — its reply editor and invocation log.
/// Clicking Unregister raises the method name in `to_remove`; the caller
/// performs the actual unregister so this widget needs no service/room.
pub struct RpcHandlerCard<'a> {
    pub entry: &'a Arc<Mutex<HandlerEntry>>,
    pub id: egui::Id,
    pub to_remove: &'a mut Option<String>,
}

impl egui::Widget for RpcHandlerCard<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let RpcHandlerCard {
            entry,
            id,
            to_remove,
        } = self;

        ui.add_space(6.0);
        egui::Frame::group(ui.style())
            .show(ui, |ui| {
                let mut guard = entry.lock();
                ui.horizontal(|ui| {
                    ui.monospace(egui::RichText::new(&guard.method).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Unregister").clicked() {
                            *to_remove = Some(guard.method.clone());
                        }
                    });
                });

                ui.horizontal(|ui| {
                    ui.label("Reply:");
                    if ui.small_button("Hello").clicked() {
                        guard.reply = "Hello world".to_string();
                    }
                    if ui.small_button("20k").clicked() {
                        guard.reply = "X".repeat(20_000);
                    }
                });
                let max_h = ui.text_style_height(&egui::TextStyle::Body) * 5.0 + 8.0;
                egui::ScrollArea::vertical()
                    .id_salt(id.with(("rpc_handler_reply_scroll", guard.method.as_str())))
                    .max_height(max_h)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut guard.reply)
                                .desired_rows(1)
                                .desired_width(f32::INFINITY),
                        );
                    });

                ui.label(format!("Invocations ({})", guard.invocation_count));

                if guard.invocations.is_empty() {
                    ui.add(StatusBadge::neutral("No invocations yet"));
                } else {
                    for inv in guard.invocations.iter() {
                        let meta = format!(
                            "#{} | {} | {} | {}",
                            inv.n,
                            inv.caller,
                            format_size(inv.payload_len),
                            format_ts(inv.received_at),
                        );
                        ui.add(egui::Label::new(
                            egui::RichText::new(meta).small().color(Color32::GRAY),
                        ));
                        ui.add(egui::Label::new(
                            egui::RichText::new(&inv.payload_preview).monospace(),
                        ));
                        ui.separator();
                    }
                }
            })
            .response
    }
}
