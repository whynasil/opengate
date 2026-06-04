use teloxide::Bot;
use teloxide::prelude::Requester;
use teloxide::types::ChatId;

use super::{Channel, ChannelMessage, ChannelResult};

pub struct TelegramChannel {
    bot: Bot,
    allowed_users: Vec<String>,
}

impl TelegramChannel {
    pub fn new(token: impl Into<String>, allowed_users: Vec<String>) -> Self {
        Self { bot: Bot::new(token), allowed_users }
    }

    pub fn add_allowed_user(&mut self, user_id: String) {
        if !self.allowed_users.contains(&user_id) {
            self.allowed_users.push(user_id);
        }
    }

    pub fn is_allowed(&self, user_id: &str) -> bool {
        self.allowed_users.is_empty() || self.allowed_users.iter().any(|u| u == user_id)
    }
}

#[async_trait::async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn send_message(&self, session_id: &str, msg: ChannelMessage) -> ChannelResult<()> {
        let chat_id: i64 =
            session_id.parse().map_err(|e| format!("Invalid chat ID '{session_id}': {e}"))?;
        let text = match &msg {
            ChannelMessage::Text { text } => text.clone(),
            ChannelMessage::Reply { text } => format!("Reply: {text}"),
            ChannelMessage::Error { message } => format!("Error: {message}"),
        };
        self.bot
            .send_message(ChatId(chat_id), text)
            .await
            .map_err(|e| format!("Failed to send telegram message: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_allowed_empty() {
        let channel = TelegramChannel::new("test_token", vec![]);
        assert!(channel.is_allowed("12345"));
    }

    #[test]
    fn test_is_allowed_match() {
        let channel = TelegramChannel::new("test_token", vec!["12345".into(), "67890".into()]);
        assert!(channel.is_allowed("12345"));
        assert!(channel.is_allowed("67890"));
    }

    #[test]
    fn test_is_allowed_no_match() {
        let channel = TelegramChannel::new("test_token", vec!["12345".into()]);
        assert!(!channel.is_allowed("99999"));
    }

    #[test]
    fn test_add_allowed_user() {
        let mut channel = TelegramChannel::new("test_token", vec![]);
        channel.add_allowed_user("100".into());
        assert!(channel.is_allowed("100"));
        channel.add_allowed_user("100".into());
        assert_eq!(channel.allowed_users.len(), 1);
    }
}
