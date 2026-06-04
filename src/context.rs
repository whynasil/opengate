//! Context builder — assembles LLM conversation context
//! from session history, system prompt, and user message.

use std::sync::Arc;

use crate::session::SessionPool;

pub use crate::llm::LlmMessage;

pub const SYSTEM_PROMPT: &str = "You are OpenGate, a high-performance AI agent gateway. \
You are concise, system-aware, and efficient. Respond to user queries directly and accurately.";

pub struct ContextBuilder {
    session_pool: Arc<SessionPool>,
}

impl ContextBuilder {
    pub fn new(session_pool: Arc<SessionPool>) -> Self {
        Self { session_pool }
    }

    pub fn build(
        &self,
        session_id: &str,
        user_message: &str,
    ) -> Result<Vec<LlmMessage>, crate::error::GateError> {
        let mut messages = Vec::new();

        messages.push(LlmMessage {
            role: "system".to_string(),
            content: SYSTEM_PROMPT.to_string(),
        });

        let history = self
            .session_pool
            .get_messages(session_id)
            .map_err(|e| crate::error::GateError::Storage(e.to_string()))?;

        for msg in &history {
            messages.push(LlmMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
            });
        }

        messages.push(LlmMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        Ok(messages)
    }

    pub fn tool_schema(&self) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    #[test]
    fn test_build_system_prompt_is_first() {
        let storage = Arc::new(Storage::open(":memory:").expect("Failed to open in-memory DB"));
        let pool = Arc::new(SessionPool::new(storage));
        let builder = ContextBuilder::new(pool.clone());

        let session = pool.create_session().expect("Failed to create session");

        let messages = builder.build(&session.id, "Hello").expect("Build failed");

        assert!(!messages.is_empty());
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, SYSTEM_PROMPT);
    }

    #[test]
    fn test_build_history_loaded() {
        let storage = Arc::new(Storage::open(":memory:").expect("Failed to open in-memory DB"));
        let pool = Arc::new(SessionPool::new(storage));
        let builder = ContextBuilder::new(pool.clone());

        let session = pool.create_session().expect("Failed to create session");

        pool.add_message(&session.id, "user", "First message")
            .expect("Failed to add message");
        pool.add_message(&session.id, "assistant", "First response")
            .expect("Failed to add message");

        let messages = builder
            .build(&session.id, "Second message")
            .expect("Build failed");

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, "First message");
        assert_eq!(messages[2].role, "assistant");
        assert_eq!(messages[2].content, "First response");
        assert_eq!(messages[3].role, "user");
        assert_eq!(messages[3].content, "Second message");
    }
}
