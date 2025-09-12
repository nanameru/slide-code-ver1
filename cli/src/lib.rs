use clap::{Parser, Subcommand};
use anyhow::Result;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "slide")]
#[command(about = "AI-powered slide generation and management")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Enable debug output
    #[arg(long)]
    pub debug: bool,

    /// Override model (e.g., gpt-4o, gpt-4o-mini)
    #[arg(long)]
    pub model: Option<String>,

    /// Approval policy: untrusted | on-failure | on-request | never
    #[arg(long)]
    pub approval_mode: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interactive slide creation mode
    Interactive,
    /// Preview existing markdown slides
    Preview {
        /// Path to markdown file
        file: PathBuf,
    },
    /// Generate slides from prompt
    Generate {
        /// Slide generation prompt
        prompt: String,
        /// Number of slides to generate
        #[arg(short, long, default_value = "6")]
        count: usize,
        /// Output language
        #[arg(short, long, default_value = "ja")]
        language: String,
    },
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        std::env::set_var("RUST_LOG", "debug");
    }

    if let Some(model) = &cli.model {
        std::env::set_var("SLIDE_MODEL", model);
    }

    if let Some(mode) = &cli.approval_mode {
        std::env::set_var("SLIDE_APPROVAL_MODE", mode);
    }

    match cli.command {
        Some(Commands::Interactive) => {
            slide_tui::run_interactive().await?;
        }
        Some(Commands::Preview { file }) => {
            slide_tui::run_preview(&file).await?;
        }
        Some(Commands::Generate { prompt, count, language }) => {
            generate_slides(prompt, count, language).await?;
        }
        None => {
            // Default to interactive mode
            slide_tui::run_interactive().await?;
        }
    }

    Ok(())
}

async fn generate_slides(prompt: String, count: usize, language: String) -> Result<()> {
    println!("Generating {} slides in {} for: {}", count, language, prompt);
    
    // For now, just create a simple markdown output
    let slides = create_sample_slides(&prompt, count);
    
    let output_path = "generated_slides.md";
    tokio::fs::write(output_path, slides).await?;
    
    println!("Slides saved to: {}", output_path);
    Ok(())
}

fn create_sample_slides(prompt: &str, count: usize) -> String {
    let mut content = format!("# {}\n\n", prompt);
    
    for i in 1..=count {
        content.push_str(&format!("## スライド {}: ", i));
        
        match i {
            1 => content.push_str("イントロダクション\n- 概要を説明します\n- 目的を明確化します\n\n"),
            n if n == count => content.push_str("まとめ\n- 重要ポイントの再確認\n- 次のステップ\n\n"),
            _ => content.push_str(&format!("内容 {}\n- ポイント 1\n- ポイント 2\n\n", i)),
        }
    }
    
    content
}