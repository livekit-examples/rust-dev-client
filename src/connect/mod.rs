use crate::ui::{labeled_field::LabeledTextEdit, prominent_button::ProminentButton};
use livekit::e2ee::{
    E2eeOptions, EncryptionType,
    key_provider::{KeyProvider, KeyProviderOptions},
};
use livekit_token_source::{
    TokenSourceConfigurable, TokenSourceFetchOptions, TokenSourceFixed, development_token_server,
};
use std::time::Duration;

mod agent;

pub use agent::{AgentConnectSettings, LocalTokenSource};

/// How a room or agent connection is authenticated. The methods that target a
/// known server carry its URL; the token-source method learns it from the
/// development token server.
#[derive(Clone, Debug)]
pub enum Auth {
    /// A pre-generated access token; the room, and any agent dispatch, are
    /// encoded in it.
    Token { url: String, token: String },
    /// API credentials from which a join token is minted on demand,
    /// dispatching the source's agent if the room is new.
    ApiKey(LocalTokenSource),
    /// A LiveKit Cloud development token server, which provides both the server
    /// URL and the join token. `options` customizes the request; unset
    /// fields are left to server defaults.
    DevelopmentTokenServer {
        token_server_id: String,
        options: TokenSourceFetchOptions,
    },
}

impl Auth {
    /// Resolve to `(server_url, token)`: the JWT is minted locally for the
    /// API-key method, fetched over HTTP for the token-source method.
    ///
    /// Async because of that fetch — call it from the service task, not the UI
    /// thread.
    pub async fn connection_details(&self) -> Result<(String, String), String> {
        let response = match self {
            Auth::Token { url, token } => return Ok((url.clone(), token.clone())),
            Auth::ApiKey(source) => source.fetch().await,
            Auth::DevelopmentTokenServer {
                token_server_id,
                options,
            } => {
                development_token_server(token_server_id)
                    .fetch(options)
                    .await
            }
        };
        response
            .map(|response| (response.server_url, response.participant_token))
            .map_err(|e| e.to_string())
    }

    /// Short label of the connection target for window titles: the agent being
    /// dispatched, else the server URL when known up front, else the token
    /// server id.
    pub fn target_label(&self) -> &str {
        match self {
            Auth::Token { url, .. } => url,
            Auth::ApiKey(source) if !source.agent_name.is_empty() => &source.agent_name,
            Auth::ApiKey(source) => &source.url,
            Auth::DevelopmentTokenServer {
                token_server_id, ..
            } => token_server_id,
        }
    }

    /// Whether reconnecting rejoins the same room, where an agent may not be
    /// dispatched again. An agent session never asks a token server for a
    /// room name (see `TokenSourceOptions::for_agent`).
    pub fn reuses_room(&self) -> bool {
        match self {
            Auth::Token { .. } => true,
            Auth::ApiKey(source) => source.room.is_some(),
            Auth::DevelopmentTokenServer { .. } => false,
        }
    }
}

/// Settings for connecting to a room, entered on the connect screen; one copy
/// per application, passed to each room window as it is opened.
#[derive(Clone)]
pub struct ConnectSettings {
    pub auth: Auth,
    /// The shared E2EE key, if encryption is enabled.
    pub e2ee_key: Option<String>,
    pub auto_subscribe: bool,
    pub dynacast: bool,
}

