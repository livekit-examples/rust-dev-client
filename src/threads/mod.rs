//! The threads feature: a window listing the threads of one scope through a
//! `livekit_thread::ThreadDirectory`, with a chat and a call in the selected
//! thread. Reuses the agent feature's feed and avatar.

pub mod service;
pub mod window;

pub use window::ThreadsWindow;
