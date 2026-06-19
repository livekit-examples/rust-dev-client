//! The room feature: the per-room window plus its room-context widgets and
//! state. Depends on the reusable toolkit in [`crate::ui`].

pub mod context;
pub mod data_track;
pub mod menu_bar;
pub mod participants;
pub mod right_panel;
pub mod rpc;
pub mod sidebar;
pub mod video_grid_view;
pub mod window;

pub use context::RoomContext;
pub use window::RoomWindow;
