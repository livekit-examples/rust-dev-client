use crate::media::MicTrack;
use crate::room::RoomContext;
use crate::service::AsyncCmd;
use crate::style::Palette;
use crate::ui::status_badge::StatusBadge;
use livekit::prelude::*;

/// Fixed width for the action button so it doesn't resize as its label changes
/// between Disconnect / Reconnect / Connecting.
const BUTTON_WIDTH: f32 = 100.0;

/// Connect/disconnect intents raised by the status bar. It only holds
/// `&RoomContext`, so it records the intent here and the window applies it
/// (with `&mut self`) after the borrow ends.
#[derive(Default)]
pub struct StatusBarActions {
    pub reconnect: bool,
    pub disconnect: bool,
}

/// Bottom status bar: the current connection status on the left and a
/// connect/disconnect button floated to the right.
pub struct StatusBar<'a> {
    pub ctx: &'a RoomContext<'a>,
    pub connecting: bool,
    pub connection_failure: Option<&'a str>,
    pub actions: &'a mut StatusBarActions,
}

impl egui::Widget for StatusBar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let StatusBar {
            ctx,
            connecting,
            connection_failure,
            actions,
        } = self;

        let room = ctx
            .room
            .filter(|r| r.connection_state() == ConnectionState::Connected);

        let mic_active = room.is_some_and(|r| {
            r.local_participant()
                .track_publications()
                .values()
                .any(|p| p.name().as_str() == MicTrack::TRACK_NAME)
        });

        ui.horizontal(|ui| {
            if let Some(err) = connection_failure {
                ui.add(StatusBadge::error(format!("Connection failed: {err}")));
            } else if connecting {
                ui.label("Connecting...");
            } else if let Some(room) = room {
                ui.label(format!(
                    "Connected to '{}' as '{}'",
                    room.name(),
                    room.local_participant().identity()
                ));
            } else {
                ui.label("Disconnected");
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let size = egui::vec2(BUTTON_WIDTH, 0.0);
                if connecting {
                    ui.add_enabled(false, egui::Button::new("Connecting...").min_size(size));
                } else if room.is_some() {
                    if ui
                        .add(egui::Button::new("Disconnect").min_size(size))
                        .clicked()
                    {
                        actions.disconnect = true;
                    }
                    let palette = Palette::for_theme(ui.theme());
                    let mic_label = if mic_active {
                        egui::RichText::new("Microphone").color(palette.bg_1)
                    } else {
                        egui::RichText::new("Microphone")
                    };
                    let mut mic_button = egui::Button::new(mic_label).min_size(size);
                    if mic_active {
                        mic_button = mic_button.fill(palette.fg_success);
                    }
                    let mic_tooltip = if mic_active {
                        "Microphone is publishing — click to stop (platform audio / ADM)"
                    } else {
                        "Publish microphone audio via platform audio (ADM)"
                    };
                    if ui.add(mic_button).on_hover_text(mic_tooltip).clicked() {
                        let _ = ctx.service.send(AsyncCmd::ToggleMic);
                    }
                } else if ui
                    .add(egui::Button::new("Reconnect").min_size(size))
                    .clicked()
                {
                    actions.reconnect = true;
                }
            });
        })
        .response
    }
}
