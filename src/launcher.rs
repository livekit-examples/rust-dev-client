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
    fn is_connect_enabled(&self) -> bool {
        !self.settings.url.trim().is_empty() && !self.settings.token.trim().is_empty()
    }

    /// Returns the settings to open a connection with when Connect is clicked.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<ConnectSettings> {
        let mut request = None;

        let frame = egui::Frame::central_panel(ui.style()).inner_margin(16.0);

        egui::Panel::bottom("launcher_info").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let version_string = format!("SDK Version: {}", livekit::SDK_VERSION);
                let text = egui::RichText::new(version_string).text_style(egui::TextStyle::Small);
                ui.label(text);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::warn_if_debug_build(ui);
                });
            });
        });

        egui::Panel::bottom("launcher_connect")
            .frame(frame)
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.add_enabled_ui(self.is_connect_enabled(), |ui| {
                    let text = egui::RichText::new("Connect").color(egui::Color32::WHITE);

                    let button = egui::Button::new(text)
                        .fill(egui::Color32::from_rgb(40, 120, 200))
                        .min_size(egui::vec2(ui.available_width(), 32.0));

                    if ui.add(button).clicked() {
                        request = Some(self.settings.clone());
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(frame)
            .show_inside(ui, |ui| {
                let connect_string =
                    egui::RichText::new("Connect to a Room").text_style(egui::TextStyle::Heading);

                ui.label(connect_string);
                ui.add_space(8.0);

                ui.vertical(|ui| {
                    let label = ui.label("URL");
                    ui.text_edit_singleline(&mut self.settings.url)
                        .labelled_by(label.id);
                });
                ui.add_space(8.0);

                ui.vertical(|ui| {
                    let label = ui.label("Token");
                    ui.text_edit_singleline(&mut self.settings.token)
                        .labelled_by(label.id);
                });
                ui.add_space(8.0);

                ui.vertical(|ui| {
                    let label = ui.label("E2EE Key");
                    ui.add_enabled_ui(self.settings.enable_e2ee, |ui| {
                        ui.text_edit_singleline(&mut self.settings.key)
                            .labelled_by(label.id)
                    });
                });
                ui.add_space(8.0);

                ui.checkbox(&mut self.settings.enable_e2ee, "Enable E2EE");
                ui.checkbox(&mut self.settings.auto_subscribe, "Auto Subscribe");

                ui.add_space(8.0);
            });

        request
    }
}
