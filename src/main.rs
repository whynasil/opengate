use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use opengate::agent::AgentLoop;
use opengate::config;
use opengate::gateway::Gateway;
use opengate::session::SessionPool;
use opengate::storage::Storage;
use opengate::tui::TuiApp;

#[derive(Parser)]
#[command(name = "opengate", version, about = "High-Performance AI Agent Gateway")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server
    Serve {
        /// Path to config file
        #[arg(short = 'c', long, default_value = "config/default.toml")]
        config: PathBuf,
    },
    /// Start terminal UI client
    Tui {
        /// Path to config file
        #[arg(short = 'c', long, default_value = "config/default.toml")]
        config: PathBuf,
    },
    /// Config management commands
    #[command(subcommand)]
    Config(ConfigCmd),
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// Print the default config file path
    Path,
    /// Validate a config file
    Validate {
        /// Path to config file
        #[arg(short = 'c', long, default_value = "config/default.toml")]
        config: PathBuf,
    },
}

/// Convert a path to a lossy UTF-8 string for display/error messages.
fn path_display(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

async fn load_config_or_exit(config_path: &Path) -> config::Config {
    let path_str = path_display(config_path);
    match config::Config::load(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("error: failed to load config from '{path_str}': {e}");
            std::process::exit(1);
        }
    }
}

async fn do_serve(config_path: &Path) {
    let config = load_config_or_exit(config_path).await;

    let storage = match Storage::open(&config.workspace.path) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("error: failed to open storage: {e}");
            std::process::exit(1);
        }
    };

    let session_pool = Arc::new(SessionPool::new(storage.clone()));
    let agent_loop = Arc::new(AgentLoop::new(session_pool.clone(), 50));

    let gateway = Gateway::new(storage, session_pool, agent_loop, Arc::new(config));
    gateway.start().await;
}

async fn do_tui(config_path: &Path) {
    let config = load_config_or_exit(config_path).await;
    let config = Arc::new(config);

    if let Err(e) = TuiApp::run(config).await {
        eprintln!("TUI error: {e}");
        std::process::exit(1);
    }
}

fn do_config_path() {
    println!("config/default.toml");
}

fn do_config_validate(config_path: &Path) {
    match config::Config::load(config_path) {
        Ok(_) => println!("Config valid"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { ref config } => {
            do_serve(config).await;
        }
        Commands::Tui { ref config } => {
            do_tui(config).await;
        }
        Commands::Config(cmd) => match cmd {
            ConfigCmd::Path => {
                do_config_path();
            }
            ConfigCmd::Validate { ref config } => {
                do_config_validate(config);
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_parse_serve_default_config() {
        let cli = Cli::try_parse_from(["opengate", "serve"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_parse_serve_custom_config() {
        let cli = Cli::try_parse_from(["opengate", "serve", "--config", "custom.toml"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(matches!(cli.command, Commands::Serve { .. }));
    }

    #[test]
    fn test_parse_tui_default_config() {
        let cli = Cli::try_parse_from(["opengate", "tui"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_parse_config_path() {
        let cli = Cli::try_parse_from(["opengate", "config", "path"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(matches!(cli.command, Commands::Config(ConfigCmd::Path)));
    }

    #[test]
    fn test_parse_config_validate() {
        let cli = Cli::try_parse_from(["opengate", "config", "validate"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_parse_invalid_subcommand() {
        let cli = Cli::try_parse_from(["opengate", "invalid"]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_path_display_utf8() {
        let s = path_display(Path::new("config/default.toml"));
        assert_eq!(s, "config/default.toml");
    }
}
