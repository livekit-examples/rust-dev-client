use crate::media::{LogoTrack, SineParameters, SineTrack};
use livekit::{
    SimulateScenario, StreamByteOptions, StreamTextOptions,
    e2ee::{E2eeOptions, EncryptionType, key_provider::*},
    prelude::*,
    track::VideoQuality,
};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::SendError};

#[derive(Debug)]
pub enum AsyncCmd {
    RoomConnect {
        url: String,
        token: String,
        auto_subscribe: bool,
        dynacast: bool,
        enable_e2ee: bool,
        key: String,
    },
    RoomDisconnect,
    SimulateScenario {
        scenario: SimulateScenario,
    },
    ToggleLogo,
    ToggleSine,
    ToggleDataTrack,
    SubscribeTrack {
        publication: RemoteTrackPublication,
    },
    UnsubscribeTrack {
        publication: RemoteTrackPublication,
    },
    SetVideoQuality {
        publication: RemoteTrackPublication,
        quality: VideoQuality,
    },
    E2eeKeyRatchet,
    LogStats,
    RpcSendRequest {
        destination: String,
        method: String,
        payload: String,
        request_id: u64,
    },
    DataStreamSend {
        request_id: u64,
        topic: String,
        destination: Option<ParticipantIdentity>,
        payload: DataStreamPayload,
    },
}

/// The body of an outgoing data stream send: already-decoded so the async side
/// has no parsing to do (hex is parsed UI-side before dispatch).
#[derive(Debug)]
pub enum DataStreamPayload {
    Text(String),
    Bytes(Vec<u8>),
}

#[derive(Debug)]
pub enum UiCmd {
    ConnectResult {
        result: RoomResult<()>,
    },
    RoomEvent {
        event: RoomEvent,
    },
    DataTrackPublished {
        track: LocalDataTrack,
    },
    DataTrackUnpublished,
    RpcSendResult {
        request_id: u64,
        result: Result<String, RpcError>,
    },
    DataStreamSendResult {
        request_id: u64,
        /// `Ok` carries the new stream's id; `Err` a human-readable message.
        result: Result<String, String>,
    },
}

/// AppService is the "asynchronous" part of our application, where we connect to a room and
/// handle events.
pub struct LkService {
    cmd_tx: mpsc::UnboundedSender<AsyncCmd>,
    ui_rx: mpsc::UnboundedReceiver<UiCmd>,
    handle: tokio::task::JoinHandle<()>,
    inner: Arc<ServiceInner>,
    runtime: tokio::runtime::Handle,
}

struct ServiceInner {
    ui_tx: mpsc::UnboundedSender<UiCmd>,
    room: Mutex<Option<Arc<Room>>>,
}

