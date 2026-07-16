use crate::connect::{ConnectSettings, ConnectView};
use crate::room::RoomWindow;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

mod connect;
mod media;
mod room;
mod service;
mod style;
mod ui;
mod utils;

pub static APP_NAME: &str = "LiveKit Client";

/// eframe storage key under which the connect form is persisted between runs.
const CONNECT_STORAGE_KEY: &str = "connect";

/// A room window, shown as a deferred viewport so it repaints independently of
/// the connect screen and of other rooms.
struct WindowEntry {
    id: u64,
    title: String,
    open: Arc<AtomicBool>,
    /// Shared with the deferred viewport callback, which runs outside of
    /// [`AppRoot::ui`] and therefore needs `Send + Sync` access.
    window: Arc<Mutex<RoomWindow>>,
}

pub struct AppRoot {
    async_runtime: tokio::runtime::Runtime,
    render_state: egui_wgpu::RenderState,
    connect: ConnectView,
    next_window_id: u64,
    windows: Vec<WindowEntry>,
}

impl AppRoot {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        utils::watch_for_deadlocks();

        style::install_fonts(&cc.egui_ctx);
        style::install_style(&cc.egui_ctx);

        let async_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        // Restore the last-used connect form; fall back to env-seeded defaults.
        let connect = cc
            .storage
            .and_then(|storage| eframe::get_value::<ConnectView>(storage, CONNECT_STORAGE_KEY))
            .unwrap_or_default();

        Self {
            connect,
            render_state: cc.wgpu_render_state.clone().unwrap(),
            async_runtime,
            next_window_id: 0,
            windows: Vec::new(),
        }
    }

    fn open_room(&mut self, request: ConnectSettings) {
        let id = self.next_window_id;
        self.next_window_id += 1;

        let title = format!("{} - {}", APP_NAME, request.url);
        let window = RoomWindow::new(
            id,
            self.async_runtime.handle().clone(),
            self.render_state.clone(),
            request,
        );

        self.windows.push(WindowEntry {
            id,
            title,
            open: Arc::new(AtomicBool::new(true)),
            window: Arc::new(Mutex::new(window)),
        });
    }
}

impl eframe::App for AppRoot {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, CONNECT_STORAGE_KEY, &self.connect);
    }

    /// Scope persistence to our own connect settings; don't persist egui memory.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(request) = self.connect.ui(ui) {
            self.open_room(request);
        }

        self.windows.retain(|window| {
            let open = window.open.load(Ordering::Relaxed);
            if !open {
                // Dropping LkService alone doesn't close the room; disconnect first.
                window.window.lock().disconnect();
            }
            open
        });

        // Deferred viewports must be re-declared every frame to stay open.
        for entry in &self.windows {
            let window = entry.window.clone();
            let open = entry.open.clone();
            ui.ctx().show_viewport_deferred(
                egui::ViewportId::from_hash_of(("lk_room", entry.id)),
                egui::ViewportBuilder::default()
                    .with_title(entry.title.clone())
                    .with_inner_size([800.0, 600.0]),
                move |ui, _class| {
                    window.lock().ui(ui);
                    if ui.input(|i| i.viewport().close_requested()) {
                        open.store(false, Ordering::Relaxed);
                        // The connect screen only repaints on interaction; wake it
                        // so it notices and drops this room.
                        ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
                    }
                },
            );
        }
    }
}
