//! Messaging channel abstraction — Telegram, Webhook, and extensible
//! channel types for delivering agent responses to users.

pub mod webhook;
pub mod telegram;

use std::collections::HashMap;
use std::fmt;

pub type ChannelResult<T> = Result<T, String>;

// ---------------------------------------------------------------------------
// ChannelId
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChannelId(String);

impl ChannelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// ChannelMessage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ChannelMessage {
    Text { text: String },
    Reply { text: String },
    Error { message: String },
}

// ---------------------------------------------------------------------------
// Channel trait
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
pub trait Channel: Send + Sync {
    /// Send a message to a specific session on this channel.
    async fn send_message(&self, session_id: &str, msg: ChannelMessage) -> ChannelResult<()>;

    /// Human-readable name for the channel type (e.g. "telegram", "discord").
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// ChannelManager
// ---------------------------------------------------------------------------

pub struct ChannelManager {
    channels: HashMap<ChannelId, Box<dyn Channel>>,
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelManager {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: ChannelId, channel: Box<dyn Channel>) {
        self.channels.insert(id, channel);
    }

    pub async fn send(
        &self,
        channel_id: &ChannelId,
        session_id: &str,
        msg: ChannelMessage,
    ) -> ChannelResult<()> {
        match self.channels.get(channel_id) {
            Some(channel) => channel.send_message(session_id, msg).await,
            None => Err(format!("Channel '{}' not found", channel_id)),
        }
    }

    pub fn list(&self) -> Vec<&ChannelId> {
        self.channels.keys().collect()
    }

    pub fn get(&self, id: &ChannelId) -> Option<&dyn Channel> {
        self.channels.get(id).map(|c| c.as_ref())
    }
}

pub fn register_telegram_channel(
    manager: &mut ChannelManager,
    id: ChannelId,
    token: String,
    allowed_users: Vec<String>,
) {
    let channel = telegram::TelegramChannel::new(token, allowed_users);
    manager.register(id, Box::new(channel));
}

pub fn register_webhook_channel(
    manager: &mut ChannelManager,
    id: ChannelId,
    api_key: Option<String>,
) {
    let channel = webhook::WebhookChannel::new(api_key);
    manager.register(id, Box::new(channel));
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyChannel;

    #[async_trait::async_trait]
    impl Channel for DummyChannel {
        async fn send_message(
            &self,
            _session_id: &str,
            _msg: ChannelMessage,
        ) -> ChannelResult<()> {
            Ok(())
        }

        fn name(&self) -> &str {
            "dummy"
        }
    }

    #[tokio::test]
    async fn test_register_and_send() {
        let mut manager = ChannelManager::new();
        let id = ChannelId::new("dummy");

        manager.register(id.clone(), Box::new(DummyChannel));

        let channels = manager.list();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0], &id);

        let result = manager
            .send(&id, "session-1", ChannelMessage::Text { text: "hello".into() })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_to_missing_channel() {
        let manager = ChannelManager::new();
        let id = ChannelId::new("nonexistent");

        let result = manager
            .send(&id, "session-1", ChannelMessage::Text { text: "hello".into() })
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_channel_id_display() {
        let id = ChannelId::new("telegram");
        assert_eq!(format!("{id}"), "telegram");
        assert_eq!(id.as_str(), "telegram");
    }
}
