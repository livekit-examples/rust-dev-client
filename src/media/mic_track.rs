use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use std::sync::Arc;

/// Publishes real microphone audio via the platform Audio Device Module (ADM).
/// Unlike [`crate::media::SineTrack`], frames are captured automatically by
/// WebRTC, so there is no generation task.
pub struct MicTrack {
    room: Arc<Room>,
    handle: Option<TrackHandle>,
}

struct TrackHandle {
    track: LocalAudioTrack,
    // Held for its lifetime to keep the platform ADM recording; dropping it
    // releases the ADM. Underscore-prefixed: held for its drop, never read.
    _platform_audio: PlatformAudio,
}

impl MicTrack {
    /// Track name used when publishing; also how the UI detects the mic is live.
    pub const TRACK_NAME: &str = "microphone";

    pub fn new(room: Arc<Room>) -> Self {
        Self { room, handle: None }
    }

    pub fn is_published(&self) -> bool {
        self.handle.is_some()
    }

    pub async fn publish(&mut self) -> Result<(), RoomError> {
        let platform_audio = match PlatformAudio::new() {
            Ok(audio) => audio,
            Err(err) => {
                log::error!("failed to initialize platform audio: {err}");
                return Ok(()); // stay unpublished so the toggle can retry
            }
        };

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

        self.handle = Some(TrackHandle {
            track,
            _platform_audio: platform_audio,
        });
        Ok(())
    }

    pub async fn unpublish(&mut self) -> Result<(), RoomError> {
        if let Some(handle) = self.handle.take() {
            self.room
                .local_participant()
                .unpublish_track(&handle.track.sid())
                .await?;
        }

        Ok(())
    }
}
