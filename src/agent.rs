//! Agent loop — processes user messages through LLM backends
//! with tool execution and multi-turn conversation support.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::error::GateError;
use crate::session::SessionPool;

#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    Idle,
    Running,
    WaitingForTools,
    Completed,
    Error(String),
}

#[allow(dead_code)]
pub struct AgentLoop {
    session_pool: Arc<SessionPool>,
    turn_budget: u32,
    tool_registry: Vec<String>,
    states: Mutex<HashMap<String, AgentState>>,
}

impl AgentLoop {
    pub fn new(session_pool: Arc<SessionPool>, turn_budget: u32) -> Self {
        Self {
            session_pool,
            turn_budget,
            tool_registry: Vec::new(),
            states: Mutex::new(HashMap::new()),
        }
    }

    pub async fn run(&self, session_id: &str, user_message: &str) -> Result<(), GateError> {
        {
            let mut states = self.states.lock().await;
            states.insert(session_id.to_string(), AgentState::Running);
        }

        self.session_pool
            .add_message(session_id, "user", user_message)
            .map_err(|e| GateError::Storage(e.to_string()))?;

        let response = format!("Echo: {user_message}");
        self.session_pool
            .add_message(session_id, "assistant", &response)
            .map_err(|e| GateError::Storage(e.to_string()))?;

        {
            let mut states = self.states.lock().await;
            states.insert(session_id.to_string(), AgentState::Completed);
        }

        Ok(())
    }

    pub async fn cancel(&self, session_id: &str) {
        let mut states = self.states.lock().await;
        states.insert(
            session_id.to_string(),
            AgentState::Error("cancelled".to_string()),
        );
    }

    pub async fn get_state(&self, session_id: &str) -> Option<AgentState> {
        let states = self.states.lock().await;
        states.get(session_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    #[tokio::test]
    async fn test_agent_echo_response() {
        let storage = Arc::new(Storage::open(":memory:").expect("Failed to open in-memory DB"));
        let pool = Arc::new(SessionPool::new(storage));
        let agent = AgentLoop::new(pool.clone(), 5);

        let session = pool.create_session().expect("Failed to create session");

        agent
            .run(&session.id, "Hello, agent!")
            .await
            .expect("Agent run failed");

        assert_eq!(
            agent.get_state(&session.id).await,
            Some(AgentState::Completed)
        );

        let messages = pool
            .get_messages(&session.id)
            .expect("Failed to get messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello, agent!");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "Echo: Hello, agent!");
    }
}
