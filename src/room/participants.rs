use crate::room::RoomContext;
use crate::service::AsyncCmd;
use crate::ui::status_badge::StatusBadge;
use livekit::{e2ee::EncryptionType, prelude::*, track::VideoQuality};

/// Scrollable list of remote participants and their track publications.
pub struct ParticipantsPanel<'a> {
    pub ctx: &'a RoomContext<'a>,
}

impl egui::Widget for ParticipantsPanel<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let ctx = self.ctx;
        ui.scope(|ui| {
            let Some(room) = ctx.room else {
                ui.label("Not connected");
                return;
            };

            egui::ScrollArea::vertical()
                .id_salt(ctx.id.with("participants_scroll"))
                .show(ui, |ui| {
                    // Iterate with sorted keys to avoid flickers (immediate-mode UI).
                    let participants = room.remote_participants();
                    let mut sorted_participants = participants
                        .keys()
                        .cloned()
                        .collect::<Vec<ParticipantIdentity>>();
                    sorted_participants.sort_by(|a, b| a.as_str().cmp(b.as_str()));

                    for psid in sorted_participants {
                        let participant = participants.get(&psid).unwrap();
                        ui.add(ParticipantCard { ctx, participant });
                    }
                });
        })
        .response
    }
}

/// One participant: a collapsible section titled by its identity, with a row per
/// track publication in the body.
struct ParticipantCard<'a> {
    ctx: &'a RoomContext<'a>,
    participant: &'a RemoteParticipant,
}

impl egui::Widget for ParticipantCard<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let ParticipantCard { ctx, participant } = self;
        let identity = participant.identity().0;
        egui::CollapsingHeader::new(identity.as_str())
            .id_salt(ctx.id.with(("participant", identity.as_str())))
            .default_open(true)
            .show(ui, |ui| {
                ui.weak(format!(
                    "Client protocol: {}",
                    participant.client_protocol()
                ));

                // Sorted keys avoid flicker in immediate mode.
                let tracks = participant.track_publications();
                let mut sorted_tracks = tracks.keys().cloned().collect::<Vec<TrackSid>>();
                sorted_tracks.sort_by(|a, b| a.as_str().cmp(b.as_str()));

                for tsid in sorted_tracks {
                    let publication = tracks.get(&tsid).unwrap().clone();
                    ui.add(TrackPublicationRow { ctx, publication });
                }
            })
            .header_response
    }
}

/// One track publication: encryption, name/source, simulcast + quality menu,
/// and muted/subscribed status with subscribe/unsubscribe controls.
struct TrackPublicationRow<'a> {
    ctx: &'a RoomContext<'a>,
    publication: RemoteTrackPublication,
}

impl egui::Widget for TrackPublicationRow<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let TrackPublicationRow { ctx, publication } = self;
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Encrypted - ");
                let enc_type = publication.encryption_type();
                let enc_label = format!("{:?}", enc_type);
                if enc_type == EncryptionType::None {
                    ui.add(StatusBadge::error(enc_label));
                } else {
                    ui.add(StatusBadge::ok(enc_label));
                }
            });

            ui.label(format!(
                "{} - {:?}",
                publication.name(),
                publication.source()
            ));

            ui.horizontal(|ui| {
                ui.label("Simulcasted - ");
                let is_simulcasted = publication.simulcasted();
                ui.label(if is_simulcasted { "Yes" } else { "No" });
                if is_simulcasted {
                    ui.menu_button("Set Quality", |ui| {
                        let publication = publication.clone();
                        if ui.button("Low").clicked() {
                            let _ = ctx.service.send(AsyncCmd::SetVideoQuality {
                                publication,
                                quality: VideoQuality::Low,
                            });
                        } else if ui.button("Medium").clicked() {
                            let _ = ctx.service.send(AsyncCmd::SetVideoQuality {
                                publication,
                                quality: VideoQuality::Medium,
                            });
                        } else if ui.button("High").clicked() {
                            let _ = ctx.service.send(AsyncCmd::SetVideoQuality {
                                publication,
                                quality: VideoQuality::High,
                            });
                        }
                    });
                }
            });

            ui.horizontal(|ui| {
                if publication.is_muted() {
                    ui.add(StatusBadge::muted("Muted"));
                }

                if publication.is_subscribed() {
                    ui.add(StatusBadge::ok("Subscribed"));
                } else {
                    ui.add(StatusBadge::error("Unsubscribed"));
                }

                if publication.is_subscribed() {
                    if ui.button("Unsubscribe").clicked() {
                        let _ = ctx.service.send(AsyncCmd::UnsubscribeTrack { publication });
                    }
                } else if ui.button("Subscribe").clicked() {
                    let _ = ctx.service.send(AsyncCmd::SubscribeTrack { publication });
                }
            });
        })
        .response
    }
}
