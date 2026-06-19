use crate::service::LkService;
use livekit::prelude::Room;

/// Shared, per-frame context passed to a room window's widgets: the service,
/// the optional connected room, and a window-scoped id used to salt
/// widget ids (all viewports share one `egui::Context`, so ids must be unique
/// per window). Widgets derive child ids via `ctx.id.with("name")`.
pub struct RoomContext<'a> {
    pub service: &'a LkService,
    pub room: Option<&'a Room>,
    pub id: egui::Id,
}
