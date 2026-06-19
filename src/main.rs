mod app;
mod connection;
mod launcher;
mod logo_track;
mod service;
mod sine_track;
mod ui;
mod utils;
mod video_renderer;

static APP_NAME: &str = "LiveKit Client";

fn main() {
    env_logger::init();
    utils::watch_for_deadlocks();

    // FIXME: root window has to always be on top to prevent glitches with room windows.

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([280.0, 480.0])
        .with_resizable(false)
        .with_always_on_top()
        .with_minimize_button(false);

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    let creator: eframe::AppCreator<'_> =
        Box::new(|context| Ok(Box::new(app::AppRoot::new(context))));

    eframe::run_native(APP_NAME, options, creator).unwrap();
}
