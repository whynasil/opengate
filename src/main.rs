use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use opengate::agent::AgentLoop;
use opengate::config;
use opengate::gateway::Gateway;
use opengate::session::SessionPool;
use opengate::storage::Storage;

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
    Tui,
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

async fn do_serve(config_path: &Path) {
    let path_str = path_display(config_path);
    let config = match config::Config::load(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("error: failed to load config from '{path_str}': {e}");
            std::process::exit(1);
        }
    };

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

fn do_tui() {
    println!("TUI not yet implemented");
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
        Commands::Tui => {
            do_tui();
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
