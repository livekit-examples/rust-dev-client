use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use std::sync::Arc;

/// Publishes real microphone audio via the shared platform Audio Device Module
/// (ADM). The [`PlatformAudio`] handle is owned by the connection (see
/// `RunningState` in `crate::service`) because it also drives remote-audio
/// playout; this type only controls the mic — it starts/stops ADM recording and
/// publishes/unpublishes the track on toggle. Frames are captured automatically
/// by WebRTC, so (unlike [`crate::media::SineTrack`]) there is no generation task.
pub struct MicTrack {
    room: Arc<Room>,
    track: Option<LocalAudioTrack>,
}

impl MicTrack {
    /// Track name used when publishing; also how the UI detects the mic is live.
    pub const TRACK_NAME: &str = "microphone";

    pub fn new(room: Arc<Room>) -> Self {
        Self { room, track: None }
    }

    pub fn is_published(&self) -> bool {
        self.track.is_some()
    }

    pub async fn publish(&mut self, platform_audio: &PlatformAudio) -> Result<(), RoomError> {
        // Resume mic capture (initializes recording on the first start).
        if let Err(err) = platform_audio.start_recording() {
            log::error!("failed to start platform audio recording: {err}");
        }

        let track =
            LocalAudioTrack::create_audio_track(Self::TRACK_NAME, platform_audio.rtc_source());

        self.room
            .local_participant()
            .publish_track(
                LocalTrack::Audio(track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await?;

        self.track = Some(track);
        Ok(())
    }

    pub async fn unpublish(&mut self, platform_audio: &PlatformAudio) -> Result<(), RoomError> {
        if let Some(track) = self.track.take() {
            self.room
                .local_participant()
                .unpublish_track(&track.sid())
                .await?;
        }

        // Pause capture (turns off the OS mic indicator); the ADM stays alive for
        // remote-audio playout and is disposed only when the connection ends.
        if let Err(err) = platform_audio.stop_recording() {
            log::error!("failed to stop platform audio recording: {err}");
        }

        Ok(())
    }
}
