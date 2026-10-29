use rust_dev_client::{APP_NAME, AppRoot};

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([420.0, 600.0])
        .with_resizable(false)
        // The form isn't scrollable, so don't let it shrink below its content.
        .with_min_inner_size([380.0, 480.0])
        .with_minimize_button(false);

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        // Persist only the connect form (see AppRoot::save), not window geometry —
        // the root window always opens centered at its default size.
        persist_window: false,
        ..Default::default()
    };

    let creator: eframe::AppCreator<'_> = Box::new(|context| Ok(Box::new(AppRoot::new(context))));
    eframe::run_native(APP_NAME, options, creator).unwrap();
}
