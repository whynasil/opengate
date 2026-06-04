use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

use crate::tools::{Tool, ToolDef, ToolOutput};

#[derive(Debug)]
pub struct TerminalTool {
    pub default_workdir: String,
}

impl TerminalTool {
    pub fn new(default_workdir: String) -> Self {
        Self { default_workdir }
    }
}

#[async_trait]
impl Tool for TerminalTool {
    fn name(&self) -> &str {
        "terminal"
    }

    fn description(&self) -> &str {
        "Execute shell commands in a sandboxed subprocess"
    }

    fn schema(&self) -> ToolDef {
        ToolDef {
            name: "terminal".into(),
            description: "Execute shell commands in a sandboxed subprocess".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds (default: 30)"
                    },
                    "workdir": {
                        "type": "string",
                        "description": "Working directory for the command"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, String> {
        let cmd_str = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter: command".to_string())?;

        let timeout_secs = params
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let workdir = params
            .get("workdir")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.default_workdir);

        let output = timeout(
            std::time::Duration::from_secs(timeout_secs),
            Command::new("sh")
                .arg("-c")
                .arg(cmd_str)
                .current_dir(workdir)
                .output(),
        )
        .await
        .map_err(|_| format!("Command timed out after {} seconds", timeout_secs))?
        .map_err(|e| format!("Failed to execute command: {}", e))?;

        let mut combined = String::new();
        if !output.stdout.is_empty() {
            combined.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&String::from_utf8_lossy(&output.stderr));
        }

        const MAX_OUTPUT: usize = 64 * 1024;
        if combined.len() > MAX_OUTPUT {
            combined.truncate(MAX_OUTPUT);
            combined.push_str("\n... (output truncated)");
        }

        if output.status.success() {
            Ok(ToolOutput::Text(combined))
        } else {
            let msg = format!(
                "Command exited with code {}\n{}",
                output.status.code().unwrap_or(-1),
                combined
            );
            Ok(ToolOutput::Error(msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_echo_hello() {
        let tool = TerminalTool::new("/tmp".into());
        let params = json!({"command": "echo hello"});
        let result = tool.execute(params).await.unwrap();
        match result {
            ToolOutput::Text(text) => assert_eq!(text.trim(), "hello"),
            ToolOutput::Error(e) => panic!("Expected success, got error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_missing_command() {
        let tool = TerminalTool::new("/tmp".into());
        let params = json!({});
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_timeout() {
        let tool = TerminalTool::new("/tmp".into());
        let params = json!({"command": "sleep 10", "timeout": 1});
        let result = tool.execute(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timed out"));
    }
}
