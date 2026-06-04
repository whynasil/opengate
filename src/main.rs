use std::sync::Arc;

use opengate::agent::AgentLoop;
use opengate::config;
use opengate::gateway::Gateway;
use opengate::session::SessionPool;
use opengate::storage::Storage;

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

    let storage = Arc::new(
        Storage::open(&config.workspace.path).expect("Failed to open storage"),
    );
    let session_pool = Arc::new(SessionPool::new(storage.clone()));
    let agent_loop = Arc::new(AgentLoop::new(session_pool.clone(), 50));

    let gateway = Gateway::new(storage, session_pool, agent_loop, Arc::new(config));

    gateway.start().await;
}
