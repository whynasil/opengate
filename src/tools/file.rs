use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::tools::{Tool, ToolDef, ToolOutput};

const MAX_READ_SIZE: usize = 200 * 1024;
const MAX_LIST_DEPTH: usize = 3;
const MAX_LIST_ENTRIES: usize = 1000;

#[derive(Debug)]
pub struct FileTool {
    workspace_root: PathBuf,
}

impl FileTool {
    pub fn new(workspace_root: String) -> Self {
        let path = PathBuf::from(&workspace_root);
        let canonical = if path.exists() { path.canonicalize().unwrap_or(path) } else { path };
        Self { workspace_root: canonical }
    }

    fn resolve_path(&self, path: &str) -> Result<PathBuf, String> {
        if path.starts_with('/') {
            return Err("Absolute paths are not allowed".to_string());
        }
        if path.contains("..") {
            return Err("Path traversal detected: path must not contain '..'".to_string());
        }

        let resolved = self.workspace_root.join(path);

        if resolved.exists() {
            let canonical =
                resolved.canonicalize().map_err(|e| format!("Failed to resolve path: {}", e))?;
            if !canonical.starts_with(&self.workspace_root) {
                return Err("Path escapes workspace root".to_string());
            }
            return Ok(canonical);
        }

        let mut parent = resolved.as_path();
        while !parent.exists() {
            match parent.parent() {
                Some(p) => parent = p,
                None => return Err("Path is outside filesystem root".to_string()),
            }
        }
        let canonical_parent =
            parent.canonicalize().map_err(|e| format!("Failed to resolve path: {}", e))?;
        if !canonical_parent.starts_with(&self.workspace_root) {
            return Err("Path escapes workspace root".to_string());
        }

        Ok(resolved)
    }

    async fn action_read(&self, path: &str) -> Result<ToolOutput, String> {
        let resolved = self.resolve_path(path)?;

        let file =
            fs::File::open(&resolved).await.map_err(|e| format!("Failed to open file: {}", e))?;

        let mut contents = Vec::new();
        file.take(MAX_READ_SIZE as u64 + 1)
            .read_to_end(&mut contents)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let truncated = contents.len() > MAX_READ_SIZE;
        contents.truncate(MAX_READ_SIZE);

        let text = String::from_utf8_lossy(&contents);

        let mut result = String::new();
        for (i, line) in text.split('\n').enumerate() {
            result.push_str(&format!("{:>6}: {}\n", i + 1, line));
        }

        if truncated {
            result.push_str("... (file truncated at 200KB)");
        }

        Ok(ToolOutput::Text(result))
    }

    async fn action_write(&self, path: &str, content: &str) -> Result<ToolOutput, String> {
        let resolved = self.resolve_path(path)?;

        if let Some(parent) = resolved.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create parent directories: {}", e))?;
        }

        fs::write(&resolved, content).await.map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(ToolOutput::Text(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            resolved.display()
        )))
    }

    async fn action_list(&self, path: &str) -> Result<ToolOutput, String> {
        let base =
            if path.is_empty() { self.workspace_root.clone() } else { self.resolve_path(path)? };

        if !base.is_dir() {
            return Err(format!("Path is not a directory: {}", base.display()));
        }

        let mut entries = Vec::new();
        for entry in walkdir::WalkDir::new(&base)
            .max_depth(MAX_LIST_DEPTH)
            .into_iter()
            .filter_map(|e| e.ok())
            .take(MAX_LIST_ENTRIES + 1)
        {
            let relative = entry
                .path()
                .strip_prefix(&base)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .to_string();

            let entry_type = if entry.file_type().is_dir() {
                "dir"
            } else if entry.file_type().is_symlink() {
                "symlink"
            } else {
                "file"
            };

            entries.push(format!("{}  {}", entry_type, relative));
        }

        let mut result = entries.join("\n");
        if entries.len() > MAX_LIST_ENTRIES {
            result.push_str(&format!("\n... (listing truncated at {} entries)", MAX_LIST_ENTRIES));
        }

        Ok(ToolOutput::Text(result))
    }
}

