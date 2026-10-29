use async_trait::async_trait;
use livekit_api::access_token::{AccessToken, VideoGrants};
use livekit_protocol::{RoomAgentDispatch, RoomConfiguration};
use livekit_session::SessionTokenSource;
use livekit_token_source::{
    TokenSourceError, TokenSourceFetchOptions, TokenSourceFixed, TokenSourceResponse,
    TokenSourceResult, development_token_server, literal,
};
use std::hash::{BuildHasher, Hasher, RandomState};
use std::time::Duration;

/// How an agent session gets its credentials. Unlike a room connection, no
/// method names the room: each session should land in a new one, because an
/// agent is dispatched when its room is created.
#[derive(Clone, Debug)]
pub enum AgentAuth {
    /// A pre-generated token; the agent dispatch must already be encoded in it.
    Literal { url: String, token: String },
    /// A LiveKit Cloud development token server, which dispatches the agent
    /// named in `options`.
    DevelopmentTokenServer {
        token_server_id: String,
        options: TokenSourceFetchOptions,
    },
    /// API credentials from which a token dispatching `agent_name` into a
    /// fresh room is minted on every fetch.
    ApiKey(LocalTokenSource),
}

impl AgentAuth {
    /// Short label of the connection target for window titles.
    pub fn target_label(&self) -> &str {
        match self {
            AgentAuth::Literal { url, .. } => url,
            AgentAuth::DevelopmentTokenServer {
                token_server_id, ..
            } => token_server_id,
            AgentAuth::ApiKey(source) if !source.agent_name.is_empty() => &source.agent_name,
            AgentAuth::ApiKey(source) => &source.url,
        }
    }

    /// Whether reconnecting rejoins the same room, where the agent may not be
    /// dispatched again.
    pub fn reuses_room(&self) -> bool {
        matches!(self, AgentAuth::Literal { .. })
    }
}

impl From<AgentAuth> for SessionTokenSource {
    fn from(auth: AgentAuth) -> Self {
        match auth {
            AgentAuth::Literal { url, token } => literal(url, token).into(),
            AgentAuth::DevelopmentTokenServer {
                token_server_id,
                options,
            } => (development_token_server(token_server_id), options).into(),
            AgentAuth::ApiKey(source) => SessionTokenSource::fixed(source),
        }
    }
}

/// Settings for starting an agent session, entered on the connect screen.
#[derive(Clone)]
pub struct AgentConnectSettings {
    pub auth: AgentAuth,
    pub key: String,
    pub enable_e2ee: bool,
    /// Publish the microphone as soon as the session starts.
    pub voice: bool,
    pub agent_connect_timeout: Duration,
}

/// Mints a token locally from API credentials. As a session's token source it
/// uses a new room per fetch, so every session start dispatches the agent again.
#[derive(Clone, Debug)]
pub struct LocalTokenSource {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    pub identity: String,
    /// Empty to dispatch no agent explicitly, leaving it to automatic dispatch.
    pub agent_name: String,
    pub agent_metadata: String,
}

impl LocalTokenSource {
    /// A join token for `room`, dispatching the agent if the room is new.
    pub(super) fn mint(&self, room: String) -> Result<String, String> {
        let mut token = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(&self.identity)
            .with_name(&self.identity)
            .with_grants(VideoGrants {
                room_join: true,
                room,
                ..Default::default()
            });
        if !self.agent_name.is_empty() {
            token = token.with_room_config(RoomConfiguration {
                agents: vec![RoomAgentDispatch {
                    agent_name: self.agent_name.clone(),
                    metadata: self.agent_metadata.clone(),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        token.to_jwt().map_err(|e| e.to_string())
    }
}

#[async_trait]
impl TokenSourceFixed for LocalTokenSource {
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        let participant_token =
            self.mint(random_room_name())
                .map_err(|body| TokenSourceError::Server {
                    // `TokenSourceError` has no variant for local failures.
                    status: 0,
                    body: format!("failed to mint token: {body}"),
                })?;
        Ok(TokenSourceResponse {
            server_url: self.url.clone(),
            participant_token,
        })
    }
}

/// A room name unlikely to exist yet, so the agent is dispatched into it.
fn random_room_name() -> String {
    // Every `RandomState` is seeded differently, which is all the randomness needed here.
    let suffix = RandomState::new().build_hasher().finish() as u32;
    format!("agent-{suffix:08x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use livekit_api::access_token::TokenVerifier;

    fn source(agent_name: &str) -> LocalTokenSource {
        LocalTokenSource {
            url: "ws://localhost:7880".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "secret".to_string(),
            identity: "participant-0".to_string(),
            agent_name: agent_name.to_string(),
            agent_metadata: "{}".to_string(),
        }
    }

    fn claims(response: &TokenSourceResponse) -> livekit_api::access_token::Claims {
        TokenVerifier::with_api_key("devkey", "secret")
            .verify(&response.participant_token)
            .expect("minted token verifies")
    }

    #[tokio::test]
    async fn every_fetch_uses_a_new_room() {
        let source = source("my-agent");
        let first = claims(&source.fetch().await.unwrap());
        let second = claims(&source.fetch().await.unwrap());
        assert!(first.video.room.starts_with("agent-"));
        assert_ne!(first.video.room, second.video.room);
    }

    #[tokio::test]
    async fn dispatches_the_named_agent() {
        let response = source("my-agent").fetch().await.unwrap();
        assert!(response.dispatches_agent());
        let agents = claims(&response).room_config.unwrap().agents;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].agent_name, "my-agent");
        assert_eq!(agents[0].metadata, "{}");
    }

    #[tokio::test]
    async fn no_agent_name_dispatches_nothing() {
        let response = source("").fetch().await.unwrap();
        assert!(!response.dispatches_agent());
    }
}
