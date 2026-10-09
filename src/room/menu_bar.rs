use crate::room::RoomContext;
use crate::service::{AsyncCmd, LkService, LocalSource, PublishState};
use livekit::SimulateScenario;
use std::collections::HashMap;

/// Top menu bar: Simulate / Publish / Debug actions, all sent to the service.
pub struct TopMenuBar<'a> {
    pub ctx: &'a RoomContext<'a>,
    pub publish_states: &'a HashMap<LocalSource, PublishState>,
}

impl egui::Widget for TopMenuBar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let service = self.ctx.service;
        egui::MenuBar::new()
            .ui(ui, |ui| {
                publish_menu(ui, service, self.publish_states);
                simulate_menu(ui, service);
                debug_menu(ui, service);
                help_menu(ui);
            })
            .response
    }
}

fn publish_menu(
    ui: &mut egui::Ui,
    service: &LkService,
    states: &HashMap<LocalSource, PublishState>,
) {
    ui.menu_button("Publish", |ui| {
        for source in LocalSource::ALL {
            let state = states.get(&source).copied().unwrap_or_default();
            // Throwaway copy: the service owns the real state and reports it back.
            let mut published = state == PublishState::Published;
            let checkbox = egui::Checkbox::new(&mut published, source.label());
            if ui
                .add_enabled(state != PublishState::Pending, checkbox)
                .on_hover_text(source.description())
                .clicked()
            {
                let _ = service.send(AsyncCmd::TogglePublish { source });
            }
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
    const COMMUNITY_URL: &str = "https://community.livekit.io/c/robotics";
    const DOCS_URL: &str = "https://docs.livekit.io/";
    const ISSUES_URL: &str = "https://github.com/livekit-examples/rust-dev-client/issues";

    ui.menu_button("Help", |ui| {
        if ui.button("Developer Community").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(COMMUNITY_URL));
        }
        if ui.button("Documentation").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(DOCS_URL));
        }
        if ui.button("Report an Issue").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(ISSUES_URL));
        }
    });
}
