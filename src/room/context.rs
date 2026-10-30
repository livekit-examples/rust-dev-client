use crate::service::LkService;
use livekit::prelude::Room;

/// Shared, per-frame context passed to a room window's widgets: the service and
/// the optional connected room.
///
/// The rule for widgets: one that only reads the room takes `Option<&Room>`
/// directly (like `StatusBar` and `ParticipantsPanel`), which also lets the
/// agent window reuse it; one that sends [`AsyncCmd`](crate::service::AsyncCmd)s
/// takes `&RoomContext`.
pub struct RoomContext<'a> {
    pub service: &'a LkService,
    pub room: Option<&'a Room>,
}
