use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;

use crate::storage::{Message, Storage, StorageError};

#[derive(Debug, Clone)]
pub struct SessionState {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: HashMap<String, Value>,
}

pub struct SessionPool {
    storage: Arc<RwLock<Storage>>,
}

impl SessionPool {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage: Arc::new(RwLock::new(storage)),
        }
    }

    pub async fn create_session(&self) -> Result<SessionState, StorageError> {
        let storage = self.storage.write().await;
        storage.create_session().map(Into::into)
    }

    pub async fn get_session(&self, id: &str) -> Option<SessionState> {
        let storage = self.storage.read().await;
        storage.get_session(id).ok().map(Into::into)
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), StorageError> {
        let storage = self.storage.write().await;
        storage.delete_session(id)
    }

    pub async fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> Result<Message, StorageError> {
        let storage = self.storage.write().await;
        storage.insert_message(session_id, role, content)
    }

    pub async fn get_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<Message>, StorageError> {
        let storage = self.storage.read().await;
        storage.get_messages(session_id)
    }
}

impl From<crate::storage::Session> for SessionState {
    fn from(session: crate::storage::Session) -> Self {
        let metadata = match session.metadata {
            Value::Object(map) => map.into_iter().collect(),
            _ => HashMap::new(),
        };
        SessionState {
            id: session.id,
            created_at: session.created_at,
            updated_at: session.updated_at,
            metadata,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    #[tokio::test]
    async fn test_create_session_and_add_message() {
        let storage = Storage::open(":memory:").expect("Failed to open in-memory DB");
        let pool = SessionPool::new(storage);

        let session = pool.create_session().await.expect("Failed to create session");
        assert!(!session.id.is_empty());
        assert!(session.metadata.is_empty());

        let msg = pool
            .add_message(&session.id, "user", "Hello!")
            .await
            .expect("Failed to add message");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello!");

        let messages = pool
            .get_messages(&session.id)
            .await
            .expect("Failed to get messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello!");

        let fetched = pool.get_session(&session.id).await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, session.id);

        pool.delete_session(&session.id)
            .await
            .expect("Failed to delete session");

        let after = pool.get_session(&session.id).await;
        assert!(after.is_none());
    }

    #[tokio::test]
    async fn test_get_session_nonexistent() {
        let storage = Storage::open(":memory:").expect("Failed to open in-memory DB");
        let pool = SessionPool::new(storage);
        let result = pool.get_session("nonexistent").await;
        assert!(result.is_none());
    }
}
