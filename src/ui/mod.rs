//! Reusable UI toolkit: widgets and layout helpers with **no** LiveKit / room /
//! service / `RoomCtx` dependency. The room feature in `crate::room` depends on
//! this layer, never the reverse.

pub mod data_chart;
pub mod labeled_field;
pub mod placeholder_tile;
pub mod prominent_button;
pub mod status_badge;
pub mod track_grid;
pub mod video_tile;
