use crate::room::RoomContext;
use crate::{
    connect::ConnectSettings,
    media::VideoRenderer,
    room::data_track::{LocalDataTrackTile, RemoteDataTrackTile},
    room::menu_bar::TopMenuBar,
    room::right_panel::{RightPanel, RightPanelState},
    room::sidebar::{Sidebar, SidebarActions},
    room::video_grid_view::VideoGridView,
    service::{AsyncCmd, LkService, UiCmd},
};
use livekit::prelude::*;
use std::collections::HashMap;

/// State and UI of a single room window. Connecting starts immediately on
/// creation with the settings handed over by the connect screen; there is no
/// connect form here.
pub struct RoomWindow {
    window_id: u64,
    runtime: tokio::runtime::Handle,
    request: ConnectSettings,
    video_renderers: HashMap<(ParticipantIdentity, TrackSid), VideoRenderer>,
    local_data_tracks: Vec<LocalDataTrackTile>,
    remote_data_tracks: Vec<RemoteDataTrackTile>,
    connecting: bool,
    connection_failure: Option<String>,
    render_state: egui_wgpu::RenderState,
    service: LkService,
    right_panel: RightPanelState,
}

impl RoomWindow {
    pub fn new(
        window_id: u64,
        runtime: tokio::runtime::Handle,
        render_state: egui_wgpu::RenderState,
        request: ConnectSettings,
    ) -> Self {
        let mut window = Self {
            service: LkService::new(&runtime),
            window_id,
            runtime,
            request,
            video_renderers: HashMap::new(),
            local_data_tracks: Vec::new(),
            remote_data_tracks: Vec::new(),
            connecting: false,
            connection_failure: None,
            render_state,
            right_panel: RightPanelState::default(),
        };
        window.connect();
        window
    }

    /// Connect (or reconnect) with the settings this window was opened with.
    fn connect(&mut self) {
        self.connecting = true;
        self.connection_failure = None;
        let _ = self.service.send(AsyncCmd::RoomConnect {
            url: self.request.url.clone(),
            token: self.request.token.clone(),
            auto_subscribe: self.request.auto_subscribe,
            enable_e2ee: self.request.enable_e2ee,
            key: self.request.key.clone(),
        });
    }

    /// Dropping [`LkService`] alone doesn't close the room, so this must be
    /// called before the window is dropped.
    pub fn disconnect(&self) {
        let _ = self.service.send(AsyncCmd::RoomDisconnect);
    }

    fn event(&mut self, event: UiCmd) {
        match event {
            UiCmd::ConnectResult { result } => {
                self.connecting = false;
                if let Err(err) = result {
                    self.connection_failure = Some(err.to_string());
                }
            }
            UiCmd::DataTrackPublished { track } => {
                self.local_data_tracks.push(LocalDataTrackTile::new(track));
            }
            UiCmd::DataTrackUnpublished => {
                self.local_data_tracks.clear();
            }
            UiCmd::RpcSendResult { request_id, result } => {
                self.right_panel.rpc.handle_send_result(request_id, result);
            }
            UiCmd::RoomEvent { event } => {
                log::info!("{:?}", event);
                match event {
                    RoomEvent::TrackSubscribed {
                        track,
                        publication: _,
                        participant,
                    } => {
                        if let RemoteTrack::Video(ref video_track) = track {
                            let video_renderer = VideoRenderer::new(
                                &self.runtime,
                                self.render_state.clone(),
                                video_track.rtc_track(),
                            );
                            self.video_renderers
                                .insert((participant.identity(), track.sid()), video_renderer);
                        }
                    }
                    RoomEvent::TrackUnsubscribed {
                        track,
                        publication: _,
                        participant,
                    } => {
                        self.video_renderers
                            .remove(&(participant.identity(), track.sid()));
                    }
                    RoomEvent::LocalTrackPublished {
                        track,
                        publication: _,
                        participant,
                    } => {
                        if let LocalTrack::Video(ref video_track) = track {
                            let video_renderer = VideoRenderer::new(
                                &self.runtime,
                                self.render_state.clone(),
                                video_track.rtc_track(),
                            );
                            self.video_renderers
                                .insert((participant.identity(), track.sid()), video_renderer);
                        }
                    }
                    RoomEvent::LocalTrackUnpublished {
                        publication,
                        participant,
                    } => {
                        self.video_renderers
                            .remove(&(participant.identity(), publication.sid()));
                    }
                    RoomEvent::DataTrackPublished(track) => {
                        self.remote_data_tracks
                            .push(RemoteDataTrackTile::new(&self.runtime, track));
                    }
                    RoomEvent::Disconnected { reason: _ } => {
                        self.video_renderers.clear();
                        self.local_data_tracks.clear();
                        self.remote_data_tracks.clear();
                        self.right_panel.rpc.on_disconnect();
                    }
                    _ => {}
                }
            }
        }
    }

    /// Panel ids are salted via `RoomCtx::id` (derived from `window_id`): all
    /// viewports share a single `egui::Context`, so unsalted ids would share
    /// state across windows.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if let Some(event) = self.service.try_recv() {
            self.event(event);
        }

        let mut actions = SidebarActions::default();

        // Scope `ctx` (which borrows `&self.service`) so it is released before
        // we apply sidebar actions that need `&mut self`.
        {
            let room = self.service.room();
            let ctx = RoomContext {
                service: &self.service,
                room: room.as_deref(),
                id: egui::Id::new(self.window_id),
            };

            egui::Panel::top(ctx.id.with("top_panel")).show_inside(ui, |ui| {
                ui.add(TopMenuBar { ctx: &ctx });
            });

            egui::Panel::left(ctx.id.with("left_panel"))
                .resizable(true)
                .size_range(20.0..=360.0)
                .show_inside(ui, |ui| {
                    ui.add(Sidebar {
                        ctx: &ctx,
                        url: &self.request.url,
                        connecting: self.connecting,
                        connection_failure: self.connection_failure.as_deref(),
                        actions: &mut actions,
                    });
                });

            egui::Panel::right(ctx.id.with("right_panel"))
                .resizable(true)
                .size_range(20.0..=360.0)
                .show_inside(ui, |ui| {
                    ui.add(RightPanel {
                        ctx: &ctx,
                        state: &mut self.right_panel,
                    });
                });

            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.add(VideoGridView {
                    ctx: &ctx,
                    video_renderers: &self.video_renderers,
                    local_data_tracks: &mut self.local_data_tracks,
                    remote_data_tracks: &self.remote_data_tracks,
                });
            });
        }

        if actions.disconnect {
            self.disconnect();
        } else if actions.reconnect {
            self.connect();
        }

        ui.ctx().request_repaint();
    }
}
