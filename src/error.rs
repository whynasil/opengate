//! Centralized error types — `GateError` enum using `thiserror`
//! for all error domains across the gateway.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum GateError {
    #[error("Config error: {0}")]
    Config(#[from] crate::config::ConfigError),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Gateway error: {0}")]
    Gateway(String),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("LLM error: {0}")]
    LLM(String),

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Timeout")]
    Timeout,
}
