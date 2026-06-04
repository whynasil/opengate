use std::sync::Arc;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Memory types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub category: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// MemoryStore
// ---------------------------------------------------------------------------

pub struct MemoryStore {
    pool: Arc<Pool<SqliteConnectionManager>>,
}

impl MemoryStore {
    pub fn new(pool: Arc<Pool<SqliteConnectionManager>>) -> Self {
        Self { pool }
    }

    /// Initialize the memory table if it doesn't exist.
    pub fn init(&self) -> Result<(), String> {
        let conn = self
            .pool
            .get()
            .map_err(|e| format!("DB connection error: {e}"))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS memory (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT NOT NULL UNIQUE,
                value TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'general',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )
        .map_err(|e| format!("Failed to create memory table: {e}"))?;

        Ok(())
    }

    /// Set a memory key-value pair.
    pub fn set(&self, key: &str, value: &str, category: &str) -> Result<(), String> {
        let conn = self
            .pool
            .get()
            .map_err(|e| format!("DB connection error: {e}"))?;

        conn.execute(
            "INSERT INTO memory (key, value, category)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                 value = excluded.value,
                 category = excluded.category,
                 created_at = datetime('now')",
            rusqlite::params![key, value, category],
        )
        .map_err(|e| format!("Failed to set memory: {e}"))?;

        Ok(())
    }

    /// Get a single memory entry by key.
    pub fn get(&self, key: &str) -> Result<Option<MemoryEntry>, String> {
        let conn = self
            .pool
            .get()
            .map_err(|e| format!("DB connection error: {e}"))?;

        let mut stmt = conn
            .prepare("SELECT key, value, category, created_at FROM memory WHERE key = ?1")
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let result = stmt
            .query_row(rusqlite::params![key], |row| {
                Ok(MemoryEntry {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    category: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .optional()
            .map_err(|e| format!("Failed to query memory: {e}"))?;

        Ok(result)
    }

    /// Delete a memory entry by key.
    pub fn delete(&self, key: &str) -> Result<bool, String> {
        let conn = self
            .pool
            .get()
            .map_err(|e| format!("DB connection error: {e}"))?;

        let rows = conn
            .execute("DELETE FROM memory WHERE key = ?1", rusqlite::params![key])
            .map_err(|e| format!("Failed to delete memory: {e}"))?;

        Ok(rows > 0)
    }

    /// List all memory entries, optionally filtered by category.
    pub fn list(&self, category: Option<&str>) -> Result<Vec<MemoryEntry>, String> {
        let conn = self
            .pool
            .get()
            .map_err(|e| format!("DB connection error: {e}"))?;

        let entries = if let Some(cat) = category {
            let mut stmt = conn
                .prepare(
                    "SELECT key, value, category, created_at FROM memory WHERE category = ?1 ORDER BY created_at DESC",
                )
                .map_err(|e| format!("Failed to prepare query: {e}"))?;

            let rows = stmt
                .query_map(rusqlite::params![cat], |row| {
                    Ok(MemoryEntry {
                        key: row.get(0)?,
                        value: row.get(1)?,
                        category: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                })
                .map_err(|e| format!("Failed to query memory: {e}"))?;

            let mut vec = Vec::new();
            for row in rows {
                vec.push(row.map_err(|e| format!("Row error: {e}"))?);
            }
            vec
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT key, value, category, created_at FROM memory ORDER BY created_at DESC",
                )
                .map_err(|e| format!("Failed to prepare query: {e}"))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(MemoryEntry {
                        key: row.get(0)?,
                        value: row.get(1)?,
                        category: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                })
                .map_err(|e| format!("Failed to query memory: {e}"))?;

            let mut vec = Vec::new();
            for row in rows {
                vec.push(row.map_err(|e| format!("Row error: {e}"))?);
            }
            vec
        };

        Ok(entries)
    }

    /// Build a system prompt inject from all non-empty memory entries.
    pub fn build_inject(&self) -> Result<String, String> {
        let entries = self.list(None)?;
        if entries.is_empty() {
            return Ok(String::new());
        }

        let mut lines = Vec::new();
        for entry in &entries {
            lines.push(format!("- [{}] {}", entry.category, entry.value));
        }

        Ok(format!(
            "<memory>\nThe following facts are saved from previous conversations:\n{}\n</memory>",
            lines.join("\n")
        ))
    }
}

// ---------------------------------------------------------------------------
// Helper trait for rusqlite OptionalRow
// ---------------------------------------------------------------------------

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use r2d2::Pool;

    fn setup_store() -> (MemoryStore, PathBuf) {
        let dir = PathBuf::from(format!(
            "/home/test/opengate_memory_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");

        let manager = SqliteConnectionManager::file(db_path.to_str().unwrap());
        let pool = Pool::builder()
            .max_size(2)
            .build(manager)
            .expect("Failed to create pool");

        // Create tables
        let conn = pool.get().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT NOT NULL UNIQUE,
                value TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'general',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );"
        )
        .unwrap();

        let store = MemoryStore::new(Arc::new(pool));
        (store, dir)
    }

    fn cleanup(dir: &PathBuf) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_set_and_get() {
        let (store, dir) = setup_store();

        store.set("user_name", "Faruk", "user").unwrap();
        let entry = store.get("user_name").unwrap().unwrap();
        assert_eq!(entry.value, "Faruk");
        assert_eq!(entry.category, "user");

        cleanup(&dir);
    }

    #[test]
    fn test_get_missing() {
        let (store, dir) = setup_store();
        let entry = store.get("nonexistent").unwrap();
        assert!(entry.is_none());
        cleanup(&dir);
    }

    #[test]
    fn test_delete() {
        let (store, dir) = setup_store();
        store.set("temp", "value", "test").unwrap();
        let deleted = store.delete("temp").unwrap();
        assert!(deleted);
        let deleted_again = store.delete("temp").unwrap();
        assert!(!deleted_again);
        cleanup(&dir);
    }

    #[test]
    fn test_list_by_category() {
        let (store, dir) = setup_store();
        store.set("a", "1", "cat-a").unwrap();
        store.set("b", "2", "cat-b").unwrap();
        store.set("c", "3", "cat-a").unwrap();

        let cat_a = store.list(Some("cat-a")).unwrap();
        assert_eq!(cat_a.len(), 2);

        cleanup(&dir);
    }

    #[test]
    fn test_build_inject() {
        let (store, dir) = setup_store();
        store.set("fact1", "User is Turkish", "user").unwrap();
        store.set("fact2", "Project uses Rust 2024", "project").unwrap();

        let inject = store.build_inject().unwrap();
        assert!(inject.contains("Turkish"));
        assert!(inject.contains("Rust 2024"));
        assert!(inject.contains("<memory>"));

        cleanup(&dir);
    }
}
