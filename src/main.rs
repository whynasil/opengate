pub mod agent;
pub mod config;
pub mod error;
pub mod gateway;
pub mod session;
pub mod storage;

use std::sync::Arc;

#[tokio::main]
async fn main() {
    let cfg_path =
        std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config/default.toml".to_string());

    let config = match config::Config::load(&cfg_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("error: failed to load config from '{cfg_path}': {e}");
            std::process::exit(1);
        }
    };

    let workspace_path = expand_tilde(&config.workspace.path);
    if let Err(e) = std::fs::create_dir_all(&workspace_path) {
        eprintln!("error: failed to create workspace directory '{workspace_path}': {e}");
        std::process::exit(1);
    }
    let db_path = format!("{workspace_path}/opengate.db");

    let storage = match storage::Storage::open(&db_path) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("error: failed to open storage at '{db_path}': {e}");
            std::process::exit(1);
        }
    };

    let config = Arc::new(config);

    println!();
    println!("  ╔══════════════════════════════════════════════════╗");
    println!("  ║               OpenGate v0.1.0                    ║");
    println!("  ║     High-Performance AI Agent Gateway            ║");
    println!("  ║     https://github.com/whynasil/opengate         ║");
    println!("  ╚══════════════════════════════════════════════════╝");
    println!();

    let gateway = gateway::Gateway::new(storage, config);
    gateway.start().await;
}

fn expand_tilde(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
        return format!("{home}/{rest}");
    }
    trimmed.to_string()
}
