use std::path::PathBuf;
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

fn get_config_path() -> String {
    std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config/default.toml".to_string())
}

fn do_serve(config_path: &str) {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    rt.block_on(async {
        let config = match config::Config::load(config_path) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("error: failed to load config from '{config_path}': {e}");
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
    });
}

fn do_tui() {
    println!("TUI not yet implemented");
}

fn do_config_path() {
    println!("{}", get_config_path());
}

fn do_config_validate(config_path: &str) {
    match config::Config::load(config_path) {
        Ok(_) => println!("Config valid"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { config } => {
            do_serve(config.to_str().unwrap_or("config/default.toml"));
        }
        Commands::Tui => {
            do_tui();
        }
        Commands::Config(cmd) => match cmd {
            ConfigCmd::Path => {
                do_config_path();
            }
            ConfigCmd::Validate { config } => {
                do_config_validate(config.to_str().unwrap_or("config/default.toml"));
            }
        },
    }
}
