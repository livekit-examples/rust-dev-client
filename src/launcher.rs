/// Connection settings entered in the launcher; one copy per application,
/// passed to each connection window as it is opened.
#[derive(Clone)]
pub struct ConnectSettings {
    pub url: String,
    pub token: String,
    pub key: String,
    pub auto_subscribe: bool,
    pub enable_e2ee: bool,
}

impl Default for ConnectSettings {
    fn default() -> Self {
        Self {
            url: "ws://localhost:7880".to_string(),
            token: "".to_string(),
            auto_subscribe: true,
            enable_e2ee: false,
            key: "".to_string(),
        }
    }
}

/// The root window: a welcome screen holding the only connection form in the
/// app. Each successful Connect click spawns a dedicated connection window.
#[derive(Default)]
pub struct LauncherView {
    pub settings: ConnectSettings,
}

impl LauncherView {
    /// Returns the settings to open a connection with when Connect is clicked.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<ConnectSettings> {
        let mut request = None;

        egui::Frame::new().inner_margin(16.0).show(ui, |ui| {
            ui.monospace("Livekit - Connect to a room");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Url: ");
                ui.text_edit_singleline(&mut self.settings.url);
            });

            ui.horizontal(|ui| {
                ui.label("Token: ");
                ui.text_edit_singleline(&mut self.settings.token);
            });

            ui.horizontal(|ui| {
                ui.label("E2ee Key: ");
                ui.text_edit_singleline(&mut self.settings.key);
            });

            ui.checkbox(&mut self.settings.enable_e2ee, "Enable E2ee");
            ui.checkbox(&mut self.settings.auto_subscribe, "Auto Subscribe");

            ui.add_space(8.0);

            let can_connect =
                !self.settings.url.trim().is_empty() && !self.settings.token.trim().is_empty();
            ui.add_enabled_ui(can_connect, |ui| {
                if ui.button("Connect").clicked() {
                    request = Some(self.settings.clone());
                }
            });

            egui::warn_if_debug_build(ui);
        });

        request
    }
}
