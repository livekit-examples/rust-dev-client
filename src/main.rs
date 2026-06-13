mod app;
mod connection_window;
mod data_track;
mod launcher;
mod logo_track;
mod rpc_ui;
mod service;
mod sine_track;
mod utils;
mod video_grid;
mod video_renderer;

static APP_NAME: &str = "LiveKit Client";

fn main() {
    env_logger::init();
    utils::watch_for_deadlocks();

    // The root window is the launcher: a compact connect form.
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([420.0, 280.0])
        .with_icon(egui::IconData::default()); // TODO: add custom icon

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    let creator: eframe::AppCreator<'_> =
        Box::new(|context| Ok(Box::new(app::AppRoot::new(context))));

    eframe::run_native(APP_NAME, options, creator).unwrap();
}
