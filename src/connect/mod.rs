use crate::ui::{labeled_field::LabeledTextEdit, prominent_button::ProminentButton};
use livekit_token_source::{
    TokenSourceConfigurable, TokenSourceFetchOptions, development_token_server,
};
use std::time::Duration;

mod agent;

pub use agent::{AgentAuth, AgentConnectSettings, LocalTokenSource};

/// How a room connection is authenticated. The methods that target a known
/// server carry its URL; the token-source method learns it from the development token server.
#[derive(Clone, Debug)]
pub enum Auth {
    /// A pre-generated access token (the room is encoded in it).
    Token { url: String, token: String },
    /// API credentials from which a join token is generated on demand.
    ApiKey {
        url: String,
        api_key: String,
        api_secret: String,
        identity: String,
        room: String,
    },
    /// A LiveKit Cloud development token server, which provides both the server
    /// URL and the join token. `options` customizes the request; unset
    /// fields are left to server defaults.
    DevelopmentTokenServer {
        token_server_id: String,
        options: TokenSourceFetchOptions,
    },
}

impl Auth {
    /// Resolve to `(server_url, token)`: the JWT is generated locally for the
    /// API-key method, fetched over HTTP for the token-source method.
    ///
    /// Async because of that fetch — call it from the service task, not the UI
    /// thread.
    pub async fn connection_details(&self) -> Result<(String, String), String> {
        match self {
            Auth::Token { url, token } => Ok((url.clone(), token.clone())),
            Auth::ApiKey {
                url,
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
                .map(|token| (url.clone(), token))
                .map_err(|e| e.to_string()),
            Auth::DevelopmentTokenServer {
                token_server_id,
                options,
            } => {
                let token_source = development_token_server(token_server_id);
                let response = token_source
                    .fetch(options)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok((response.server_url, response.participant_token))
            }
        }
    }

    /// Short label of the connection target for window titles: the server URL
    /// when known up front, otherwise the token server id.
    pub fn target_label(&self) -> &str {
        match self {
            Auth::Token { url, .. } | Auth::ApiKey { url, .. } => url,
            Auth::DevelopmentTokenServer {
                token_server_id, ..
            } => token_server_id,
        }
    }
}

/// Settings for connecting to a room, entered on the connect screen; one copy
/// per application, passed to each room window as it is opened.
#[derive(Clone)]
pub struct ConnectSettings {
    pub auth: Auth,
    pub key: String,
    pub auto_subscribe: bool,
    pub dynacast: bool,
    pub enable_e2ee: bool,
}

/// What a Connect click asks the app to open.
pub enum ConnectRequest {
    Room(ConnectSettings),
    Agent(AgentConnectSettings),
}

/// Whether the connect form joins a room directly or starts an agent session.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
enum ConnectMode {
    #[default]
    Room,
    Agent,
}

/// Which authentication method the connect form is editing.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
enum AuthMethod {
    #[default]
    ApiKey,
    Token,
    DevelopmentTokenServer,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct TokenSourceOptions {
    room_name: String,
    participant_name: String,
    participant_identity: String,
    participant_metadata: String,
    agent_name: String,
    agent_metadata: String,
    agent_deployment: String,
}

impl TokenSourceOptions {
    /// The options for an agent session: as for a room, but never naming the
    /// room, so the agent is dispatched into a new one.
    fn for_agent(&self) -> TokenSourceFetchOptions {
        TokenSourceFetchOptions::from(&TokenSourceOptions {
            room_name: String::new(),
            participant_name: self.participant_name.clone(),
            participant_identity: self.participant_identity.clone(),
            participant_metadata: self.participant_metadata.clone(),
            agent_name: self.agent_name.clone(),
            agent_metadata: self.agent_metadata.clone(),
            agent_deployment: self.agent_deployment.clone(),
        })
    }
}

impl From<&TokenSourceOptions> for TokenSourceFetchOptions {
    fn from(value: &TokenSourceOptions) -> Self {
        // Empty (or whitespace-only) fields are left unset so the
        // token server applies its defaults.
        let opt = |s: &str| {
            let s = s.trim();
            (!s.is_empty()).then(|| s.to_string())
        };
        let mut options = TokenSourceFetchOptions::new();
        if let Some(v) = opt(&value.room_name) {
            options = options.with_room_name(v);
        }
        if let Some(v) = opt(&value.participant_name) {
            options = options.with_participant_name(v);
        }
        if let Some(v) = opt(&value.participant_identity) {
            options = options.with_participant_identity(v);
        }
        if let Some(v) = opt(&value.participant_metadata) {
            options = options.with_participant_metadata(v);
        }
        if let Some(v) = opt(&value.agent_name) {
            options = options.with_agent_name(v);
        }
        if let Some(v) = opt(&value.agent_metadata) {
            options = options.with_agent_metadata(v);
        }
        if let Some(v) = opt(&value.agent_deployment) {
            options = options.with_deployment(v);
        }
        options
    }
}

/// The root window: a welcome screen holding the only connect form in the app.
/// Each successful Connect click spawns a dedicated room or agent window. Inputs
/// for every auth method are kept, and shared between the room and agent modes,
/// so switching tabs doesn't lose what was typed.
///
/// Persisted between runs via eframe storage (see `AppRoot`); `#[serde(default)]`
/// lets a stored blob from an older version fall back to `Default` (env-seeded)
/// for any missing field.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ConnectView {
    mode: ConnectMode,
    method: AuthMethod,
    agent_method: AuthMethod,
    url: String,
    token: String,
    token_server_id: String,
    token_source_options: TokenSourceOptions,
    api_key: String,
    api_secret: String,
    identity: String,
    room: String,
    key: String,
    auto_subscribe: bool,
    dynacast: bool,
    enable_e2ee: bool,
    voice: bool,
    agent_connect_timeout_secs: u64,
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
            mode: ConnectMode::default(),
            method: AuthMethod::default(),
            agent_method: AuthMethod::DevelopmentTokenServer,
            url: env_or("LIVEKIT_URL", "ws://localhost:7880"),
            token: env_or("LIVEKIT_TOKEN", ""),
            token_server_id: String::new(),
            token_source_options: TokenSourceOptions::default(),
            api_key: env_or("LIVEKIT_API_KEY", "devkey"),
            api_secret: env_or("LIVEKIT_API_SECRET", "secret"),
            identity: "participant-0".to_string(),
            room: "dev-room".to_string(),
            key: String::new(),
            auto_subscribe: true,
            dynacast: true,
            enable_e2ee: false,
            voice: true,
            agent_connect_timeout_secs: 20,
            show_secrets: false,
        }
    }
}

