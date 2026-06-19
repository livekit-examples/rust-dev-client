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
                simulate_menu(ui, service);
                publish_menu(ui, service);
                debug_menu(ui, service);
            })
            .response
    }
}

fn simulate_menu(ui: &mut egui::Ui, service: &LkService) {
    const SIMULATE_SCENARIOS: [SimulateScenario; 7] = [
        SimulateScenario::SignalReconnect,
        SimulateScenario::Speaker,
        SimulateScenario::NodeFailure,
        SimulateScenario::ServerLeave,
        SimulateScenario::Migration,
        SimulateScenario::ForceTcp,
        SimulateScenario::ForceTls,
    ];
    ui.menu_button("Simulate", |ui| {
        for scenario in SIMULATE_SCENARIOS {
            if ui.button(format!("{:?}", scenario)).clicked() {
                let _ = service.send(AsyncCmd::SimulateScenario { scenario });
            }
        }
        if ui.button("E2eeKeyRatchet").clicked() {
            let _ = service.send(AsyncCmd::E2eeKeyRatchet);
        }
    });
}

fn publish_menu(ui: &mut egui::Ui, service: &LkService) {
    ui.menu_button("Publish", |ui| {
        if ui.button("Logo").clicked() {
            let _ = service.send(AsyncCmd::ToggleLogo);
        }
        if ui.button("SineWave").clicked() {
            let _ = service.send(AsyncCmd::ToggleSine);
        }
        if ui.button("DataTrack").clicked() {
            let _ = service.send(AsyncCmd::ToggleDataTrack);
        }
    });
}

fn debug_menu(ui: &mut egui::Ui, service: &LkService) {
    ui.menu_button("Debug", |ui| {
        if ui.button("Log stats").clicked() {
            let _ = service.send(AsyncCmd::LogStats);
        }
    });
}
