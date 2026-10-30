//! The agent feature: a window driving a `livekit_session::Session`, with the
//! agent's state, the conversation feed, and its avatar. Depends on the
//! reusable toolkit in [`crate::ui`] and reuses the participants panel and
//! status bar from [`crate::room`].

pub mod service;
pub mod window;

pub use window::AgentWindow;
