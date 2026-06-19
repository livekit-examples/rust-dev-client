//! Media plumbing over WebRTC, independent of the rest of the app (no `crate::`
//! deps): the synthetic local sources we publish — a generated logo video
//! ([`LogoTrack`]) and a sine-wave audio track ([`SineTrack`]) — plus the
//! [`VideoRenderer`] that turns incoming video frames into egui textures.

pub mod logo_track;
pub mod sine_track;
pub mod video_renderer;

pub use logo_track::LogoTrack;
pub use sine_track::{SineParameters, SineTrack};
pub use video_renderer::VideoRenderer;
