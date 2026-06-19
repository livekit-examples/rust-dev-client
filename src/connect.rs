use crate::ui::{labeled_field::LabeledTextEdit, prominent_button::ProminentButton};

/// Settings for connecting to a room, entered on the connect screen; one copy
/// per application, passed to each room window as it is opened.
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

/// The root window: a welcome screen holding the only connect form in the app.
/// Each successful Connect click spawns a dedicated room window.
#[derive(Default)]
pub struct ConnectView {
    pub settings: ConnectSettings,
}

impl ConnectView {
    fn is_connect_enabled(&self) -> bool {
        !self.settings.url.trim().is_empty() && !self.settings.token.trim().is_empty()
    }

    /// Returns the settings to open a room with when Connect is clicked.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<ConnectSettings> {
        let mut request = None;

        let frame = egui::Frame::central_panel(ui.style()).inner_margin(16.0);

        egui::Panel::bottom("connect_info").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let version_string = format!("SDK Version: {}", livekit::SDK_VERSION);
                let text = egui::RichText::new(version_string).text_style(egui::TextStyle::Small);
                ui.label(text);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::warn_if_debug_build(ui);
                });
            });
        });

        egui::Panel::bottom("connect_action")
            .frame(frame)
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                let connect =
                    ui.add(ProminentButton::new("Connect").enabled(self.is_connect_enabled()));
                if connect.clicked() {
                    request = Some(self.settings.clone());
                }
            });

        egui::CentralPanel::default()
            .frame(frame)
            .show_inside(ui, |ui| {
                ui.add(ConnectForm {
                    settings: &mut self.settings,
                });
            });

        request
    }
}

/// The connect form fields (URL / token / E2EE / options). The Connect button
/// lives in the connect screen's bottom panel, so this widget only edits the
/// borrowed [`ConnectSettings`].
struct ConnectForm<'a> {
    settings: &'a mut ConnectSettings,
}

impl egui::Widget for ConnectForm<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let settings = self.settings;
        ui.vertical(|ui| {
            let connect_string =
                egui::RichText::new("Connect to a Room").text_style(egui::TextStyle::Heading);

            ui.label(connect_string);
            ui.add_space(8.0);

            ui.add(LabeledTextEdit::singleline("URL", &mut settings.url));
            ui.add_space(8.0);

            ui.add(LabeledTextEdit::singleline("Token", &mut settings.token));
            ui.add_space(8.0);

            ui.add(
                LabeledTextEdit::singleline("E2EE Key", &mut settings.key)
                    .enabled(settings.enable_e2ee),
            );
            ui.add_space(8.0);

            ui.checkbox(&mut settings.enable_e2ee, "Enable E2EE");
            ui.checkbox(&mut settings.auto_subscribe, "Auto Subscribe");

            ui.add_space(8.0);
        })
        .response
    }
}
