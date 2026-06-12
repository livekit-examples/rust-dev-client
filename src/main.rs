use eframe::Renderer;
use parking_lot::deadlock;
use std::thread;
use std::time::Duration;

mod app;
mod data_track;
mod logo_track;
mod rpc_ui;
mod service;
mod sine_track;
mod video_grid;
mod video_renderer;
mod utils;

fn main() {
    env_logger::init();
    utils::watch_for_deadlocks();

    eframe::run_native(
        "LiveKit Client",
        eframe::NativeOptions {
            centered: true,
            renderer: Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(|context| Ok(Box::new(app::AppRoot::new(context)))),
    )
    .unwrap();
}
