use crate::connection::ConnCtx;
use crate::{
    connection::data_track::{LocalDataTrackTile, RemoteDataTrackTile},
    connection::menu_bar::TopMenuBar,
    connection::participants::ParticipantsPanel,
    connection::rpc::{RpcPanel, RpcUiState},
    connection::sidebar::{ConnectionControls, RoomInfo, SidebarActions},
    connection::video_grid_view::VideoGridView,
    launcher::ConnectSettings,
    service::{AsyncCmd, LkService, UiCmd},
    video_renderer::VideoRenderer,
};
use livekit::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RightTab {
    Participants,
    Rpc,
}

/// State and UI of a single connection window. Connecting starts immediately
/// on creation with the settings handed over by the launcher; there is no
/// connection form here.
pub struct ConnectionWindow {
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
    rpc_ui: RpcUiState,
    right_tab: RightTab,
}

impl ConnectionWindow {
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
            rpc_ui: RpcUiState::default(),
            right_tab: RightTab::Participants,
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
                self.rpc_ui.handle_send_result(request_id, result);
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
                        self.rpc_ui.on_disconnect();
                    }
                    _ => {}
                }
            }
        }
    }

    /// Panel ids are salted via `ConnCtx::id` (derived from `window_id`): all
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
            let ctx = ConnCtx {
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
                    ui.add_space(8.0);
                    ui.monospace(&self.request.url);
                    ui.add_space(8.0);
                    ui.add(ConnectionControls {
                        ctx: &ctx,
                        connecting: self.connecting,
                        connection_failure: self.connection_failure.as_deref(),
                        actions: &mut actions,
                    });
                    if let Some(room) = ctx.room {
                        ui.add(RoomInfo { room });
                    }
                    ui.separator();
                });

            egui::Panel::right(ctx.id.with("right_panel"))
                .resizable(true)
                .size_range(20.0..=360.0)
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.right_tab,
                            RightTab::Participants,
                            "Participants",
                        );
                        ui.selectable_value(&mut self.right_tab, RightTab::Rpc, "RPC");
                    });
                    ui.separator();

                    match self.right_tab {
                        RightTab::Participants => {
                            ui.add(ParticipantsPanel { ctx: &ctx });
                        }
                        RightTab::Rpc => {
                            ui.add(RpcPanel {
                                state: &mut self.rpc_ui,
                                ctx: &ctx,
                            });
                        }
                    }
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
