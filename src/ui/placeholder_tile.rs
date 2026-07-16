/// Cached placeholder texture, decoded once and shared across frames and windows
/// via egui temp-data (all viewports share one `Context`). Returns a handle whose
/// `.id()` feeds [`PlaceholderTile`].
pub fn placeholder_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("placeholder_tile_texture");
    if let Some(handle) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return handle;
    }

    let image = image::load_from_memory_with_format(
        include_bytes!("../../resources/PlaceholderTileSquare.png"),
        image::ImageFormat::Png,
    )
    .expect("placeholder image is a valid PNG")
    .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());

    let handle = ctx.load_texture("placeholder_tile", color, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, handle.clone()));
    handle
}

/// Widget: stands in for a participant that isn't sending live video. Paints a
/// centered silhouette on a dark cell, with a speaking border and a
/// participant-identity overlay. Mirrors [`crate::ui::video_tile::VideoTile`].
pub struct PlaceholderTile<'a> {
    texture: egui::TextureId,
    identity: &'a str,
    speaking: bool,
}

impl<'a> PlaceholderTile<'a> {
    pub fn new(texture: egui::TextureId, identity: &'a str, speaking: bool) -> Self {
        Self {
            texture,
            identity,
            speaking,
        }
    }
}

impl egui::Widget for PlaceholderTile<'_> {
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

        // The silhouette is square; fit it (contain) and center it in the cell.
        let side = inner_rect.width().min(inner_rect.height());
        let image_rect = egui::Rect::from_center_size(inner_rect.center(), egui::Vec2::splat(side));
        ui.painter().image(
            self.texture,
            image_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

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
