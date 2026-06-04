use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures::{
    sink::SinkExt,
    stream::{SplitSink, StreamExt},
};
use serde::{Deserialize, Serialize};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};

use crate::config;
use crate::storage::Storage;

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Auth { token: String },
    Chat { message: String, session: String },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    AuthOk,
    ChatResponse { content: String, session: String },
    Error { code: String, message: String },
    Pong,
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: "0.1.0",
    })
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_router(state: Arc<Gateway>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/", get(ws_upgrade_handler))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------

pub struct Gateway {
    #[allow(dead_code)]
    storage: Arc<Storage>,
    config: Arc<config::Config>,
}

impl Gateway {
    pub fn new(storage: Arc<Storage>, config: Arc<config::Config>) -> Self {
        Self { storage, config }
    }

    pub async fn start(self) {
        let addr = format!("{}:{}", self.config.gateway.host, self.config.gateway.port);
        let state = Arc::new(self);
        let app = build_router(state);

        info!("Gateway listening on {addr}");

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .expect("Failed to bind gateway address");
        axum::serve(listener, app)
            .await
            .expect("Gateway failed");
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<Gateway>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, gateway: Arc<Gateway>) {
    let (mut sender, mut receiver) = socket.split();
    let auth_token = &gateway.config.gateway.auth_token;
    let mut authenticated = false;

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                warn!("WebSocket recv error: {e}");
                break;
            }
        };

        // Handle raw WebSocket frames – only Binary is accepted for MsgPack.
        let data = match msg {
            Message::Binary(data) => data,
            Message::Close(_) => break,
            Message::Ping(d) => {
                if sender.send(Message::Pong(d)).await.is_err() {
                    break;
                }
                continue;
            }
            Message::Pong(_) => continue,
            Message::Text(_) => {
                let err = ServerMessage::Error {
                    code: "INVALID_FORMAT".into(),
                    message: "Only binary MessagePack messages are accepted".into(),
                };
                if encode_and_send(&mut sender, &err).await.is_err() {
                    break;
                }
                continue;
            }
        };

        // Deserialize the MsgPack payload.
        let client_msg = match rmp_serde::from_slice::<ClientMessage>(&data) {
            Ok(msg) => msg,
            Err(e) => {
                let err = ServerMessage::Error {
                    code: "DESERIALIZATION_ERROR".into(),
                    message: format!("Failed to deserialize: {e}"),
                };
                if encode_and_send(&mut sender, &err).await.is_err() {
                    break;
                }
                continue;
            }
        };

        // Process according to authentication state.
        if !authenticated {
            match client_msg {
                ClientMessage::Auth { token } if token == *auth_token => {
                    authenticated = true;
                    if encode_and_send(&mut sender, &ServerMessage::AuthOk).await.is_err() {
                        break;
                    }
                }
                ClientMessage::Auth { .. } => {
                    let err = ServerMessage::Error {
                        code: "AUTH_FAILED".into(),
                        message: "Invalid auth token".into(),
                    };
                    let _ = encode_and_send(&mut sender, &err).await;
                    break;
                }
                _ => {
                    let err = ServerMessage::Error {
                        code: "NOT_AUTHENTICATED".into(),
                        message: "First message must be Auth".into(),
                    };
                    let _ = encode_and_send(&mut sender, &err).await;
                    break;
                }
            }
        } else {
            match client_msg {
                ClientMessage::Chat { message, session } => {
                    let resp = ServerMessage::ChatResponse {
                        content: format!("Echo: {message}"),
                        session,
                    };
                    if encode_and_send(&mut sender, &resp).await.is_err() {
                        break;
                    }
                }
                ClientMessage::Ping => {
                    if encode_and_send(&mut sender, &ServerMessage::Pong).await.is_err() {
                        break;
                    }
                }
                // Authenticated user sends another Auth – inform them.
                ClientMessage::Auth { .. } => {
                    let err = ServerMessage::Error {
                        code: "ALREADY_AUTHENTICATED".into(),
                        message: "Already authenticated".into(),
                    };
                    if encode_and_send(&mut sender, &err).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding helper
// ---------------------------------------------------------------------------

/// Serialise a `ServerMessage` to MessagePack and send it as a binary frame.
async fn encode_and_send(
    sender: &mut SplitSink<WebSocket, Message>,
    msg: &ServerMessage,
) -> Result<(), ()> {
    let data = rmp_serde::to_vec(msg).expect("ServerMessage serialization should not fail");
    sender.send(Message::Binary(data.into())).await.map_err(|_| ())
}
