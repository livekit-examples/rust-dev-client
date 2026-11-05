use crate::{
    media::VideoRenderer,
    ui::{
        placeholder_tile::{PlaceholderTile, placeholder_texture},
        status_badge::StatusBadge,
        video_tile::{VideoTile, fit_size},
    },
};
use livekit_session::{AgentActivity, AgentState};

/// Share of the available height the avatar may take.
const AVATAR_HEIGHT_FRACTION: f32 = 0.4;
const AVATAR_OVERLAY_MARGIN: f32 = 5.0;

/// Widget: the agent's avatar (or placeholder once it joined) with its state on
/// top; before that, only the state. The avatar keeps its aspect ratio: it is
/// scaled to fit the width and a share of the height, and centered.
pub struct AvatarPanel<'a> {
    pub state: AgentState,
    pub avatar: Option<&'a VideoRenderer>,
}

impl egui::Widget for AvatarPanel<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let bounds = egui::vec2(
            ui.available_width(),
            ui.available_height() * AVATAR_HEIGHT_FRACTION,
        );
        if !matches!(self.state, AgentState::Connected(_)) {
            let (rect, response) = ui.allocate_exact_size(bounds, egui::Sense::hover());
            ui.painter().rect_filled(
                rect.shrink(1.0),
                egui::CornerRadius::default(),
                ui.style().visuals.code_bg_color,
            );
            ui.place(rect, StatusBadge::from(self.state));
            return response;
        }

        let speaking = self.state == AgentState::Connected(AgentActivity::Speaking);
        let texture = self.avatar.and_then(VideoRenderer::texture);
        let size = texture.map_or(bounds, |texture| fit_size(bounds, texture.size));
        let (row, response) =
            ui.allocate_exact_size(egui::vec2(bounds.x, size.y), egui::Sense::hover());
        let rect = egui::Rect::from_center_size(row.center(), size);
        if self.avatar.is_some() {
            ui.place(rect, VideoTile::new(texture, "Avatar", speaking));
        } else {
            let placeholder = placeholder_texture(ui.ctx());
            ui.place(
                rect,
                PlaceholderTile::new(placeholder.id(), "Agent", speaking),
            );
        }
        ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink(AVATAR_OVERLAY_MARGIN))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        )
        .add(StatusBadge::from(self.state));
        response
    }
}
