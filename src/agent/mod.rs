//! The agent feature: a window driving a `livekit_session::Session`, with the
//! agent's state, the conversation feed, and its avatar. Depends on the
//! reusable toolkit in [`crate::ui`].

pub mod service;
pub mod status_bar;
pub mod window;

pub use window::AgentWindow;
