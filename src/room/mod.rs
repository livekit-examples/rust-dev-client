//! The room feature: the per-room window plus its room-context widgets and
//! state. Depends on the reusable toolkit in [`crate::ui`]. The [`participants`]
//! and [`status_bar`] widgets are also used by [`crate::agent`].

pub mod context;
pub mod data_streams;
pub mod data_track;
pub mod menu_bar;
pub mod participants;
pub mod right_panel;
pub mod rpc;
pub mod status_bar;
pub mod track_grid_view;
pub mod window;

pub use context::RoomContext;
pub use window::RoomWindow;
