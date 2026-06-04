//! Tool system — extensible tool registry with terminal, file,
//! and browser tools for agent operations.

use serde_json::Value;
use std::collections::HashMap;

pub mod browser;
pub mod file;
pub mod terminal;

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub enum ToolOutput {
    Text(String),
    Error(String),
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> ToolDef;
    async fn execute(&self, params: Value) -> Result<ToolOutput, String>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool + Send + Sync>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool + Send + Sync>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    pub fn list_tools(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    pub fn get_tool(&self, name: &str) -> Option<&(dyn Tool + Send + Sync)> {
        self.tools.get(name).map(|t| t.as_ref())
    }
}

pub fn register_terminal_tool(registry: &mut ToolRegistry, default_workdir: String) {
    registry.register(Box::new(terminal::TerminalTool::new(default_workdir)));
}

pub fn register_file_tool(registry: &mut ToolRegistry, workspace_root: String) {
    registry.register(Box::new(file::FileTool::new(workspace_root)));
}

pub fn register_browser_tool(registry: &mut ToolRegistry) {
    registry.register(Box::new(browser::BrowserTool::new()));
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyTool;

    #[async_trait::async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "A dummy tool for testing"
        }
        fn schema(&self) -> ToolDef {
            ToolDef {
                name: "dummy".into(),
                description: "A dummy tool for testing".into(),
                parameters: Value::Object(Default::default()),
            }
        }
        async fn execute(&self, _params: Value) -> Result<ToolOutput, String> {
            Ok(ToolOutput::Text("ok".into()))
        }
    }

    #[tokio::test]
    async fn test_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));

        let names = registry.list_tools();
        assert_eq!(names, vec!["dummy"]);

        let tool = registry.get_tool("dummy");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "dummy");

        let missing = registry.get_tool("nonexistent");
        assert!(missing.is_none());
    }
}
