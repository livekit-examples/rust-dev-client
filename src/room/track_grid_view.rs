use crate::media::VideoRenderer;
use crate::room::RoomContext;
use crate::room::data_track::{
    LocalDataTrackTile, LocalDataTrackWidget, RemoteDataTrackTile, RemoteDataTrackWidget,
};
use crate::ui::placeholder_tile::{PlaceholderTile, placeholder_texture};
use crate::ui::{track_grid::TrackGrid, video_tile::VideoTile};
use livekit::prelude::*;
use std::collections::HashMap;

pub struct TrackGridView<'a> {
    pub ctx: &'a RoomContext<'a>,
    pub video_renderers: &'a HashMap<(ParticipantIdentity, TrackSid), VideoRenderer>,
    pub local_data_tracks: &'a mut [LocalDataTrackTile],
    pub remote_data_tracks: &'a [RemoteDataTrackTile],
}

impl egui::Widget for TrackGridView<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let TrackGridView {
            ctx,
            video_renderers,
            local_data_tracks,
            remote_data_tracks,
        } = self;

        ui.scope(|ui| {
            egui::ScrollArea::vertical()
                .id_salt(ctx.id.with("central_scroll"))
                .show(ui, |ui| {
                    // `TrackGrid::show` hands the closure a `TrackGridContext`, not
                    // an `egui::Ui`, so capture the egui context here (cheap Arc
                    // clone) to resolve the placeholder texture lazily below.
                    let egui_ctx = ui.ctx().clone();
                    TrackGrid::new(ctx.id.with("default_grid"))
                        .max_columns(6)
                        .show(ui, |ui| {
                            if let Some(room) = ctx.room {
                                let placeholder = placeholder_texture(&egui_ctx);

                                // Local participant first, then remotes sorted by
                                // identity for a stable order in immediate mode.
                                let mut participants =
                                    vec![Participant::Local(room.local_participant())];
                                let remotes = room.remote_participants();
                                let mut ids = remotes
                                    .keys()
                                    .cloned()
                                    .collect::<Vec<ParticipantIdentity>>();
                                ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
                                participants.extend(
                                    ids.into_iter()
                                        .map(|id| Participant::Remote(remotes[&id].clone())),
                                );

                                for participant in &participants {
                                    let identity = participant.identity();
                                    let speaking = participant.is_speaking();

                                    // Video tracks currently sending live frames:
                                    // not muted and backed by a renderer with a frame.
                                    let publications = participant.track_publications();
                                    let mut video_sids = publications
                                        .iter()
                                        .filter(|(_, p)| {
                                            p.kind() == TrackKind::Video && !p.is_muted()
                                        })
                                        .map(|(sid, _)| sid.clone())
                                        .collect::<Vec<TrackSid>>();
                                    video_sids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

                                    let mut rendered_video = false;
                                    for sid in video_sids {
                                        let Some(renderer) =
                                            video_renderers.get(&(identity.clone(), sid))
                                        else {
                                            continue;
                                        };
                                        if renderer.texture_id().is_none() {
                                            continue; // no frame decoded yet
                                        }
                                        ui.track_frame(|ui| {
                                            ui.add(VideoTile::new(
                                                renderer.texture_id(),
                                                identity.as_str(),
                                                speaking,
                                            ));
                                        });
                                        rendered_video = true;
                                    }

                                    if !rendered_video {
                                        ui.track_frame(|ui| {
                                            ui.add(PlaceholderTile::new(
                                                placeholder.id(),
                                                identity.as_str(),
                                                speaking,
                                            ));
                                        });
                                    }
                                }

                                for tile in &mut *local_data_tracks {
                                    ui.track_frame(|ui| {
                                        ui.add(LocalDataTrackWidget::new(tile));
                                    });
                                }

                                for tile in remote_data_tracks {
                                    ui.track_frame(|ui| {
                                        ui.add(RemoteDataTrackWidget::new(tile));
                                    });
                                }
                            } else {
                                for _ in 0..5 {
                                    ui.track_frame(|ui| {
                                        egui::Frame::new()
                                            .fill(ui.style().visuals.code_bg_color)
                                            .show(ui, |ui| {
                                                ui.allocate_space(ui.available_size());
                                            });
                                    });
                                }
                            }
                        });
                });
        })
        .response
    }
}