/// Room encryption with the shared `key`, or `None` without one.
pub fn e2ee_options(key: Option<&str>) -> Option<E2eeOptions> {
    let key = key?;
    Some(E2eeOptions {
        encryption_type: EncryptionType::Gcm,
        key_provider: KeyProvider::with_shared_key(
            KeyProviderOptions::default(),
            key.as_bytes().to_vec(),
        ),
    })
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

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
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
            ..self.clone()
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
    fn is_connect_enabled(&self) -> bool {
        let filled = |s: &str| !s.trim().is_empty();
        // The URL only matters for the methods that use it; the token-source
        // method gets its server URL from the development token server response.
        // Agent sessions never name a room.
        match self.method {
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

    fn local_token_source(&self) -> LocalTokenSource {
        let options = &self.token_source_options;
        LocalTokenSource {
            url: self.url.clone(),
            api_key: self.api_key.clone(),
            api_secret: self.api_secret.clone(),
            identity: self.identity.clone(),
            // An agent session gets a new room each time; see `LocalTokenSource::room`.
            room: (self.mode == ConnectMode::Room).then(|| self.room.clone()),
            agent_name: options.agent_name.trim().to_string(),
            agent_metadata: options.agent_metadata.trim().to_string(),
        }
    }

    /// The selected method's credentials, for the selected mode.
    fn auth(&self) -> Auth {
        match self.method {
            AuthMethod::ApiKey => Auth::ApiKey(self.local_token_source()),
            AuthMethod::Token => Auth::Token {
                url: self.url.clone(),
                token: self.token.clone(),
            },
            AuthMethod::DevelopmentTokenServer => Auth::DevelopmentTokenServer {
                token_server_id: self.token_server_id.clone(),
                options: match self.mode {
                    ConnectMode::Room => TokenSourceFetchOptions::from(&self.token_source_options),
                    ConnectMode::Agent => self.token_source_options.for_agent(),
                },
            },
        }
    }

    fn e2ee_key(&self) -> Option<String> {
        self.enable_e2ee.then(|| self.key.clone())
    }

    fn room_settings(&self) -> ConnectSettings {
        ConnectSettings {
            auth: self.auth(),
            e2ee_key: self.e2ee_key(),
            auto_subscribe: self.auto_subscribe,
            dynacast: self.dynacast,
        }
    }

    fn agent_settings(&self) -> AgentConnectSettings {
        AgentConnectSettings {
            auth: self.auth(),
            e2ee_key: self.e2ee_key(),
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

            // The method is shared between the modes, so switching keeps the tab.
            let mode = view.mode;
            let method = &mut view.method;
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
            ui.push_id("auth_method_fields", |ui| match view.method {
                AuthMethod::Token => {
                    token_fields(ui, view, mask);
                }
                AuthMethod::ApiKey => {
                    api_key_fields(ui, view, mask, mode);
                }
                AuthMethod::DevelopmentTokenServer => {
                    token_server_fields(ui, view, mode);
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

/// The API credentials, identity, room, and the agent to dispatch.
fn api_key_fields(ui: &mut egui::Ui, view: &mut ConnectView, mask: bool, mode: ConnectMode) {
    ui.add(LabeledTextEdit::singleline("URL", &mut view.url));
    ui.add_space(8.0);
    ui.columns(2, |columns| {
        columns[0].add(LabeledTextEdit::singleline("API Key", &mut view.api_key).password(mask));
        columns[1]
            .add(LabeledTextEdit::singleline("API Secret", &mut view.api_secret).password(mask));
    });
    ui.add_space(8.0);
    ui.columns(2, |columns| {
        columns[0].add(LabeledTextEdit::singleline("Identity", &mut view.identity));
        columns[1]
            .add(
                LabeledTextEdit::singleline("Room", &mut view.room)
                    .enabled(mode == ConnectMode::Room),
            )
            .on_disabled_hover_text(NEW_ROOM_HINT);
    });
    ui.add_space(8.0);
    let options = &mut view.token_source_options;
    ui.columns(2, |columns| {
        columns[0]
            .add(LabeledTextEdit::singleline(
                "Agent Name",
                &mut options.agent_name,
            ))
            .on_hover_text(
                "Dispatched when the room is created, for room joins as well as agent \
                 sessions; leave empty to rely on automatic agent dispatch",
            );
        columns[1].add(LabeledTextEdit::singleline(
            "Agent Metadata",
            &mut options.agent_metadata,
        ));
    });
    ui.add_space(8.0);
}

/// Why the room name fields are disabled in agent mode. They stay in place to
/// keep the layout from shifting between the modes.
const NEW_ROOM_HINT: &str =
    "Agent sessions always use a new room, so the agent is dispatched into it";

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
    ui.columns(2, |columns| {
        columns[0]
            .add(
                LabeledTextEdit::singleline("Room Name", &mut options.room_name)
                    .enabled(mode == ConnectMode::Room),
            )
            .on_disabled_hover_text(NEW_ROOM_HINT);
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
