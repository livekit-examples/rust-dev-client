use crate::connection::ConnCtx;
use crate::service::AsyncCmd;
use livekit::SimulateScenario;

/// Top menu bar: Simulate / Publish / Debug actions, all sent to the service.
pub struct TopMenuBar<'a> {
    pub ctx: &'a ConnCtx<'a>,
}

impl egui::Widget for TopMenuBar<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let service = self.ctx.service;
        egui::MenuBar::new()
            .ui(ui, |ui| {
                ui.menu_button("Simulate", |ui| {
                    let scenarios = [
                        SimulateScenario::SignalReconnect,
                        SimulateScenario::Speaker,
                        SimulateScenario::NodeFailure,
                        SimulateScenario::ServerLeave,
                        SimulateScenario::Migration,
                        SimulateScenario::ForceTcp,
                        SimulateScenario::ForceTls,
                    ];

                    for scenario in scenarios {
                        if ui.button(format!("{:?}", scenario)).clicked() {
                            let _ = service.send(AsyncCmd::SimulateScenario { scenario });
                        }
                    }
                });

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

                ui.menu_button("Debug", |ui| {
                    if ui.button("Log stats").clicked() {
                        let _ = service.send(AsyncCmd::LogStats);
                    }
                });
            })
            .response
    }
}
