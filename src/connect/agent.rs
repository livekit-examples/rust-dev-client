use super::{Auth, e2ee_options};
use async_trait::async_trait;
use livekit::prelude::RoomOptions;
use livekit_api::access_token::{AccessToken, VideoGrants};
use livekit_protocol::{RoomAgentDispatch, RoomConfiguration};
use livekit_session::{Session, SessionOptions};
use livekit_token_source::{
    TokenSourceError, TokenSourceFixed, TokenSourceResponse, TokenSourceResult,
    development_token_server, literal,
};
use std::hash::{BuildHasher, Hasher, RandomState};
use std::time::Duration;

/// Settings for starting an agent session, entered on the connect screen.
#[derive(Clone)]
pub struct AgentConnectSettings {
    pub auth: Auth,
    /// The shared E2EE key, if encryption is enabled.
    pub e2ee_key: Option<String>,
    /// Publish the microphone as soon as the session starts.
    pub voice: bool,
    pub agent_connect_timeout: Duration,
}

/// A session that connects as the settings say; nothing happens until it is
/// started.
impl From<&AgentConnectSettings> for Session {
    fn from(settings: &AgentConnectSettings) -> Self {
        let mut room_options = RoomOptions::default();
        room_options.encryption = e2ee_options(settings.e2ee_key.as_deref());
        let session_options = SessionOptions::default()
            .with_agent_connect_timeout(settings.agent_connect_timeout)
            .with_room_options(room_options);
        match settings.auth.clone() {
            Auth::Token { url, token } => Session::new(literal(url, token), session_options),
            Auth::ApiKey(token_source) => Session::new(token_source, session_options),
            Auth::DevelopmentTokenServer {
                token_server_id,
                options: fetch_options,
            } => Session::with_fetch_options(
                development_token_server(token_server_id),
                fetch_options,
                session_options,
            ),
        }
    }
}

/// Mints a join token locally from API credentials.
#[derive(Clone, Debug)]
pub struct LocalTokenSource {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    pub identity: String,
    /// The room to join, or `None` for a new `agent-<random>` room per fetch,
    /// so that every session start dispatches the agent again.
    pub room: Option<String>,
    /// Empty to dispatch no agent explicitly, leaving it to automatic dispatch.
    pub agent_name: String,
    pub agent_metadata: String,
}

impl LocalTokenSource {
    /// A join token for the room, dispatching the agent if the room is new.
    fn mint(&self) -> Result<String, String> {
        let room = self.room.clone().unwrap_or_else(random_room_name);
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
        let participant_token = self.mint().map_err(|body| TokenSourceError::Server {
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
            room: None,
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
    async fn a_named_room_is_joined_as_is() {
        let source = LocalTokenSource {
            room: Some("dev-room".to_string()),
            ..source("my-agent")
        };
        let first = claims(&source.fetch().await.unwrap());
        let second = claims(&source.fetch().await.unwrap());
        assert_eq!(first.video.room, "dev-room");
        assert_eq!(second.video.room, "dev-room");
    }

    #[tokio::test]
    async fn dispatches_the_named_agent() {
        let response = source("my-agent").fetch().await.unwrap();
        let agents = claims(&response).room_config.unwrap().agents;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].agent_name, "my-agent");
        assert_eq!(agents[0].metadata, "{}");
    }

    #[tokio::test]
    async fn no_agent_name_dispatches_nothing() {
        let response = source("").fetch().await.unwrap();
        assert!(claims(&response).room_config.is_none());
    }
}
