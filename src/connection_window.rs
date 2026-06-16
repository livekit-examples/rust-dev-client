use crate::{
    data_track::{
        LocalDataTrackTile, LocalDataTrackWidget, RemoteDataTrackTile, RemoteDataTrackWidget,
    },
    launcher::ConnectSettings,
    rpc_ui::{RpcPanel, RpcUiState},
    service::{AsyncCmd, LkService, UiCmd},
    video_grid::VideoGrid,
    video_renderer::{VideoRenderer, VideoTile},
};
use livekit::{SimulateScenario, e2ee::EncryptionType, prelude::*, track::VideoQuality};
use std::collections::HashMap;

/// Shared, per-frame context passed to a connection window's widgets: the
/// service, the optional connected room, and a window-scoped id used to salt
/// widget ids (all viewports share one `egui::Context`, so ids must be unique
/// per window). Widgets derive child ids via `ctx.id.with("name")`.
pub struct ConnCtx<'a> {
    pub service: &'a LkService,
    pub room: Option<&'a Room>,
    pub id: egui::Id,
}

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
                    ui.add(ConnectionSidebar {
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

/// Top menu bar: Simulate / Publish / Debug actions, all sent to the service.
struct TopMenuBar<'a> {
    ctx: &'a ConnCtx<'a>,
}

impl egui::Widget for TopMenuBar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let service = self.ctx.service;
        egui::MenuBar::new()
            .ui(ui, |ui| {
                ui.menu_button("Simulate", |ui| {
                    let scenarios = [
                        SimulateScenario::SignalReconnect,
                        SimulateScenario::Speaker,
                        SimulateScenario::NodeFailure,
                        SimulateScenario::ServerLeave,
                        SimulateScenario::Migration,
                        SimulateScenario::ForceTcp,
                        SimulateScenario::ForceTls,
                    ];

                    for scenario in scenarios {
                        if ui.button(format!("{:?}", scenario)).clicked() {
                            let _ = service.send(AsyncCmd::SimulateScenario { scenario });
                        }
                    }
                });

                ui.menu_button("Publish", |ui| {
                    if ui.button("Logo").clicked() {
                        let _ = service.send(AsyncCmd::ToggleLogo);
                    }
                    if ui.button("SineWave").clicked() {
                        let _ = service.send(AsyncCmd::ToggleSine);
                    }
                    if ui.button("DataTrack").clicked() {
                        let _ = service.send(AsyncCmd::ToggleDataTrack);
                    }
                });

                ui.menu_button("Debug", |ui| {
                    if ui.button("Log stats").clicked() {
                        let _ = service.send(AsyncCmd::LogStats);
                    }
                });
            })
            .response
    }
}

/// Window-level intents raised by [`ConnectionSidebar`] buttons, applied by the
/// window after rendering (so the widget can borrow `&ctx` while the window
/// keeps owning `connect`/`disconnect`).
#[derive(Default)]
struct SidebarActions {
    reconnect: bool,
    disconnect: bool,
}

/// Connection status and room info (the connection form lives in the launcher).
struct ConnectionSidebar<'a> {
    ctx: &'a ConnCtx<'a>,
    url: &'a str,
    connecting: bool,
    connection_failure: Option<&'a str>,
    actions: &'a mut SidebarActions,
}

impl egui::Widget for ConnectionSidebar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let ConnectionSidebar {
            ctx,
            url,
            connecting,
            connection_failure,
            actions,
        } = self;

        let connected = ctx
            .room
            .is_some_and(|r| r.connection_state() == ConnectionState::Connected);

        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.monospace(url);
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if connecting {
                    ui.spinner();
                    ui.label("Connecting...");
                } else if connected {
                    if ui.button("Disconnect").clicked() {
                        actions.disconnect = true;
                    }
                } else if ui.button("Reconnect").clicked() {
                    actions.reconnect = true;
                }
            });

            if ui.button("E2eeKeyRatchet").clicked() {
                let _ = ctx.service.send(AsyncCmd::E2eeKeyRatchet);
            }

            if let Some(err) = connection_failure {
                ui.colored_label(egui::Color32::RED, err);
            }

            if let Some(room) = ctx.room {
                ui.label(format!("Name: {}", room.name()));
                ui.label(format!("ConnectionState: {:?}", room.connection_state()));
                ui.label(format!(
                    "ParticipantCount: {:?}",
                    room.remote_participants().len() + 1
                ));
            }

            ui.separator();
        })
        .response
    }
}

/// Remote participants and their track publications.
struct ParticipantsPanel<'a> {
    ctx: &'a ConnCtx<'a>,
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
                        let tracks = participant.track_publications();
                        let mut sorted_tracks = tracks.keys().cloned().collect::<Vec<TrackSid>>();
                        sorted_tracks.sort_by(|a, b| a.as_str().cmp(b.as_str()));

                        ui.monospace(&participant.identity().0);
                        for tsid in sorted_tracks {
                            let publication = tracks.get(&tsid).unwrap().clone();

                            ui.horizontal(|ui| {
                                ui.label("Encrypted - ");
                                let enc_type = publication.encryption_type();
                                if enc_type == EncryptionType::None {
                                    ui.colored_label(egui::Color32::RED, format!("{:?}", enc_type));
                                } else {
                                    ui.colored_label(
                                        egui::Color32::GREEN,
                                        format!("{:?}", enc_type),
                                    );
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
                                    ui.colored_label(egui::Color32::DARK_GRAY, "Muted");
                                }

                                if publication.is_subscribed() {
                                    ui.colored_label(egui::Color32::GREEN, "Subscribed");
                                } else {
                                    ui.colored_label(egui::Color32::RED, "Unsubscribed");
                                }

                                if publication.is_subscribed() {
                                    if ui.button("Unsubscribe").clicked() {
                                        let _ = ctx
                                            .service
                                            .send(AsyncCmd::UnsubscribeTrack { publication });
                                    }
                                } else if ui.button("Subscribe").clicked() {
                                    let _ =
                                        ctx.service.send(AsyncCmd::SubscribeTrack { publication });
                                }
                            });
                        }
                        ui.separator();
                    }
                });
        })
        .response
    }
}

/// Central grid of all track tiles (video + data).
struct VideoGridView<'a> {
    ctx: &'a ConnCtx<'a>,
    video_renderers: &'a HashMap<(ParticipantIdentity, TrackSid), VideoRenderer>,
    local_data_tracks: &'a mut [LocalDataTrackTile],
    remote_data_tracks: &'a [RemoteDataTrackTile],
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
                                                video_renderer,
                                                name.as_str(),
                                                p.is_speaking(),
                                            ));
                                        } else {
                                            let lp = room.local_participant();
                                            let name = lp.name();
                                            ui.add(VideoTile::new(
                                                video_renderer,
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
