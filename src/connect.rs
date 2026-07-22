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
    TokenSource{sandbox_id: String}
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
            Auth::TokenSource{sandbox_id} => Ok("".to_string()),
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
    pub dynacast: bool,
    pub enable_e2ee: bool,
}

/// Which authentication method the connect form is editing.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
enum AuthMethod {
    #[default]
    ApiKey,
    Token,
    TokenSource
}

/// The root window: a welcome screen holding the only connect form in the app.
/// Each successful Connect click spawns a dedicated room window. Inputs for both
/// auth methods are kept so switching tabs doesn't lose what was typed.
///
/// Persisted between runs via eframe storage (see `AppRoot`); `#[serde(default)]`
/// lets a stored blob from an older version fall back to `Default` (env-seeded)
/// for any missing field.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ConnectView {
    method: AuthMethod,
    url: String,
    token: String,
    sandbox_id: String,
    api_key: String,
    api_secret: String,
    identity: String,
    room: String,
    key: String,
    auto_subscribe: bool,
    dynacast: bool,
    enable_e2ee: bool,
    // Not persisted: secrets always start hidden on launch.
    #[serde(skip)]
    show_secrets: bool,
}

impl Default for ConnectView {
    fn default() -> Self {
        // Seed the fields from the standard LiveKit env vars when present, so a
        // configured shell or `.env` pre-fills the form instead of the defaults.
        let env_or = |key: &str, default: &str| {
            std::env::var(key)
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        Self {
            method: AuthMethod::default(),
            url: env_or("LIVEKIT_URL", "ws://localhost:7880"),
            token: env_or("LIVEKIT_TOKEN", ""),
            sandbox_id: "sandbox-id".to_string(),
            api_key: env_or("LIVEKIT_API_KEY", "devkey"),
            api_secret: env_or("LIVEKIT_API_SECRET", "secret"),
            identity: "participant-0".to_string(),
            room: "dev-room".to_string(),
            key: String::new(),
            auto_subscribe: true,
            dynacast: true,
            enable_e2ee: false,
            show_secrets: false,
        }
    }
}

impl ConnectView {
    fn is_connect_enabled(&self) -> bool {
        if self.url.trim().is_empty() {
            return false;
        }
        match self.method {
            AuthMethod::ApiKey => {
                !self.api_key.trim().is_empty()
                    && !self.api_secret.trim().is_empty()
                    && !self.identity.trim().is_empty()
                    && !self.room.trim().is_empty()
            }
            AuthMethod::Token => !self.token.trim().is_empty(),
            AuthMethod::TokenSource => !self.sandbox_id.trim().is_empty()
        }
    }

    fn current_settings(&self) -> ConnectSettings {
        let auth = match self.method {
            AuthMethod::ApiKey => Auth::ApiKey {
                api_key: self.api_key.clone(),
                api_secret: self.api_secret.clone(),
                identity: self.identity.clone(),
                room: self.room.clone(),
            },
            AuthMethod::Token => Auth::Token(self.token.clone()),
            AuthMethod::TokenSource => Auth::TokenSource{sandbox_id: self.sandbox_id.clone()},
        };
        ConnectSettings {
            url: self.url.clone(),
            auth,
            key: self.key.clone(),
            auto_subscribe: self.auto_subscribe,
            dynacast: self.dynacast,
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

        egui::Panel::bottom("connect_info").show(ui, |ui| {
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
            .show(ui, |ui| {
                let connect =
                    ui.add(ProminentButton::new("Connect").enabled(self.is_connect_enabled()));
                if connect.clicked() {
                    request = Some(self.current_settings());
                    if self.method == AuthMethod::ApiKey {
                        self.bump_identity();
                    }
                }
            });

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
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
                ui.selectable_value(&mut view.method, AuthMethod::ApiKey, "API Key");
                ui.selectable_value(&mut view.method, AuthMethod::Token, "Token");
                ui.selectable_value(&mut view.method, AuthMethod::TokenSource, "TokenSource");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let toggle = ui
                        .add(egui::Button::selectable(view.show_secrets, "👁"))
                        .on_hover_text("Show/hide secrets");
                    if toggle.clicked() {
                        view.show_secrets = !view.show_secrets;
                    }
                });
            });
            ui.add_space(8.0);

            // Mask the secret fields (token / API key / secret / E2EE key) unless
            // the eye toggle above is on.
            let mask = !view.show_secrets;

            // Scope the method's fields under a *constant* id. The single-field
            // tabs (Token / TokenSource) render the same full-width widget at the
            // same rect, so a per-method salt would give that rect a different id
            // each switch — which is exactly what egui's "rect changed id between
            // passes" warning flags. A stable salt keeps the id constant.
            ui.push_id("auth_method_fields", |ui| match view.method {
                AuthMethod::Token => {
                    ui.add(LabeledTextEdit::singleline("Token", &mut view.token).password(mask));
                    ui.add_space(8.0);
                }
                AuthMethod::ApiKey => {
                    ui.columns(2, |columns| {
                        columns[0].add(
                            LabeledTextEdit::singleline("API Key", &mut view.api_key)
                                .password(mask),
                        );
                        columns[1].add(
                            LabeledTextEdit::singleline("API Secret", &mut view.api_secret)
                                .password(mask),
                        );
                    });
                    ui.add_space(8.0);
                    ui.columns(2, |columns| {
                        columns[0].add(LabeledTextEdit::singleline("Identity", &mut view.identity));
                        columns[1].add(LabeledTextEdit::singleline("Room", &mut view.room));
                    });
                    ui.add_space(8.0);
                },
                AuthMethod::TokenSource => {
                    ui.add(LabeledTextEdit::singleline("Sandbox Id", &mut view.sandbox_id));
                    ui.add_space(8.0);
                }
            });

            ui.add(
                LabeledTextEdit::singleline("E2EE Key", &mut view.key)
                    .enabled(view.enable_e2ee)
                    .password(mask),
            );
            ui.add_space(8.0);

            ui.checkbox(&mut view.enable_e2ee, "Enable E2EE");
            ui.checkbox(&mut view.auto_subscribe, "Auto Subscribe");
            ui.checkbox(&mut view.dynacast, "Dynacast");

            ui.add_space(8.0);
        })
        .response
    }
}
