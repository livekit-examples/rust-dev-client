use crate::connection_window::ConnectionWindow;
use crate::launcher::{ConnectSettings, LauncherView};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A connection window, shown as a deferred viewport so it repaints
/// independently of the launcher and of other connections.
struct WindowEntry {
    id: u64,
    title: String,
    open: Arc<AtomicBool>,
    /// Shared with the deferred viewport callback, which runs outside of
    /// [`AppRoot::ui`] and therefore needs `Send + Sync` access.
    window: Arc<Mutex<ConnectionWindow>>,
}

pub struct AppRoot {
    async_runtime: tokio::runtime::Runtime,
    render_state: egui_wgpu::RenderState,
    launcher: LauncherView,
    next_window_id: u64,
    windows: Vec<WindowEntry>,
}

impl AppRoot {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let async_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self {
            launcher: LauncherView::default(),
            render_state: cc.wgpu_render_state.clone().unwrap(),
            async_runtime,
            next_window_id: 0,
            windows: Vec::new(),
        }
    }

    fn open_connection(&mut self, request: ConnectSettings) {
        let id = self.next_window_id;
        self.next_window_id += 1;

        let title = format!("{} - {}", crate::APP_NAME, request.url);
        let window = ConnectionWindow::new(
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(request) = self.launcher.ui(ui) {
            self.open_connection(request);
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
                egui::ViewportId::from_hash_of(("lk_connection", entry.id)),
                egui::ViewportBuilder::default()
                    .with_title(entry.title.clone())
                    .with_inner_size([800.0, 600.0]),
                move |ui, _class| {
                    window.lock().ui(ui);
                    if ui.input(|i| i.viewport().close_requested()) {
                        open.store(false, Ordering::Relaxed);
                        // The launcher only repaints on interaction; wake it so
                        // it notices and drops this window's connection.
                        ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
                    }
                },
            );
        }
    }
}
