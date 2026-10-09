use crate::connect::Auth;
use crate::media::{
    CaptureSource, CaptureTrack, MicTrack, OnUnpublished, SineParameters, SineTrack,
};
use livekit::{
    SimulateScenario, StreamByteOptions, StreamTextOptions,
    e2ee::{E2eeOptions, EncryptionType, key_provider::*},
    prelude::*,
    track::VideoQuality,
};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::SendError};

#[derive(Debug)]
pub enum AsyncCmd {
    RoomConnect {
        /// Boxed to keep `AsyncCmd` small (clippy: `result_large_err` on
        /// [`LkService::send`]); the token-source options make `Auth` large.
        auth: Box<Auth>,
        auto_subscribe: bool,
        dynacast: bool,
        enable_e2ee: bool,
        key: String,
    },
    RoomDisconnect,
    SimulateScenario {
        scenario: SimulateScenario,
    },
    TogglePublish {
        source: LocalSource,
    },
    ToggleMic,
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

/// A local source the user can publish from the Publish menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalSource {
    Capture(CaptureSource),
    Sine,
    DataTrack,
}

impl LocalSource {
    pub const ALL: [Self; 5] = [
        Self::Capture(CaptureSource::Gradient),
        Self::Capture(CaptureSource::Logo),
        Self::Capture(CaptureSource::Clock),
        Self::Sine,
        Self::DataTrack,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Capture(source) => source.label(),
            Self::Sine => "Audio: Sine Wave",
            Self::DataTrack => "Data Track: Slider",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Capture(source) => source.description(),
            Self::Sine => "Synthetic 440 Hz sine wave tone",
            Self::DataTrack => "Integer from 0 to 512 set with a slider, sent as text",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PublishState {
    #[default]
    Unpublished,
    /// A publish or unpublish is in flight.
    Pending,
    Published,
}

#[derive(Debug)]
pub enum UiCmd {
    ConnectResult {
        /// `Err` is a human-readable message: token resolution and room
        /// connection can each fail, with different error types.
        result: Result<(), String>,
    },
    RoomEvent {
        event: RoomEvent,
    },
    DataTrackPublished {
        track: LocalDataTrack,
    },
    DataTrackUnpublished,
    PublishState {
        source: LocalSource,
        state: PublishState,
    },
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
        capture_tracks: HashMap<CaptureSource, CaptureTrack>,
        sine_track: SineTrack,
        mic_track: MicTrack,
        data_track: Option<LocalDataTrack>,
        // The platform ADM, held for the whole connection: it drives remote-audio
        // playout and is shared with `mic_track` for capture. Dropped on
        // disconnect (when `RunningState` is taken), which releases the ADM.
        platform_audio: Option<PlatformAudio>,
    }

    impl RunningState {
        /// Unpublishes every local track and reports each source unpublished.
        async fn unpublish_all(&mut self, ui_tx: &mpsc::UnboundedSender<UiCmd>) {
            for track in self.capture_tracks.values_mut() {
                track.unpublish().await;
            }
            if let Err(err) = self.sine_track.unpublish().await {
                log::warn!("failed to unpublish sine track: {err}");
            }
            if let Some(track) = self.data_track.take() {
                track.unpublish();
                let _ = ui_tx.send(UiCmd::DataTrackUnpublished);
            }
            if self.mic_track.is_published()
                && let Some(platform_audio) = self.platform_audio.clone()
                && let Err(err) = self.mic_track.unpublish(&platform_audio).await
            {
                log::warn!("failed to unpublish microphone: {err}");
            }
            for source in LocalSource::ALL {
                let state = PublishState::Unpublished;
                let _ = ui_tx.send(UiCmd::PublishState { source, state });
            }
        }

        /// Toggles `source` and returns whether it ends up published.
        async fn toggle_publish(
            &mut self,
            source: LocalSource,
            ui_tx: &mpsc::UnboundedSender<UiCmd>,
        ) -> bool {
            match source {
                LocalSource::Capture(capture) => {
                    let Some(track) = self.capture_tracks.get_mut(&capture) else {
                        return false;
                    };
                    if track.is_published() {
                        track.unpublish().await;
                    } else if let Err(err) = track.publish().await {
                        log::error!("failed to publish {capture:?} capture track: {err}");
                    }
                    track.is_published()
                }
                LocalSource::Sine => {
                    let result = if self.sine_track.is_published() {
                        self.sine_track.unpublish().await
                    } else {
                        self.sine_track.publish().await
                    };
                    if let Err(err) = result {
                        log::error!("failed to toggle sine track: {err}");
                    }
                    self.sine_track.is_published()
                }
                LocalSource::DataTrack => {
                    if let Some(track) = self.data_track.take() {
                        track.unpublish();
                        let _ = ui_tx.send(UiCmd::DataTrackUnpublished);
                        return false;
                    }
                    match self
                        .room
                        .local_participant()
                        .publish_data_track("slider")
                        .await
                    {
                        Ok(track) => {
                            let _ = ui_tx.send(UiCmd::DataTrackPublished {
                                track: track.clone(),
                            });
                            self.data_track = Some(track);
                            true
                        }
                        Err(err) => {
                            log::error!("failed to publish data track: {err}");
                            false
                        }
                    }
                }
            }
        }
    }

    let mut running_state: Option<RunningState> = None;
    let (disconnected_tx, mut disconnected_rx) = mpsc::unbounded_channel();

    loop {
        let event = tokio::select! {
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break };
                cmd
            }
            Some(()) = disconnected_rx.recv() => {
                // Ignore a stale notice from a room we already replaced.
                if let Some(mut state) = running_state.take_if(|state| {
                    state.room.connection_state() == ConnectionState::Disconnected
                }) {
                    state.unpublish_all(&inner.ui_tx).await;
                }
                continue;
            }
        };
        match event {
            AsyncCmd::RoomConnect {
                auth,
                auto_subscribe,
                dynacast,
                enable_e2ee,
                key,
            } => {
                // Resolved here rather than UI-side: the token-source method
                // fetches the connection details over HTTP, which must not
                // block the UI.
                let (url, token) = match auth.connection_details().await {
                    Ok(details) => details,
                    Err(err) => {
                        log::error!("failed to resolve connection details: {err}");
                        let _ = inner.ui_tx.send(UiCmd::ConnectResult { result: Err(err) });
                        continue;
                    }
                };

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
                    tokio::spawn(room_task(inner.clone(), events, disconnected_tx.clone()));

                    let new_room = Arc::new(new_room);

                    // Enable the platform ADM for the connection so remote audio
                    // plays out; also shared with the mic for capture.
                    let platform_audio = match PlatformAudio::new() {
                        Ok(audio) => Some(audio),
                        Err(err) => {
                            log::error!(
                                "failed to init platform audio (remote audio + mic disabled): {err}"
                            );
                            None
                        }
                    };

                    running_state = Some(RunningState {
                        room: new_room.clone(),
                        capture_tracks: capture_tracks(&new_room, &inner.ui_tx),
                        sine_track: SineTrack::new(new_room.clone(), SineParameters::default()),
                        mic_track: MicTrack::new(new_room.clone()),
                        data_track: None,
                        platform_audio,
                    });

                    // Allow direct access to the room from the UI (Used for sync access)
                    inner.room.lock().replace(new_room);

                    let _ = inner.ui_tx.send(UiCmd::ConnectResult { result: Ok(()) });
                } else if let Err(err) = res {
                    log::error!("failed to connect to room: {:?}", err);
                    let _ = inner.ui_tx.send(UiCmd::ConnectResult {
                        result: Err(err.to_string()),
                    });
                }
            }
            AsyncCmd::RoomDisconnect => {
                if let Some(mut state) = running_state.take() {
                    *inner.room.lock() = None;
                    state.unpublish_all(&inner.ui_tx).await;
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
            AsyncCmd::TogglePublish { source } => {
                let Some(state) = running_state.as_mut() else {
                    continue;
                };
                let report = |state| {
                    let _ = inner.ui_tx.send(UiCmd::PublishState { source, state });
                };
                report(PublishState::Pending);
                let published = state.toggle_publish(source, &inner.ui_tx).await;
                report(if published {
                    PublishState::Published
                } else {
                    PublishState::Unpublished
                });
            }
            AsyncCmd::ToggleMic => {
                if let Some(state) = running_state.as_mut() {
                    // Clone the ref-counted handle to sidestep a disjoint borrow of
                    // `state` (mic_track is borrowed mutably below).
                    if let Some(platform_audio) = state.platform_audio.clone() {
                        let result = if state.mic_track.is_published() {
                            state.mic_track.unpublish(&platform_audio).await
                        } else {
                            state.mic_track.publish(&platform_audio).await
                        };
                        if let Err(err) = result {
                            log::error!("failed to toggle microphone: {err}");
                        }
                    } else {
                        log::error!("cannot toggle microphone: platform audio unavailable");
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

/// One [`CaptureTrack`] per source, each reporting to the UI when its pump
/// exits so the menu never shows a dead track as published.
fn capture_tracks(
    room: &Arc<Room>,
    ui_tx: &mpsc::UnboundedSender<UiCmd>,
) -> HashMap<CaptureSource, CaptureTrack> {
    CaptureSource::ALL
        .map(|source| {
            let ui_tx = ui_tx.clone();
            let on_unpublished: OnUnpublished = Arc::new(move || {
                let _ = ui_tx.send(UiCmd::PublishState {
                    source: LocalSource::Capture(source),
                    state: PublishState::Unpublished,
                });
            });
            (
                source,
                CaptureTrack::new(room.clone(), source, on_unpublished),
            )
        })
        .into()
}

/// Task basically used to forward room events to the UI.
/// It will automatically close when the room is disconnected.
async fn room_task(
    inner: Arc<ServiceInner>,
    mut events: mpsc::UnboundedReceiver<RoomEvent>,
    disconnected_tx: mpsc::UnboundedSender<()>,
) {
    while let Some(event) = events.recv().await {
        if matches!(event, RoomEvent::Disconnected { .. }) {
            let _ = disconnected_tx.send(());
        }
        let _ = inner.ui_tx.send(UiCmd::RoomEvent { event });
    }
}
