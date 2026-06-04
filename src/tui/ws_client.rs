use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::warn;

use crate::gateway::{ClientMessage, ServerMessage};

#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected,
    Disconnected,
    MessageReceived(ServerMessage),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum WsCommand {
    SendChat { message: String, session: String },
    Ping,
}

pub async fn ws_client_task(
    url: String,
    auth_token: String,
    mut cmd_rx: mpsc::Receiver<WsCommand>,
    event_tx: mpsc::Sender<WsEvent>,
) {
    let (ws_stream, _) = match connect_async(&url).await {
        Ok(stream) => stream,
        Err(e) => {
            let _ = event_tx.send(WsEvent::Error(format!("Connection failed: {e}"))).await;
            return;
        }
    };

    if event_tx.send(WsEvent::Connected).await.is_err() {
        return;
    }

    let (mut write, mut read) = ws_stream.split();

    let auth = ClientMessage::Auth { token: auth_token.clone() };
    if let Ok(data) = rmp_serde::to_vec(&auth) {
        let _ = write.send(Message::Binary(data)).await;
    }

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(WsCommand::SendChat { message, session }) => {
                        let msg = ClientMessage::Chat { message, session };
                        if let Ok(data) = rmp_serde::to_vec(&msg)
                            && write.send(Message::Binary(data)).await.is_err()
                        {
                            break;
                        }
                    }
                    Some(WsCommand::Ping) => {
                        let msg = ClientMessage::Ping;
                        if let Ok(data) = rmp_serde::to_vec(&msg)
                            && write.send(Message::Binary(data)).await.is_err()
                        {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        match rmp_serde::from_slice::<ServerMessage>(&data) {
                            Ok(server_msg) => {
                                if event_tx.send(WsEvent::MessageReceived(server_msg)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Failed to deserialize server message: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        let _ = event_tx.send(WsEvent::Disconnected).await;
                        break;
                    }
                    Some(Ok(Message::Ping(d))) => {
                        let _ = write.send(Message::Pong(d)).await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        let _ = event_tx.send(WsEvent::Error(format!("WS error: {e}"))).await;
                        break;
                    }
                    None => {
                        let _ = event_tx.send(WsEvent::Disconnected).await;
                        break;
                    }
                }
            }
        }
    }
}
