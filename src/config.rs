//! Configuration system — TOML parsing with `${ENV_VAR}` interpolation,
//! multi-model provider config, channel credentials, and workspace settings.

use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse TOML config: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Environment variable '{0}' is not set")]
    EnvVar(String),
    #[error("No model with default=true found")]
    NoDefaultModel,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub allowed_users: Vec<i64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GatewayConfig {
    pub host: String,
    pub port: u16,
    pub auth_token: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct WorkspaceConfig {
    pub path: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawConfig {
    pub gateway: GatewayConfig,
    pub models: Vec<ModelConfig>,
    pub channels: Option<HashMap<String, serde_json::Value>>,
    pub workspace: Option<WorkspaceConfig>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub gateway: Gateway,
    pub models: Vec<ModelConfig>,
    pub channels: Channels,
    pub workspace: Workspace,
}

#[derive(Debug, Clone)]
pub struct Gateway {
    pub host: String,
    pub port: u16,
    pub auth_token: String,
}

#[derive(Debug, Clone)]
pub struct Channels {
    pub telegram: Option<TelegramConfig>,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub path: String,
}

impl Config {
    /// Load configuration from a TOML file with env-var interpolation.
    ///
    /// # Errors
    /// Returns `ConfigError` if the file cannot be read, parsed, or has missing env vars.
    #[must_use = "the Config result must be used; ignoring it discards the load"]
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path.as_ref())?;
        let interpolated = Self::interpolate(&content)?;
        let raw: RawConfig = toml::from_str(&interpolated)?;

        let telegram = raw
            .channels
            .as_ref()
            .and_then(|ch| ch.get("telegram"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .map(|t: TelegramConfig| TelegramConfig {
                bot_token: Self::interpolate(&t.bot_token).unwrap_or(t.bot_token),
                allowed_users: t.allowed_users,
            });

        Ok(Config {
            gateway: Gateway {
                host: raw.gateway.host,
                port: raw.gateway.port,
                auth_token: raw.gateway.auth_token,
            },
            models: raw.models,
            channels: Channels { telegram },
            workspace: Workspace {
                path: raw
                    .workspace
                    .map(|w| w.path)
                    .unwrap_or_else(|| "~/.opengate".to_string()),
            },
        })
    }

    fn interpolate(input: &str) -> Result<String, ConfigError> {
        let mut result = String::with_capacity(input.len());
        let mut rest = input;

        while let Some(start) = rest.find("${") {
            result.push_str(&rest[..start]);
            rest = &rest[start + 2..];

            if let Some(end) = rest.find('}') {
                let var_name = &rest[..end];
                let value = std::env::var(var_name)
                    .map_err(|_| ConfigError::EnvVar(var_name.to_string()))?;
                result.push_str(&value);
                rest = &rest[end + 1..];
            } else {
                result.push_str("${");
            }
        }

        result.push_str(rest);
        Ok(result)
    }

    pub fn default_model(&self) -> Result<&ModelConfig, ConfigError> {
        self.models
            .iter()
            .find(|m| m.default)
            .ok_or(ConfigError::NoDefaultModel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_config() {
        unsafe {
            std::env::set_var("OPENGATE_AUTH_TOKEN", "test-token");
            std::env::set_var("OPENAI_API_KEY", "test-openai-key");
            std::env::set_var("ANTHROPIC_API_KEY", "test-anthropic-key");
            std::env::set_var("TELEGRAM_BOT_TOKEN", "test-telegram-token");
        }
        let config = Config::load("config/default.toml").expect("Failed to load default config");
        assert_eq!(config.gateway.host, "127.0.0.1");
        assert_eq!(config.gateway.port, 9378);
        assert_eq!(config.models.len(), 3);
        let default = config.default_model().expect("Should have default model");
        assert_eq!(default.id, "openai/gpt-5.2");
        assert!(config.channels.telegram.is_some());
        assert_eq!(config.workspace.path, "~/.opengate");
    }
}
