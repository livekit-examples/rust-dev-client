use livekit::options::{TrackPublishOptions, VideoCodec};
use livekit::prelude::*;
use livekit_capture::{
    error::SourceError,
    pixel::{PixelVideoPump, PixelVideoSource},
    primitive::VideoResolution,
    pump::PumpStop,
    sources::{
        clock::{ClockVideoSource, ClockVideoSourceConfig},
        pattern::{Pattern, PatternVideoSource, PatternVideoSourceConfig},
    },
};
use std::error::Error;
use std::sync::Arc;
use tokio::task::JoinHandle;

const RESOLUTION: VideoResolution = VideoResolution::new(1280, 720);
const FRAMERATE_FPS: u32 = 30;

type BoxError = Box<dyn Error + Send + Sync>;

/// A user-publishable video source backed by `livekit-capture`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureSource {
    Gradient,
    Logo,
    Clock,
}

impl CaptureSource {
    pub const ALL: [Self; 3] = [Self::Gradient, Self::Logo, Self::Clock];

    pub fn label(self) -> &'static str {
        match self {
            Self::Gradient => "Video: Gradient",
            Self::Logo => "Video: Logo",
            Self::Clock => "Video: Clock",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Gradient => "Animated color gradient test pattern",
            Self::Logo => "Bouncing LiveKit logo test pattern",
            Self::Clock => "Local wall clock with millisecond precision, for measuring latency",
        }
    }

    fn track_name(self) -> &'static str {
        match self {
            Self::Gradient => "gradient",
            Self::Logo => "logo",
            Self::Clock => "clock",
        }
    }

    /// Builds the source; GPU setup runs on the blocking pool.
    async fn open(self) -> Result<Box<dyn PixelVideoSource>, SourceError> {
        let pattern = match self {
            Self::Gradient => Pattern::Gradient,
            Self::Logo => Pattern::Logo,
            Self::Clock => {
                let config = ClockVideoSourceConfig {
                    resolution: RESOLUTION,
                    framerate_fps: FRAMERATE_FPS,
                };
                return Ok(Box::new(ClockVideoSource::new(config).await?));
            }
        };
        let config = PatternVideoSourceConfig {
            resolution: RESOLUTION,
            framerate_fps: FRAMERATE_FPS,
            pattern,
        };
        Ok(Box::new(PatternVideoSource::new(config).await?))
    }
}

/// Publishes a [`CaptureSource`] as a video track.
///
/// The track's publication is tied to its pump: a task owns the running pump
/// and unpublishes the track whenever the pump exits, whether stopped here or
/// on its own (end of stream, error, or panic). Dropping stops the pump without
/// waiting for the unpublish.
pub struct CaptureTrack {
    room: Arc<Room>,
    source: CaptureSource,
    handle: Option<TrackHandle>,
}

struct TrackHandle {
    stop: PumpStop,
    /// Finishes once the pump has exited and the track is unpublished.
    task: JoinHandle<()>,
}

impl CaptureTrack {
    pub fn new(room: Arc<Room>, source: CaptureSource) -> Self {
        Self {
            room,
            source,
            handle: None,
        }
    }

    pub fn is_published(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(|handle| !handle.task.is_finished())
    }

    pub async fn publish(&mut self) -> Result<(), BoxError> {
        self.unpublish().await;

        let pump = PixelVideoPump::new(self.source.open().await?);
        let track =
            LocalVideoTrack::create_video_track(self.source.track_name(), pump.rtc_source());
        let options = TrackPublishOptions {
            source: TrackSource::Camera,
            simulcast: true,
            video_codec: VideoCodec::H265,
            ..pump.publish_options()
        };
        let participant = self.room.local_participant();
        participant
            .publish_track(LocalTrack::Video(track.clone()), options)
            .await?;

        let pump = match pump.spawn() {
            Ok(pump) => pump,
            Err(err) => {
                let _ = participant.unpublish_track(&track.sid()).await;
                return Err(err.into());
            }
        };
        let stop = pump.stop_handle();
        let source = self.source;
        let task = tokio::spawn(async move {
            let result = pump.join().await;
            log::info!("{source:?} capture pump exited: {result:?}");
            if let Err(err) = participant.unpublish_track(&track.sid()).await {
                log::warn!("failed to unpublish {source:?} capture track: {err}");
            }
        });
        self.handle = Some(TrackHandle { stop, task });
        Ok(())
    }

    /// Stops the pump and waits for its task to unpublish the track.
    pub async fn unpublish(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.stop.stop();
            let _ = handle.task.await;
        }
    }
}

impl Drop for CaptureTrack {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.stop.stop();
        }
    }
}
