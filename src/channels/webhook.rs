use async_trait::async_trait;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::agent::AgentLoop;
use crate::session::SessionPool;

use super::{Channel, ChannelMessage};

// ---------------------------------------------------------------------------
// Webhook payload / response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WebhookPayload {
    pub session_id: Option<String>,
    pub message: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub response: String,
    pub status: String,
    pub session_id: String,
}

// ---------------------------------------------------------------------------
// Webhook state (for axum)
// ---------------------------------------------------------------------------

pub struct WebhookState {
    pub agent_loop: Arc<AgentLoop>,
    pub session_pool: Arc<SessionPool>,
    pub api_key: Option<String>,
}

// ---------------------------------------------------------------------------
// Webhook channel (implements Channel trait — mostly no-op)
// ---------------------------------------------------------------------------

pub struct WebhookChannel {
    _api_key: Option<String>,
}

impl WebhookChannel {
    pub fn new(api_key: Option<String>) -> Self {
        Self { _api_key: api_key }
    }
}

#[async_trait]
impl Channel for WebhookChannel {
    fn name(&self) -> &str {
        "webhook"
    }

    async fn send_message(
        &self,
        _session_id: &str,
        _msg: ChannelMessage,
    ) -> super::ChannelResult<()> {
        // Webhook channel is receive-only; sending is a no-op.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Axum handler
// ---------------------------------------------------------------------------

pub async fn webhook_handler(
    State(ws): State<Arc<WebhookState>>,
    Json(payload): Json<WebhookPayload>,
) -> impl IntoResponse {
    // Validate API key if configured.
    if let Some(ref required_key) = ws.api_key {
        match &payload.api_key {
            Some(key) if key == required_key => {}
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": "Invalid or missing api_key"
                    })),
                )
                    .into_response();
            }
        }
    }

    // Determine session ID.
    let session_id = match payload.session_id {
        Some(id) if !id.is_empty() => id,
        _ => match ws.session_pool.create_session() {
            Ok(s) => s.id,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to create session: {e}")
                    })),
                )
                    .into_response();
            }
        },
    };

    // Run the agent loop.
    match ws.agent_loop.run(&session_id, &payload.message).await {
        Ok(()) => (
            StatusCode::OK,
            Json(WebhookResponse {
                response: format!("Agent processing started for: {}", payload.message),
                status: "ok".into(),
                session_id,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Agent error: {e}")
            })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn webhook_router(state: Arc<WebhookState>) -> Router {
    Router::new().route("/webhook", post(webhook_handler)).with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelId;
    use crate::channels::ChannelManager;

    #[test]
    fn test_webhook_channel_name() {
        let channel = WebhookChannel::new(None);
        assert_eq!(channel.name(), "webhook");
    }

    #[tokio::test]
    async fn test_webhook_channel_send_is_noop() {
        let channel = WebhookChannel::new(None);
        let result = channel.send_message("test", ChannelMessage::Text { text: "hi".into() }).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_register_webhook_channel() {
        let mut manager = ChannelManager::new();
        let channel = WebhookChannel::new(Some("secret".into()));
        manager.register(ChannelId::new("webhook"), Box::new(channel));
        assert_eq!(manager.list().len(), 1);
        assert!(manager.list()[0].as_str() == "webhook");
    }
}
