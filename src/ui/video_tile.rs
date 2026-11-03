use egui::load::SizedTexture;

/// Widget: paints a video frame into the available space, with a speaking
/// border and a participant-identity overlay. The frame keeps its aspect ratio:
/// it is scaled to fit and centered, and the rest of the cell shows the tile
/// background. Decoupled from any video source — the caller passes the
/// already-resolved texture, so this stays a pure UI primitive.
pub struct VideoTile<'a> {
    texture: Option<SizedTexture>,
    identity: &'a str,
    speaking: bool,
}

impl<'a> VideoTile<'a> {
    pub fn new(texture: Option<SizedTexture>, identity: &'a str, speaking: bool) -> Self {
        Self {
            texture,
            identity,
            speaking,
        }
    }
}

impl egui::Widget for VideoTile<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let inner_rect = rect.shrink(1.0);

        if self.speaking {
            ui.painter().rect(
                rect,
                egui::CornerRadius::default(),
                egui::Color32::GREEN,
                egui::Stroke::NONE,
                egui::StrokeKind::Inside,
            );
        }

        ui.painter().rect_filled(
            inner_rect,
            egui::CornerRadius::default(),
            ui.style().visuals.code_bg_color,
        );

        if let Some(texture) = self.texture {
            ui.painter().image(
                texture.id,
                fit_rect(inner_rect, texture.size),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        ui.painter().text(
            egui::pos2(rect.min.x + 5.0, rect.max.y - 5.0),
            egui::Align2::LEFT_BOTTOM,
            self.identity,
            egui::FontId::default(),
            egui::Color32::WHITE,
        );

        response
    }
}

/// `frame` scaled to the largest size with its aspect ratio that fits in `bounds`.
pub fn fit_size(bounds: egui::Vec2, frame: egui::Vec2) -> egui::Vec2 {
    let scale = (bounds.x / frame.x).min(bounds.y / frame.y);
    frame * scale
}

/// The largest rect with `frame`'s aspect ratio that fits inside `cell`, centered.
fn fit_rect(cell: egui::Rect, frame: egui::Vec2) -> egui::Rect {
    egui::Rect::from_center_size(cell.center(), fit_size(cell.size(), frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: egui::Rect =
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(320.0, 180.0));

    #[test]
    fn portrait_frame_fits_height() {
        let rect = fit_rect(CELL, egui::vec2(720.0, 1280.0));
        assert_eq!(rect.size(), egui::vec2(101.25, 180.0));
        assert_eq!(rect.center(), CELL.center());
    }

    #[test]
    fn wide_frame_fits_width() {
        let rect = fit_rect(CELL, egui::vec2(640.0, 200.0));
        assert_eq!(rect.size(), egui::vec2(320.0, 100.0));
        assert_eq!(rect.center(), CELL.center());
    }

    #[test]
    fn matching_frame_fills_cell() {
        assert_eq!(fit_rect(CELL, egui::vec2(1280.0, 720.0)), CELL);
    }
}
