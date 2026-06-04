use async_trait::async_trait;
use opengate::channels::telegram::TelegramChannel;
use opengate::channels::webhook::WebhookChannel;
use opengate::channels::{Channel, ChannelId, ChannelManager, ChannelMessage};
use opengate::config::Config;
use opengate::session::SessionPool;
use opengate::storage::Storage;
use opengate::tools::file::FileTool;
use opengate::tools::terminal::TerminalTool;
use opengate::tools::{self, Tool, ToolOutput};
use serde_json::json;
use std::sync::Arc;

// ── Config ──

#[test]
fn test_load_default_config() {
    let config = Config::load("config/test.toml").expect("Failed to load test config");
    assert_eq!(config.gateway.host, "127.0.0.1");
    assert_eq!(config.gateway.port, 9378);
    assert_eq!(config.models.len(), 3);
}

// ── Sessions ──

#[test]
fn test_session_lifecycle() {
    let storage = Arc::new(Storage::open(":memory:").expect("Failed to open storage"));
    let pool = Arc::new(SessionPool::new(storage.clone()));

    let session = pool.create_session().expect("Failed to create session");
    assert!(!session.id.is_empty());

    let retrieved = pool.get_session(&session.id).expect("Failed to get session");
    assert!(retrieved.id == session.id);

    pool.delete_session(&session.id).expect("Failed to delete session");
    let gone = pool.get_session(&session.id);
    assert!(gone.is_none());
}

// ── Tools ──

#[tokio::test]
async fn test_terminal_tool_echo() {
    let tool = TerminalTool::new("/home/test".to_string());
    let params = json!({"command": "echo integration_ok"});
    let result = tool.execute(params).await.expect("echo should succeed");
    match result {
        ToolOutput::Text(text) => assert!(text.contains("integration_ok")),
        ToolOutput::Error(e) => panic!("Unexpected error: {e}"),
    }
}

#[tokio::test]
async fn test_file_tool_read_write() {
    let dir = format!("/home/test/opengate_int_{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let tool = FileTool::new(dir.clone());

    let params = json!({"action": "write", "path": "t.txt", "content": "Hello"});
    tool.execute(params).await.expect("write");

    let params = json!({"action": "read", "path": "t.txt"});
    let result = tool.execute(params).await.expect("read");
    match result {
        ToolOutput::Text(text) => assert!(text.contains("Hello")),
        ToolOutput::Error(e) => panic!("Unexpected error: {e}"),
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn test_file_tool_path_escape() {
    let dir = format!("/home/test/opengate_int_{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let tool = FileTool::new(dir.clone());

    let params = json!({"action": "read", "path": "../etc/passwd"});
    let result = tool.execute(params).await;
    assert!(result.is_err());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_tool_registry() {
    let mut registry = tools::ToolRegistry::new();
    registry.register(Box::new(TerminalTool::new("/home/test".to_string())));
    let tools = registry.list_tools();
    assert!(tools.contains(&"terminal"));
}

// ── Channels ──

#[test]
fn test_webhook_channel_registration() {
    let mut manager = ChannelManager::new();
    let channel = WebhookChannel::new(Some("secret".into()));
    manager.register(ChannelId::new("webhook"), Box::new(channel));
    assert_eq!(manager.list().len(), 1);
}

#[test]
fn test_telegram_allowed_users() {
    let channel =
        TelegramChannel::new("dummy-token".to_string(), vec!["123".to_string(), "456".to_string()]);
    assert!(channel.is_allowed("123"));
    assert!(!channel.is_allowed("999"));
}

#[tokio::test]
async fn test_channel_manager_send() {
    struct EchoChannel;
    #[async_trait]
    impl Channel for EchoChannel {
        fn name(&self) -> &str {
            "echo"
        }
        async fn send_message(
            &self,
            _session_id: &str,
            _msg: ChannelMessage,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    let mut manager = ChannelManager::new();
    let id = ChannelId::new("echo");
    manager.register(id.clone(), Box::new(EchoChannel));

    let result = manager.send(&id, "s1", ChannelMessage::Text { text: "hi".into() }).await;
    assert!(result.is_ok());
}
