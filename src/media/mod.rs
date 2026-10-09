//! Media plumbing over WebRTC, independent of the rest of the app (no `crate::`
//! deps): the local sources we publish — generated video from `livekit-capture`
//! ([`CaptureTrack`]), a synthetic sine-wave audio track ([`SineTrack`]), and a
//! real microphone audio track ([`MicTrack`]) — plus the [`VideoRenderer`] that
//! turns incoming video frames into egui textures.

pub mod capture_track;
pub mod mic_track;
pub mod sine_track;
pub mod video_renderer;

pub use capture_track::{CaptureSource, CaptureTrack, OnUnpublished};
pub use mic_track::MicTrack;
pub use sine_track::{SineParameters, SineTrack};
pub use video_renderer::VideoRenderer;
