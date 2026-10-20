use crate::service::LkService;
use livekit::prelude::Room;

/// Shared, per-frame context passed to a room window's widgets: the service and
/// the optional connected room.
pub struct RoomContext<'a> {
    pub service: &'a LkService,
    pub room: Option<&'a Room>,
}
