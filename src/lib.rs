use crate::agent::AgentWindow;
use crate::connect::{ConnectRequest, ConnectView};
use crate::room::RoomWindow;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

mod agent;
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

/// The kinds of window a Connect click can open.
enum AppWindow {
    // Both are large; boxed so the enum stays small.
    Room(Box<RoomWindow>),
    Agent(Box<AgentWindow>),
}

impl AppWindow {
    fn ui(&mut self, ui: &mut egui::Ui) {
        match self {
            AppWindow::Room(window) => window.ui(ui),
            AppWindow::Agent(window) => window.ui(ui),
        }
    }

    fn disconnect(&self) {
        match self {
            AppWindow::Room(window) => window.disconnect(),
            AppWindow::Agent(window) => window.disconnect(),
        }
    }
}

/// A room or agent window, shown as a deferred viewport so it repaints
/// independently of the connect screen and of other windows.
struct WindowEntry {
    id: u64,
    title: String,
    open: Arc<AtomicBool>,
    /// Shared with the deferred viewport callback, which runs outside of
    /// [`AppRoot::ui`] and therefore needs `Send + Sync` access.
    window: Arc<Mutex<AppWindow>>,
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

    fn open_window(&mut self, request: ConnectRequest) {
        let id = self.next_window_id;
        self.next_window_id += 1;

        let runtime = self.async_runtime.handle().clone();
        let render_state = self.render_state.clone();
        let (title, window) = match request {
            ConnectRequest::Room(settings) => (
                format!("{} - {}", APP_NAME, settings.auth.target_label()),
                AppWindow::Room(Box::new(RoomWindow::new(runtime, render_state, settings))),
            ),
            ConnectRequest::Agent(settings) => (
                format!("{} - Agent - {}", APP_NAME, settings.auth.target_label()),
                AppWindow::Agent(Box::new(AgentWindow::new(runtime, render_state, &settings))),
            ),
        };

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
            self.open_window(request);
        }

        self.windows.retain(|window| {
            let open = window.open.load(Ordering::Relaxed);
            if !open {
                // Dropping a room window's service alone doesn't close the room; disconnect first.
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
