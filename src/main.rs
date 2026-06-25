use rust_dev_client::{APP_NAME, AppRoot};

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    // FIXME: root window has to always be on top to prevent glitches with room windows.

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([380.0, 480.0])
        .with_resizable(false)
        .with_always_on_top()
        .with_minimize_button(false);

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    let creator: eframe::AppCreator<'_> = Box::new(|context| Ok(Box::new(AppRoot::new(context))));
    eframe::run_native(APP_NAME, options, creator).unwrap();
}
