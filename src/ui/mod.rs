//! Reusable UI toolkit: widgets and layout helpers with **no** LiveKit / room /
//! service / `ConnCtx` dependency. The room-context feature in `crate::connection`
//! depends on this layer, never the reverse.

pub mod data_chart;
pub mod labeled_field;
pub mod prominent_button;
pub mod status_badge;
pub mod video_grid;
pub mod video_tile;