impl ConnectView {
    /// The auth method of the active mode.
    fn active_method(&self) -> AuthMethod {
        match self.mode {
            ConnectMode::Room => self.method,
            ConnectMode::Agent => self.agent_method,
        }
    }

    fn is_connect_enabled(&self) -> bool {
        let filled = |s: &str| !s.trim().is_empty();
        // The URL only matters for the methods that use it; the token-source
        // method gets its server URL from the development token server response.
        // Agent sessions never name a room.
        match self.active_method() {
            AuthMethod::ApiKey => {
                filled(&self.url)
                    && filled(&self.api_key)
                    && filled(&self.api_secret)
                    && filled(&self.identity)
                    && (self.mode == ConnectMode::Agent || filled(&self.room))
            }
            AuthMethod::Token => filled(&self.url) && filled(&self.token),
            AuthMethod::DevelopmentTokenServer => filled(&self.token_server_id),
        }
    }

    fn current_request(&self) -> ConnectRequest {
        match self.mode {
            ConnectMode::Room => ConnectRequest::Room(self.room_settings()),
            ConnectMode::Agent => ConnectRequest::Agent(self.agent_settings()),
        }
    }

    fn room_settings(&self) -> ConnectSettings {
        let auth = match self.method {
            AuthMethod::ApiKey => Auth::ApiKey {
                url: self.url.clone(),
                api_key: self.api_key.clone(),
                api_secret: self.api_secret.clone(),
                identity: self.identity.clone(),
                room: self.room.clone(),
            },
            AuthMethod::Token => Auth::Token {
                url: self.url.clone(),
                token: self.token.clone(),
            },
            AuthMethod::DevelopmentTokenServer => {
                let options = TokenSourceFetchOptions::from(&self.token_source_options);
                Auth::DevelopmentTokenServer {
                    token_server_id: self.token_server_id.clone(),
                    options,
                }
            }
        };
        ConnectSettings {
            auth,
            key: self.key.clone(),
            auto_subscribe: self.auto_subscribe,
            dynacast: self.dynacast,
            enable_e2ee: self.enable_e2ee,
        }
    }