impl LkService {
    /// Create a new AppService and return a channel that informs the UI of events.
    pub fn new(async_handle: &tokio::runtime::Handle) -> Self {
        let (ui_tx, ui_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let inner = Arc::new(ServiceInner {
            ui_tx,
            room: Default::default(),
        });
        let handle = async_handle.spawn(service_task(inner.clone(), cmd_rx));

        Self {
            cmd_tx,
            ui_rx,
            handle,
            inner,
            runtime: async_handle.clone(),
        }
    }

    pub fn room(&self) -> Option<Arc<Room>> {
        self.inner.room.lock().clone()
    }

    pub fn runtime(&self) -> &tokio::runtime::Handle {
        &self.runtime
    }

    pub fn send(&self, cmd: AsyncCmd) -> Result<(), SendError<AsyncCmd>> {
        self.cmd_tx.send(cmd)
    }

    pub fn try_recv(&mut self) -> Option<UiCmd> {
        self.ui_rx.try_recv().ok()
    }

    #[allow(dead_code)]
    pub async fn close(self) {
        drop(self.cmd_tx);
        let _ = self.handle.await;
    }
}

async fn service_task(inner: Arc<ServiceInner>, mut cmd_rx: mpsc::UnboundedReceiver<AsyncCmd>) {
    struct RunningState {
        room: Arc<Room>,
        logo_track: LogoTrack,
        sine_track: SineTrack,
        data_track: Option<LocalDataTrack>,
    }

    let mut running_state = None;

    while let Some(event) = cmd_rx.recv().await {
        match event {
            AsyncCmd::RoomConnect {
                url,
                token,
                auto_subscribe,
                dynacast,
                enable_e2ee,
                key,
            } => {
                log::info!("connecting to room: {}", url);

                let key_provider =
                    KeyProvider::with_shared_key(KeyProviderOptions::default(), key.into_bytes());
                let e2ee = enable_e2ee.then_some(E2eeOptions {
                    encryption_type: EncryptionType::Gcm,
                    key_provider,
                });

                let mut options = RoomOptions::default();
                options.auto_subscribe = auto_subscribe;
                options.dynacast = dynacast;
                options.encryption = e2ee;

                let res = Room::connect(&url, &token, options).await;

                if let Ok((new_room, events)) = res {
                    log::info!("connected to room: {}", new_room.name());
                    tokio::spawn(room_task(inner.clone(), events));

                    let new_room = Arc::new(new_room);
                    running_state = Some(RunningState {
                        room: new_room.clone(),
                        logo_track: LogoTrack::new(new_room.clone()),
                        sine_track: SineTrack::new(new_room.clone(), SineParameters::default()),
                        data_track: None,
                    });

                    // Allow direct access to the room from the UI (Used for sync access)
                    inner.room.lock().replace(new_room);

                    let _ = inner.ui_tx.send(UiCmd::ConnectResult { result: Ok(()) });
                } else if let Err(err) = res {
                    log::error!("failed to connect to room: {:?}", err);
                    let _ = inner.ui_tx.send(UiCmd::ConnectResult { result: Err(err) });
                }
            }
            AsyncCmd::RoomDisconnect => {
                if let Some(state) = running_state.take() {
                    *inner.room.lock() = None;
                    if let Err(err) = state.room.close().await {
                        log::error!("failed to disconnect from room: {:?}", err);
                    }
                }
            }
            AsyncCmd::SimulateScenario { scenario } => {
                if let Some(state) = running_state.as_ref()
                    && let Err(err) = state.room.simulate_scenario(scenario).await
                {
                    log::error!("failed to simulate scenario: {:?}", err);
                }
            }
            AsyncCmd::ToggleLogo => {
                if let Some(state) = running_state.as_mut() {
                    if state.logo_track.is_published() {
                        state.logo_track.unpublish().await.unwrap();
                    } else {
                        state.logo_track.publish().await.unwrap();
                    }
                }
            }
            AsyncCmd::ToggleSine => {
                if let Some(state) = running_state.as_mut() {
                    if state.sine_track.is_published() {
                        state.sine_track.unpublish().await.unwrap();
                    } else {
                        state.sine_track.publish().await.unwrap();
                    }
                }
            }
            AsyncCmd::ToggleDataTrack => {
                if let Some(state) = running_state.as_mut() {
                    if let Some(track) = state.data_track.take() {
                        track.unpublish();
                        let _ = inner.ui_tx.send(UiCmd::DataTrackUnpublished);
                    } else {
                        match state
                            .room
                            .local_participant()
                            .publish_data_track("slider")
                            .await
                        {
                            Ok(track) => {
                                let _ = inner.ui_tx.send(UiCmd::DataTrackPublished {
                                    track: track.clone(),
                                });
                                state.data_track = Some(track);
                            }
                            Err(err) => log::error!("failed to publish data track: {err}"),
                        }
                    }
                }
            }
            AsyncCmd::SubscribeTrack { publication } => {
                publication.set_subscribed(true);
            }
            AsyncCmd::UnsubscribeTrack { publication } => {
                publication.set_subscribed(false);
            }
            AsyncCmd::SetVideoQuality {
                publication,
                quality,
            } => {
                publication.set_video_quality(quality);
            }
            AsyncCmd::E2eeKeyRatchet => {
                if let Some(state) = running_state.as_ref() {
                    let e2ee_manager = state.room.e2ee_manager();
                    if let Some(key_provider) = e2ee_manager.key_provider() {
                        key_provider.ratchet_shared_key(0);
                    }
                }
            }
            AsyncCmd::RpcSendRequest {
                destination,
                method,
                payload,
                request_id,
            } => {
                if let Some(state) = running_state.as_ref() {
                    let local = state.room.local_participant();
                    let ui_tx = inner.ui_tx.clone();
                    tokio::spawn(async move {
                        let result = local
                            .perform_rpc(
                                PerformRpcData::new(destination, method).with_payload(payload),
                            )
                            .await;
                        let _ = ui_tx.send(UiCmd::RpcSendResult { request_id, result });
                    });
                } else {
                    let _ = inner.ui_tx.send(UiCmd::RpcSendResult {
                        request_id,
                        result: Err(RpcError {
                            code: RpcErrorCode::SendFailed as u32,
                            message: "Not connected".to_string(),
                            data: None,
                        }),
                    });
                }
            }
            AsyncCmd::DataStreamSend {
                request_id,
                topic,
                destination,
                payload,
            } => {
                if let Some(state) = running_state.as_ref() {
                    let local = state.room.local_participant();
                    let ui_tx = inner.ui_tx.clone();
                    let destination_identities = destination.map(|i| vec![i]).unwrap_or_default();
                    tokio::spawn(async move {
                        let result = match payload {
                            DataStreamPayload::Text(text) => {
                                let options = StreamTextOptions::new_with_topic(topic)
                                    .with_destination_identities(destination_identities);
                                local.send_text(&text, options).await.map(|info| info.id)
                            }
                            DataStreamPayload::Bytes(bytes) => {
                                let options = StreamByteOptions::new_with_topic(topic)
                                    .with_destination_identities(destination_identities);
                                local.send_bytes(bytes, options).await.map(|info| info.id)
                            }
                        };
                        let _ = ui_tx.send(UiCmd::DataStreamSendResult {
                            request_id,
                            result: result.map_err(|e| e.to_string()),
                        });
                    });
                } else {
                    let _ = inner.ui_tx.send(UiCmd::DataStreamSendResult {
                        request_id,
                        result: Err("Not connected".to_string()),
                    });
                }
            }
            AsyncCmd::LogStats => {
                if let Some(state) = running_state.as_ref() {
                    for (_, publication) in state.room.local_participant().track_publications() {
                        if let Some(track) = publication.track() {
                            log::info!(
                                "track stats: LOCAL {:?} {:?}",
                                track.sid(),
                                track.get_stats().await,
                            );
                        }
                    }

                    for (_, participant) in state.room.remote_participants() {
                        for (_, publication) in participant.track_publications() {
                            if let Some(track) = publication.track() {
                                log::info!(
                                    "track stats: {:?} {:?} {:?}",
                                    participant.identity(),
                                    track.sid(),
                                    track.get_stats().await,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Task basically used to forward room events to the UI.
/// It will automatically close when the room is disconnected.
async fn room_task(inner: Arc<ServiceInner>, mut events: mpsc::UnboundedReceiver<RoomEvent>) {
    while let Some(event) = events.recv().await {
        let _ = inner.ui_tx.send(UiCmd::RoomEvent { event });
    }
}
