use egui::{Color32, CornerRadius, Rect, Stroke, emath, epaint, pos2};
use futures::StreamExt;
use livekit::prelude::*;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const TIME_WINDOW: Duration = Duration::from_secs(30);
pub const MAX_VALUE: f32 = 512.0;

pub struct LocalDataTrackTile {
    track: LocalDataTrack,
    pub slider_value: i32,
    pub points: Arc<Mutex<VecDeque<(Instant, i32)>>>,
    pub name: String,
}

impl LocalDataTrackTile {
    pub fn new(track: LocalDataTrack) -> Self {
        let name = track.info().name().to_string();
        Self {
            track,
            slider_value: 0,
            points: Arc::new(Mutex::new(VecDeque::new())),
            name,
        }
    }

    pub fn push_value(&self) {
        let frame = DataTrackFrame::new(self.slider_value.to_string().into_bytes());
        let _ = self.track.try_push(frame);
        self.points
            .lock()
            .push_front((Instant::now(), self.slider_value));
    }
}

pub struct RemoteDataTrackTile {
    pub points: Arc<Mutex<VecDeque<(Instant, i32)>>>,
    pub publisher_identity: String,
    pub name: String,
}

impl RemoteDataTrackTile {
    pub fn new(async_handle: &tokio::runtime::Handle, track: RemoteDataTrack) -> Self {
        let points = Arc::new(Mutex::new(VecDeque::new()));
        let points_ref = points.clone();
        let publisher_identity = track.publisher_identity().to_string();
        let name = track.info().name().to_string();

        async_handle.spawn(async move {
            let mut stream = match track.subscribe().await {
                Ok(s) => s,
                Err(err) => {
                    log::error!("Failed to subscribe to data track: {err}");
                    return;
                }
            };
            while let Some(frame) = stream.next().await {
                let payload = frame.payload();
                let Ok(s) = std::str::from_utf8(&payload) else {
                    continue;
                };
                let Ok(value) = s.parse::<i32>() else {
                    continue;
                };
                points_ref.lock().push_front((Instant::now(), value));
            }
        });

        Self {
            points,
            publisher_identity,
            name,
        }
    }
}

/// Widget: an interactive local data-track tile. Dragging sets the value and
/// pushes a frame onto the track.
pub struct LocalDataTrackWidget<'a> {
    tile: &'a mut LocalDataTrackTile,
}

impl<'a> LocalDataTrackWidget<'a> {
    pub fn new(tile: &'a mut LocalDataTrackTile) -> Self {
        Self { tile }
    }
}

impl egui::Widget for LocalDataTrackWidget<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let tile = self.tile;
        let chart = DataTrackChart::new(&tile.points, &tile.name, "local")
            .interactive(&mut tile.slider_value);
        let response = ui.add(chart);
        if response.changed() {
            tile.push_value();
        }
        response
    }
}

/// Widget: a read-only remote data-track tile.
pub struct RemoteDataTrackWidget<'a> {
    tile: &'a RemoteDataTrackTile,
}

impl<'a> RemoteDataTrackWidget<'a> {
    pub fn new(tile: &'a RemoteDataTrackTile) -> Self {
        Self { tile }
    }
}

impl egui::Widget for RemoteDataTrackWidget<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.add(DataTrackChart::new(
            &self.tile.points,
            &self.tile.name,
            &self.tile.publisher_identity,
        ))
    }
}

/// Reusable chart widget plotting a data-track's value over a time window.
/// Pass `.interactive(&mut value)` to make it a draggable input.
pub struct DataTrackChart<'a> {
    points: &'a Mutex<VecDeque<(Instant, i32)>>,
    name: &'a str,
    publisher_label: &'a str,
    drag_value: Option<&'a mut i32>,
}

impl<'a> DataTrackChart<'a> {
    pub fn new(
        points: &'a Mutex<VecDeque<(Instant, i32)>>,
        name: &'a str,
        publisher_label: &'a str,
    ) -> Self {
        Self {
            points,
            name,
            publisher_label,
            drag_value: None,
        }
    }

    pub fn interactive(mut self, value: &'a mut i32) -> Self {
        self.drag_value = Some(value);
        self
    }
}

