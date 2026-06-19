use crate::ui::{labeled_field::LabeledTextEdit, prominent_button::ProminentButton};

/// How a room connection is authenticated.
#[derive(Clone)]
pub enum Auth {
    /// A pre-generated access token (the room is encoded in it).
    Token(String),
    /// API credentials from which a join token is generated on demand.
    ApiKey {
        api_key: String,
        api_secret: String,
        identity: String,
        room: String,
    },
}

impl Auth {
    /// Resolve to a room-connection JWT, generating one from the API credentials
    /// when this is the API-key method.
    pub fn access_token(&self) -> Result<String, String> {
        match self {
            Auth::Token(token) => Ok(token.clone()),
            Auth::ApiKey {
                api_key,
                api_secret,
                identity,
                room,
            } => livekit_api::access_token::AccessToken::with_api_key(api_key, api_secret)
                .with_identity(identity)
                .with_name(identity)
                .with_grants(livekit_api::access_token::VideoGrants {
                    room_join: true,
                    room: room.clone(),
                    ..Default::default()
                })
                .to_jwt()
                .map_err(|e| e.to_string()),
        }
    }
}

/// Settings for connecting to a room, entered on the connect screen; one copy
/// per application, passed to each room window as it is opened.
#[derive(Clone)]
pub struct ConnectSettings {
    pub url: String,
    pub auth: Auth,
    pub key: String,
    pub auto_subscribe: bool,
    pub enable_e2ee: bool,
}

/// Which authentication method the connect form is editing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
enum AuthMethod {
    #[default]
    Token,
    ApiKey,
}

/// The root window: a welcome screen holding the only connect form in the app.
/// Each successful Connect click spawns a dedicated room window. Inputs for both
/// auth methods are kept so switching tabs doesn't lose what was typed.
pub struct ConnectView {
    method: AuthMethod,
    url: String,
    token: String,
    api_key: String,
    api_secret: String,
    identity: String,
    room: String,
    key: String,
    auto_subscribe: bool,
    enable_e2ee: bool,
}

impl Default for ConnectView {
    fn default() -> Self {
        Self {
            method: AuthMethod::default(),
            url: "ws://localhost:7880".to_string(),
            token: String::new(),
            api_key: "devkey".to_string(),
            api_secret: "secret".to_string(),
            identity: "participant-0".to_string(),
            room: "dev-room".to_string(),
            key: String::new(),
            auto_subscribe: true,
            enable_e2ee: false,
        }
    }
}

impl ConnectView {
    fn is_connect_enabled(&self) -> bool {
        if self.url.trim().is_empty() {
            return false;
        }
        match self.method {
            AuthMethod::Token => !self.token.trim().is_empty(),
            AuthMethod::ApiKey => {
                !self.api_key.trim().is_empty()
                    && !self.api_secret.trim().is_empty()
                    && !self.identity.trim().is_empty()
                    && !self.room.trim().is_empty()
            }
        }
    }

    fn current_settings(&self) -> ConnectSettings {
        let auth = match self.method {
            AuthMethod::Token => Auth::Token(self.token.clone()),
            AuthMethod::ApiKey => Auth::ApiKey {
                api_key: self.api_key.clone(),
                api_secret: self.api_secret.clone(),
                identity: self.identity.clone(),
                room: self.room.clone(),
            },
        };
        ConnectSettings {
            url: self.url.clone(),
            auth,
            key: self.key.clone(),
            auto_subscribe: self.auto_subscribe,
            enable_e2ee: self.enable_e2ee,
        }
    }

    /// If the identity ends in `-<number>` (e.g. `alice-0`), increment that
    ///
    /// Identities must be unique within a room, so bumping it after each
    /// Connect lets you spin up several participants in the same room with one click.
    ///
    fn bump_identity(&mut self) {
        let Some((base, suffix)) = self.identity.rsplit_once('-') else {
            return;
        };
        let Ok(n) = suffix.parse::<u64>() else {
            return;
        };
        self.identity = format!("{base}-{}", n.saturating_add(1));
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
                    request = Some(self.current_settings());
                    if self.method == AuthMethod::ApiKey {
                        self.bump_identity();
                    }
                }
            });

        egui::CentralPanel::default()
            .frame(frame)
            .show_inside(ui, |ui| {
                ui.add(ConnectForm { view: self });
            });

        request
    }
}

/// The connect form: URL, an auth-method tab selector, the active method's
/// fields, and the shared E2EE / options. The Connect button lives in the
/// connect screen's bottom panel, so this widget only edits the borrowed view.
struct ConnectForm<'a> {
    view: &'a mut ConnectView,
}

impl egui::Widget for ConnectForm<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let view = self.view;
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Connect to a Room").text_style(egui::TextStyle::Heading));
            ui.add_space(8.0);

            ui.add(LabeledTextEdit::singleline("URL", &mut view.url));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.selectable_value(&mut view.method, AuthMethod::Token, "Token");
                ui.selectable_value(&mut view.method, AuthMethod::ApiKey, "API Key");
            });
            ui.add_space(8.0);

            // Scope each method's fields under a distinct id so switching tabs
            // is seen as a layout change, not an unstable widget id (egui warns
            // when a rect's id changes between passes under the same parent).
            ui.push_id(view.method, |ui| match view.method {
                AuthMethod::Token => {
                    ui.add(LabeledTextEdit::singleline("Token", &mut view.token));
                    ui.add_space(8.0);
                }
                AuthMethod::ApiKey => {
                    ui.columns(2, |columns| {
                        columns[0].add(LabeledTextEdit::singleline("API Key", &mut view.api_key));
                        columns[1].add(LabeledTextEdit::singleline(
                            "API Secret",
                            &mut view.api_secret,
                        ));
                    });
                    ui.add_space(8.0);
                    ui.columns(2, |columns| {
                        columns[0].add(LabeledTextEdit::singleline("Identity", &mut view.identity));
                        columns[1].add(LabeledTextEdit::singleline("Room", &mut view.room));
                    });
                    ui.add_space(8.0);
                }
            });

            ui.add(
                LabeledTextEdit::singleline("E2EE Key", &mut view.key).enabled(view.enable_e2ee),
            );
            ui.add_space(8.0);

            ui.checkbox(&mut view.enable_e2ee, "Enable E2EE");
            ui.checkbox(&mut view.auto_subscribe, "Auto Subscribe");

            ui.add_space(8.0);
        })
        .response
    }
}