#[async_trait]
impl Tool for FileTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read, write, and list files within the workspace"
    }

    fn schema(&self) -> ToolDef {
        ToolDef {
            name: "file_read".into(),
            description: "Read, write, and list files within the workspace".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["read", "write", "list"],
                        "description": "The file operation to perform"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to the file or directory, relative to workspace root"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write (only used with write action)"
                    }
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, String> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter: action".to_string())?;

        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "read" => self.action_read(path).await,
            "write" => {
                let content = params.get("content").and_then(|v| v.as_str()).ok_or_else(|| {
                    "Missing required parameter: content for write action".to_string()
                })?;
                self.action_write(path, content).await
            }
            "list" => self.action_list(path).await,
            _ => Err(format!("Unknown action: {}. Must be one of: read, write, list", action)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_read_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello\nworld\n").unwrap();

        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "read", "path": "test.txt"});
        let result = tool.execute(params).await.unwrap();

        match result {
            ToolOutput::Text(text) => {
                assert!(text.contains("hello"));
                assert!(text.contains("world"));
            }
            ToolOutput::Error(e) => panic!("Expected success, got error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_read_file_with_line_numbers() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "read", "path": "test.txt"});
        let result = tool.execute(params).await.unwrap();

        match result {
            ToolOutput::Text(text) => {
                assert!(text.contains("     1: line1"));
                assert!(text.contains("     2: line2"));
                assert!(text.contains("     3: line3"));
                assert!(text.contains("     4: ")); // trailing newline
            }
            ToolOutput::Error(e) => panic!("Expected success, got error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_write_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "write", "path": "new_file.txt", "content": "test content"});
        let result = tool.execute(params).await.unwrap();

        match result {
            ToolOutput::Text(text) => assert!(text.contains("Successfully wrote")),
            ToolOutput::Error(e) => panic!("Expected success, got error: {}", e),
        }

        let content = std::fs::read_to_string(dir.path().join("new_file.txt")).unwrap();
        assert_eq!(content, "test content");
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "write", "path": "a/b/c/deep.txt", "content": "deep"});
        let result = tool.execute(params).await.unwrap();

        assert!(matches!(result, ToolOutput::Text(_)));

        let content = std::fs::read_to_string(dir.path().join("a/b/c/deep.txt")).unwrap();
        assert_eq!(content, "deep");
    }

    #[tokio::test]
    async fn test_list_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("c.txt"), "").unwrap();

        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "list", "path": ""});
        let result = tool.execute(params).await.unwrap();

        match result {
            ToolOutput::Text(text) => {
                assert!(text.contains("file  a.txt"));
                assert!(text.contains("file  b.txt"));
                assert!(text.contains("dir  sub"));
                assert!(text.contains("file  sub/c.txt"));
            }
            ToolOutput::Error(e) => panic!("Expected success, got error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_list_at_path() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("nested.txt"), "").unwrap();

        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "list", "path": "sub"});
        let result = tool.execute(params).await.unwrap();

        match result {
            ToolOutput::Text(text) => {
                assert!(text.contains("file  nested.txt"));
            }
            ToolOutput::Error(e) => panic!("Expected success, got error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_reject_absolute_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "read", "path": "/etc/passwd"});
        let result = tool.execute(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Absolute paths"));
    }

    #[tokio::test]
    async fn test_reject_path_traversal() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "read", "path": "../../etc/passwd"});
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reject_dotdot_in_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "read", "path": "foo/../../../etc/passwd"});
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reject_write_path_traversal() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "write", "path": "../escape.txt", "content": "bad"});
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_action() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({});
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "delete", "path": "foo"});
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_missing_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FileTool::new(dir.path().to_string_lossy().to_string());
        let params = json!({"action": "write", "path": "foo.txt"});
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }
}