impl egui::Widget for DataTrackChart<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut drag_value = self.drag_value;
        let interactive = drag_value.is_some();
        let sense = if interactive {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        };

        let desired_size = ui.available_size();
        let (rect, mut response) = ui.allocate_exact_size(desired_size, sense);
        let painter = ui.painter();

        let bg = Color32::from_rgb(0x1a, 0x1a, 0x2e);
        painter.rect_filled(rect, CornerRadius::default(), bg);

        let v_margin = rect.height() * 0.15;
        let h_margin = 8.0;
        let label_width = 32.0;
        let plot_rect = Rect::from_min_max(
            pos2(rect.min.x + h_margin, rect.min.y + v_margin),
            pos2(rect.max.x - h_margin - label_width, rect.max.y - v_margin),
        );

        let time_window_secs = TIME_WINDOW.as_secs_f32();
        let to_screen = emath::RectTransform::from_to(
            Rect::from_x_y_ranges(time_window_secs..=0.0, MAX_VALUE..=0.0),
            plot_rect,
        );

        let guide_color = Color32::from_rgb(0x40, 0x40, 0x50);
        let max_y = (to_screen * pos2(0.0, MAX_VALUE)).y;
        let min_y = (to_screen * pos2(0.0, 0.0)).y;
        painter.line_segment(
            [pos2(plot_rect.min.x, max_y), pos2(plot_rect.max.x, max_y)],
            Stroke::new(1.0, guide_color),
        );
        painter.line_segment(
            [pos2(plot_rect.min.x, min_y), pos2(plot_rect.max.x, min_y)],
            Stroke::new(1.0, guide_color),
        );

        if let Some(value) = &mut drag_value
            && let Some(pointer_pos) = response.interact_pointer_pos()
        {
            let from_screen = to_screen.inverse();
            let logical = from_screen * pointer_pos;
            let new_val = (logical.y as i32).clamp(0, MAX_VALUE as i32);
            if **value != new_val {
                **value = new_val;
                response.mark_changed();
            }
        }

        let now = std::time::Instant::now();
        let mut points = self.points.lock();
        while points
            .back()
            .is_some_and(|(t, _)| now.duration_since(*t) > TIME_WINDOW)
        {
            points.pop_back();
        }

        let is_interacting = response.interact_pointer_pos().is_some();
        let display_val = drag_value
            .as_deref()
            .copied()
            .filter(|_| !points.is_empty() || is_interacting)
            .or_else(|| points.front().map(|(_, v)| *v));

        let line_color = Color32::from_rgb(0xFF, 0x44, 0x44);

        if !points.is_empty() {
            let mut screen_points = Vec::with_capacity(points.len() + 1);
            if let Some(val) = display_val {
                screen_points.push(to_screen * pos2(0.0, val as f32));
            }
            for &(t, val) in points.iter() {
                let age = now.duration_since(t).as_secs_f32();
                screen_points.push(to_screen * pos2(age, val as f32));
            }
            drop(points);
            painter.add(epaint::Shape::line(
                screen_points,
                epaint::PathStroke::new(2.0, line_color),
            ));
            ui.ctx().request_repaint();
        } else {
            drop(points);
        }

        if let Some(val) = display_val {
            let newest_screen = to_screen * pos2(0.0, val as f32);
            let is_active = interactive && (response.hovered() || response.dragged());
            let dot_radius = if is_active { 6.0 } else { 4.0 };
            painter.circle_filled(newest_screen, dot_radius, line_color);
            if is_active {
                painter.circle_stroke(
                    newest_screen,
                    dot_radius + 2.0,
                    Stroke::new(1.5, Color32::WHITE),
                );
            }

            painter.text(
                pos2(plot_rect.max.x + 8.0, newest_screen.y),
                egui::Align2::LEFT_CENTER,
                val.to_string(),
                egui::FontId::monospace(14.0),
                Color32::WHITE,
            );
        } else {
            let hint = if interactive {
                "Drag to Push Frames…"
            } else {
                "Waiting for Frames…"
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                hint,
                egui::FontId::proportional(18.0),
                Color32::WHITE,
            );
        }

        painter.text(
            pos2(rect.min.x + 5.0, rect.max.y - 5.0),
            egui::Align2::LEFT_BOTTOM,
            format!("Data: {} ({})", self.name, self.publisher_label),
            egui::FontId::default(),
            Color32::WHITE,
        );

        if interactive {
            response = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
        }

        response
    }
}
