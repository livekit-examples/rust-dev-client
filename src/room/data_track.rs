use crate::ui::data_chart::DataTrackChart;
use futures::StreamExt;
use livekit::prelude::*;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

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
