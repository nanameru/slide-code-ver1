use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Slide CLI - Generate markdown slides via chat
#[derive(Debug, Parser)]
#[clap(
    name = "slide",
    version,
    about = "Generate markdown slides via chat",
    long_about = "A terminal-based AI agent for generating markdown slides through interactive chat."
)]
struct Cli {
    #[clap(subcommand)]
    command: Option<Commands>,
    
    /// Prompt for one-shot slide generation
    #[clap(value_name = "PROMPT")]
    prompt: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Preview a markdown slide file
    Preview {
        /// Path to markdown file
        file: PathBuf,
    },
    /// Login with API key
    Login {
        /// API key for authentication
        #[clap(long)]
        api_key: Option<String>,
    },
    /// Logout and remove credentials
    Logout,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Preview { file }) => {
            println!("Preview mode: {}", file.display());
            slide_tui::run_preview(&file).await?;
        }
        Some(Commands::Login { api_key }) => {
            println!("Login mode");
            if let Some(key) = api_key {
                println!("Setting API key: {}...", &key[..8.min(key.len())]);
            } else {
                println!("Interactive login flow");
            }
        }
        Some(Commands::Logout) => {
            println!("Logging out...");
        }
        None => {
            if let Some(prompt) = cli.prompt {
                println!("One-shot mode: {}", prompt);
                // TODO: Generate slides from prompt
            } else {
                println!("Interactive mode");
                slide_tui::run_interactive().await?;
            }
        }
    }

    Ok(())
}