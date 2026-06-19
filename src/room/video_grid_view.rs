use crate::room::RoomContext;
use crate::room::data_track::{
    LocalDataTrackTile, LocalDataTrackWidget, RemoteDataTrackTile, RemoteDataTrackWidget,
};
use crate::ui::{video_grid::VideoGrid, video_tile::VideoTile};
use crate::video_renderer::VideoRenderer;
use livekit::prelude::*;
use std::collections::HashMap;

/// Central grid of all track tiles (video + data). When disconnected it shows a
/// row of placeholder frames.
pub struct VideoGridView<'a> {
    pub ctx: &'a RoomContext<'a>,
    pub video_renderers: &'a HashMap<(ParticipantIdentity, TrackSid), VideoRenderer>,
    pub local_data_tracks: &'a mut [LocalDataTrackTile],
    pub remote_data_tracks: &'a [RemoteDataTrackTile],
}

impl egui::Widget for VideoGridView<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let VideoGridView {
            ctx,
            video_renderers,
            local_data_tracks,
            remote_data_tracks,
        } = self;

        let connected = ctx.room.is_some();
        let has_tiles = !video_renderers.is_empty()
            || !local_data_tracks.is_empty()
            || !remote_data_tracks.is_empty();

        ui.scope(|ui| {
            if connected && !has_tiles {
                ui.centered_and_justified(|ui| {
                    ui.label("No tracks subscribed");
                });
                return;
            }

            egui::ScrollArea::vertical()
                .id_salt(ctx.id.with("central_scroll"))
                .show(ui, |ui| {
                    VideoGrid::new(ctx.id.with("default_grid"))
                        .max_columns(6)
                        .show(ui, |ui| {
                            if let Some(room) = ctx.room {
                                for ((participant_id, _), video_renderer) in video_renderers {
                                    ui.video_frame(|ui| {
                                        if let Some(p) =
                                            room.remote_participants().get(participant_id)
                                        {
                                            let name = p.name();
                                            ui.add(VideoTile::new(
                                                video_renderer.texture_id(),
                                                video_renderer.resolution(),
                                                name.as_str(),
                                                p.is_speaking(),
                                            ));
                                        } else {
                                            let lp = room.local_participant();
                                            let name = lp.name();
                                            ui.add(VideoTile::new(
                                                video_renderer.texture_id(),
                                                video_renderer.resolution(),
                                                name.as_str(),
                                                lp.is_speaking(),
                                            ));
                                        }
                                    });
                                }

                                for tile in &mut *local_data_tracks {
                                    ui.video_frame(|ui| {
                                        ui.add(LocalDataTrackWidget::new(tile));
                                    });
                                }

                                for tile in remote_data_tracks {
                                    ui.video_frame(|ui| {
                                        ui.add(RemoteDataTrackWidget::new(tile));
                                    });
                                }
                            } else {
                                for _ in 0..5 {
                                    ui.video_frame(|ui| {
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
