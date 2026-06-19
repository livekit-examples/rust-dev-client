/// Widget: paints a video frame to fill the available space, with a speaking
/// border and a resolution/name overlay. Decoupled from any video source — the
/// caller passes the already-resolved texture id and resolution, so this stays
/// a pure UI primitive.
pub struct VideoTile<'a> {
    texture: Option<egui::TextureId>,
    resolution: (u32, u32),
    name: &'a str,
    speaking: bool,
}

impl<'a> VideoTile<'a> {
    pub fn new(
        texture: Option<egui::TextureId>,
        resolution: (u32, u32),
        name: &'a str,
        speaking: bool,
    ) -> Self {
        Self {
            texture,
            resolution,
            name,
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

        if let Some(tex) = self.texture {
            ui.painter().image(
                tex,
                inner_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        let (width, height) = self.resolution;
        ui.painter().text(
            egui::pos2(rect.min.x + 5.0, rect.max.y - 5.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{}x{} {}", width, height, self.name),
            egui::FontId::default(),
            egui::Color32::WHITE,
        );

        response
    }
}
