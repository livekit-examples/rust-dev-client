use crate::room::RoomContext;
use crate::service::{AsyncCmd, LkService};
use livekit::SimulateScenario;

/// Top menu bar: Simulate / Publish / Debug actions, all sent to the service.
pub struct TopMenuBar<'a> {
    pub ctx: &'a RoomContext<'a>,
}

impl egui::Widget for TopMenuBar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let service = self.ctx.service;
        egui::MenuBar::new()
            .ui(ui, |ui| {
                publish_menu(ui, service);
                simulate_menu(ui, service);
                debug_menu(ui, service);
                help_menu(ui);
            })
            .response
    }
}

fn publish_menu(ui: &mut egui::Ui, service: &LkService) {
    ui.menu_button("Publish", |ui| {
        if ui.button("Logo").clicked() {
            let _ = service.send(AsyncCmd::ToggleLogo);
        }
        if ui.button("Sine Wave").clicked() {
            let _ = service.send(AsyncCmd::ToggleSine);
        }
        if ui.button("Data Track").clicked() {
            let _ = service.send(AsyncCmd::ToggleDataTrack);
        }
    });
}

fn simulate_menu(ui: &mut egui::Ui, service: &LkService) {
    const SIMULATE_SCENARIOS: [(SimulateScenario, &str); 7] = [
        (SimulateScenario::SignalReconnect, "Signal Reconnect"),
        (SimulateScenario::Speaker, "Speaker"),
        (SimulateScenario::NodeFailure, "Node Failure"),
        (SimulateScenario::ServerLeave, "Server Leave"),
        (SimulateScenario::Migration, "Migration"),
        (SimulateScenario::ForceTcp, "Force TCP"),
        (SimulateScenario::ForceTls, "Force TLS"),
    ];
    ui.menu_button("Simulate", |ui| {
        for (scenario, label) in SIMULATE_SCENARIOS {
            if ui.button(label).clicked() {
                let _ = service.send(AsyncCmd::SimulateScenario { scenario });
            }
        }
        if ui.button("E2EE Key Ratchet").clicked() {
            let _ = service.send(AsyncCmd::E2eeKeyRatchet);
        }
    });
}

fn debug_menu(ui: &mut egui::Ui, service: &LkService) {
    ui.menu_button("Debug", |ui| {
        if ui.button("Log Statistics").clicked() {
            let _ = service.send(AsyncCmd::LogStats);
        }
    });
}

fn help_menu(ui: &mut egui::Ui) {
    const DOCS_URL: &str = "https://docs.livekit.io/";
    const ISSUES_URL: &str = "https://github.com/livekit-examples/rust-dev-client/issues";

    ui.menu_button("Help", |ui| {
        if ui.button("LiveKit Documentation").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(DOCS_URL));
        }
        if ui.button("Report an Issue").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(ISSUES_URL));
        }
    });
}
