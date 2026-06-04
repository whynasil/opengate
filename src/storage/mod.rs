//! Storage layer — SQLite-backed persistent storage with
//! connection pooling, WAL mode, and schema migration.

pub mod memory;

use chrono::Utc;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug)]
struct FkCustomizer;

impl r2d2::CustomizeConnection<rusqlite::Connection, rusqlite::Error> for FkCustomizer {
    fn on_acquire(&self, conn: &mut rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Pool error: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Session not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

pub struct Storage {
    pool: Pool<SqliteConnectionManager>,
}

impl Storage {
    /// Open or create the storage database.
    ///
    /// # Errors
    /// Returns `StorageError` if the database cannot be opened.
    #[must_use = "the opened Storage must be used or memory operations are leaked"]
    pub fn open(path: &str) -> Result<Self, StorageError> {
        let manager = if path == ":memory:" {
            SqliteConnectionManager::memory()
        } else {
            SqliteConnectionManager::file(path)
        };
        let pool = Pool::builder()
            .connection_customizer(Box::new(FkCustomizer))
            .build(manager)?;

        let conn = pool.get()?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        Self::run_migrations(&conn)?;

        Ok(Self { pool })
    }

    fn run_migrations(conn: &rusqlite::Connection) -> Result<(), StorageError> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT PRIMARY KEY,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                metadata    TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE IF NOT EXISTS messages (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                created_at  TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );
            ",
        )?;
        Ok(())
    }

    /// Access the connection pool directly (for memory system etc).
    pub fn pool(&self) -> &Pool<SqliteConnectionManager> {
        &self.pool
    }

    pub fn create_session(&self) -> Result<Session, StorageError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO sessions (id, created_at, updated_at, metadata) VALUES (?1, ?2, ?3, ?4)",
            params![id, now, now, "{}"],
        )?;
        Ok(Session {
            id,
            created_at: now.clone(),
            updated_at: now,
            metadata: Value::Object(Default::default()),
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>, StorageError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, created_at, updated_at, metadata FROM sessions ORDER BY created_at DESC",
        )?;
        let sessions = stmt
            .query_map([], |row| {
                let metadata_str: String = row.get(3)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    metadata_str,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(id, created_at, updated_at, metadata_str)| {
                let metadata: Value = serde_json::from_str(&metadata_str).unwrap_or_default();
                Session {
                    id,
                    created_at,
                    updated_at,
                    metadata,
                }
            })
            .collect();
        Ok(sessions)
    }

    pub fn get_session(&self, id: &str) -> Result<Session, StorageError> {
        let conn = self.pool.get()?;
        let mut stmt = conn
            .prepare("SELECT id, created_at, updated_at, metadata FROM sessions WHERE id = ?1")?;
        let result = stmt
            .query_row(params![id], |row| {
                let metadata_str: String = row.get(3)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    metadata_str,
                ))
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => StorageError::NotFound(id.to_string()),
                other => StorageError::Db(other),
            })?;
        let (id, created_at, updated_at, metadata_str) = result;
        let metadata: Value = serde_json::from_str(&metadata_str).unwrap_or_default();
        Ok(Session {
            id,
            created_at,
            updated_at,
            metadata,
        })
    }

    pub fn delete_session(&self, id: &str) -> Result<(), StorageError> {
        let conn = self.pool.get()?;
        let deleted = conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        if deleted == 0 {
            return Err(StorageError::NotFound(id.to_string()));
        }
        Ok(())
    }

    pub fn insert_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> Result<Message, StorageError> {
        let now = Utc::now().to_rfc3339();
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO messages (session_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, role, content, now],
        )?;
        let id = conn.last_insert_rowid();
        Ok(Message {
            id,
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            created_at: now,
        })
    }

    pub fn get_messages(&self, session_id: &str) -> Result<Vec<Message>, StorageError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, created_at FROM messages WHERE session_id = ?1 ORDER BY id ASC",
        )?;
        let messages = stmt
            .query_map(params![session_id], |row| {
                Ok(Message {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_and_insert_message() {
        let storage = Storage::open(":memory:").expect("Failed to open in-memory DB");

        let session = storage.create_session().expect("Failed to create session");
        assert!(!session.id.is_empty());

        let msg = storage
            .insert_message(&session.id, "user", "Hello, world!")
            .expect("Failed to insert message");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello, world!");

        let messages = storage
            .get_messages(&session.id)
            .expect("Failed to get messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello, world!");

        let sessions = storage.list_sessions().expect("Failed to list sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, session.id);

        let fetched = storage
            .get_session(&session.id)
            .expect("Failed to get session");
        assert_eq!(fetched.id, session.id);

        storage
            .delete_session(&session.id)
            .expect("Failed to delete session");
        let sessions_after = storage.list_sessions().expect("Failed to list sessions");
        assert!(sessions_after.is_empty());
    }

    #[test]
    fn test_get_session_not_found() {
        let storage = Storage::open(":memory:").expect("Failed to open in-memory DB");
        let result = storage.get_session("nonexistent");
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[test]
    fn test_delete_session_not_found() {
        let storage = Storage::open(":memory:").expect("Failed to open in-memory DB");
        let result = storage.delete_session("nonexistent");
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }
}