    fn agent_settings(&self) -> AgentConnectSettings {
        let auth = match self.agent_method {
            AuthMethod::ApiKey => AgentAuth::ApiKey(LocalTokenSource {
                url: self.url.clone(),
                api_key: self.api_key.clone(),
                api_secret: self.api_secret.clone(),
                identity: self.identity.clone(),
                agent_name: self.token_source_options.agent_name.trim().to_string(),
                agent_metadata: self.token_source_options.agent_metadata.clone(),
            }),
            AuthMethod::Token => AgentAuth::Literal {
                url: self.url.clone(),
                token: self.token.clone(),
            },
            AuthMethod::DevelopmentTokenServer => AgentAuth::DevelopmentTokenServer {
                token_server_id: self.token_server_id.clone(),
                options: self.token_source_options.for_agent(),
            },
        };
        AgentConnectSettings {
            auth,
            key: self.key.clone(),
            enable_e2ee: self.enable_e2ee,
            voice: self.voice,
            agent_connect_timeout: Duration::from_secs(self.agent_connect_timeout_secs),
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

    /// Returns what to open when Connect is clicked.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<ConnectRequest> {
        let mut request = None;

        let frame = egui::Frame::central_panel(ui.style()).inner_margin(16.0);

        egui::Panel::bottom("connect_info").show(ui, |ui| {
            ui.horizontal(|ui| {
                let version_string = format!("SDK Version: {}", livekit::SDK_VERSION);
                let text = egui::RichText::new(version_string).text_style(egui::TextStyle::Small);
                ui.label(text);
                egui::Aligned::new(egui::Align2::RIGHT_CENTER)
                    .id_salt("debug_warning")
                    .show(ui, |ui| {
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
                    request = Some(self.current_request());
                    // Agent sessions get a new room each time, so only a room
                    // join needs a fresh identity.
                    if self.mode == ConnectMode::Room && self.method == AuthMethod::ApiKey {
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

/// The connect form: a room / agent mode selector, an auth-method tab selector,
/// the active method's fields, and the mode's options. The Connect button lives
/// in the connect screen's bottom panel, so this widget only edits the borrowed view.
struct ConnectForm<'a> {
    view: &'a mut ConnectView,
}

impl egui::Widget for ConnectForm<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let view = self.view;
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                for (mode, label) in [
                    (ConnectMode::Room, "Connect to a Room"),
                    (ConnectMode::Agent, "Connect to an Agent"),
                ] {
                    let text = egui::RichText::new(label).text_style(egui::TextStyle::Heading);
                    ui.selectable_value(&mut view.mode, mode, text);
                }
            });
            ui.add_space(8.0);

            let mode = view.mode;
            let method = match mode {
                ConnectMode::Room => &mut view.method,
                ConnectMode::Agent => &mut view.agent_method,
            };
            ui.horizontal(|ui| {
                ui.selectable_value(method, AuthMethod::ApiKey, "API Key")
                    .on_hover_text(match mode {
                        ConnectMode::Room => "Generate a join token from API credentials",
                        ConnectMode::Agent => {
                            "Generate a token dispatching the agent into a new room"
                        }
                    });
                ui.selectable_value(method, AuthMethod::Token, "Token")
                    .on_hover_text(match mode {
                        ConnectMode::Room => "Join with a pre-generated token",
                        ConnectMode::Agent => {
                            "Join with a pre-generated token that dispatches the agent"
                        }
                    });
                ui.selectable_value(method, AuthMethod::DevelopmentTokenServer, "TokenSource")
                    .on_hover_text("Fetch credentials from a development token server");
                egui::Aligned::new(egui::Align2::RIGHT_CENTER)
                    .id_salt("secrets_toggle")
                    .show(ui, |ui| {
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
            // The URL lives inside the Token / API Key tabs (shared between
            // them): the token-source method gets its URL from the development token server.
            ui.push_id("auth_method_fields", |ui| {
                match (mode, view.active_method()) {
                    (_, AuthMethod::Token) => {
                        token_fields(ui, view, mask);
                    }
                    (ConnectMode::Room, AuthMethod::ApiKey) => {
                        api_key_fields(ui, view, mask);
                        ui.columns(2, |columns| {
                            columns[0]
                                .add(LabeledTextEdit::singleline("Identity", &mut view.identity));
                            columns[1].add(LabeledTextEdit::singleline("Room", &mut view.room));
                        });
                        ui.add_space(8.0);
                    }
                    (ConnectMode::Agent, AuthMethod::ApiKey) => {
                        api_key_fields(ui, view, mask);
                        let options = &mut view.token_source_options;
                        ui.columns(2, |columns| {
                            columns[0]
                                .add(LabeledTextEdit::singleline("Identity", &mut view.identity));
                            columns[1]
                                .add(LabeledTextEdit::singleline(
                                    "Agent Name",
                                    &mut options.agent_name,
                                ))
                                .on_hover_text("Leave empty to rely on automatic agent dispatch");
                        });
                        ui.add_space(8.0);
                        ui.add(LabeledTextEdit::singleline(
                            "Agent Metadata",
                            &mut options.agent_metadata,
                        ));
                        ui.add_space(8.0);
                    }
                    (mode, AuthMethod::DevelopmentTokenServer) => {
                        token_server_fields(ui, view, mode);
                    }
                }
            });

            ui.add(
                LabeledTextEdit::singleline("E2EE Key", &mut view.key)
                    .enabled(view.enable_e2ee)
                    .password(mask),
            );
            ui.add_space(8.0);

            ui.checkbox(&mut view.enable_e2ee, "Enable E2EE");
            match mode {
                ConnectMode::Room => {
                    ui.checkbox(&mut view.auto_subscribe, "Auto Subscribe");
                    ui.checkbox(&mut view.dynacast, "Dynacast");
                }
                ConnectMode::Agent => {
                    ui.checkbox(&mut view.voice, "Voice")
                        .on_hover_text("Publish the microphone as soon as the session starts");
                    ui.horizontal(|ui| {
                        ui.label("Agent connect timeout");
                        ui.add(
                            egui::DragValue::new(&mut view.agent_connect_timeout_secs)
                                .range(1..=300)
                                .suffix(" s"),
                        );
                    })
                    .response
                    .on_hover_text(
                        "How long a dispatched agent gets to become conversational before \
                         the session reports it failed",
                    );
                }
            }

            ui.add_space(8.0);
        })
        .response
    }
}

fn token_fields(ui: &mut egui::Ui, view: &mut ConnectView, mask: bool) {
    ui.add(LabeledTextEdit::singleline("URL", &mut view.url));
    ui.add_space(8.0);
    ui.add(LabeledTextEdit::singleline("Token", &mut view.token).password(mask));
    ui.add_space(8.0);
}

fn api_key_fields(ui: &mut egui::Ui, view: &mut ConnectView, mask: bool) {
    ui.add(LabeledTextEdit::singleline("URL", &mut view.url));
    ui.add_space(8.0);
    ui.columns(2, |columns| {
        columns[0].add(LabeledTextEdit::singleline("API Key", &mut view.api_key).password(mask));
        columns[1]
            .add(LabeledTextEdit::singleline("API Secret", &mut view.api_secret).password(mask));
    });
    ui.add_space(8.0);
}

/// The development token server id and its optional overrides. Agent sessions
/// leave out the room name, so each one is dispatched into a new room.
fn token_server_fields(ui: &mut egui::Ui, view: &mut ConnectView, mode: ConnectMode) {
    ui.add(LabeledTextEdit::singleline(
        "Token Server Id",
        &mut view.token_server_id,
    ));
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("Optional overrides — empty fields use server defaults")
            .text_style(egui::TextStyle::Small),
    );
    ui.add_space(8.0);
    let options = &mut view.token_source_options;
    match mode {
        ConnectMode::Room => {
            ui.columns(2, |columns| {
                columns[0].add(LabeledTextEdit::singleline(
                    "Room Name",
                    &mut options.room_name,
                ));
                columns[1].add(LabeledTextEdit::singleline(
                    "Participant Name",
                    &mut options.participant_name,
                ));
            });
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                columns[0].add(LabeledTextEdit::singleline(
                    "Participant Identity",
                    &mut options.participant_identity,
                ));
                columns[1].add(LabeledTextEdit::singleline(
                    "Participant Metadata",
                    &mut options.participant_metadata,
                ));
            });
        }
        ConnectMode::Agent => {
            ui.columns(2, |columns| {
                columns[0].add(LabeledTextEdit::singleline(
                    "Participant Name",
                    &mut options.participant_name,
                ));
                columns[1].add(LabeledTextEdit::singleline(
                    "Participant Identity",
                    &mut options.participant_identity,
                ));
            });
            ui.add_space(8.0);
            ui.add(LabeledTextEdit::singleline(
                "Participant Metadata",
                &mut options.participant_metadata,
            ));
        }
    }
    ui.add_space(8.0);
    ui.columns(2, |columns| {
        columns[0].add(LabeledTextEdit::singleline(
            "Agent Name",
            &mut options.agent_name,
        ));
        columns[1].add(LabeledTextEdit::singleline(
            "Agent Deployment",
            &mut options.agent_deployment,
        ));
    });
    ui.add_space(8.0);
    ui.add(LabeledTextEdit::singleline(
        "Agent Metadata",
        &mut options.agent_metadata,
    ));
    ui.add_space(8.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_options_never_name_the_room() {
        let options = TokenSourceOptions {
            room_name: "dev-room".to_string(),
            agent_name: "my-agent".to_string(),
            ..Default::default()
        };
        let expected = TokenSourceFetchOptions::new().with_agent_name("my-agent");
        assert_eq!(options.for_agent(), expected);
    }
}
