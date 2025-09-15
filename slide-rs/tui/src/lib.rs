pub mod app;
pub mod preview;
pub mod interactive;
pub mod agent;
pub mod bottom_pane;
pub mod app_event_sender;
pub mod user_approval_widget;
pub mod widgets;
pub mod insert_history;

use anyhow::Result;
use std::path::{Path, PathBuf};
use clap::Parser;

pub use app::*;
pub use preview::*;
pub use interactive::*;

#[derive(Debug, Parser, Default)]
pub struct Cli {
    /// Enable debug output
    #[clap(long)]
    pub debug: bool,
    /// Override model (e.g., gpt-5)
    #[clap(long)]
    pub model: Option<String>,
    /// Approval policy: untrusted | on-failure | on-request | never
    #[clap(long, value_parser = ["untrusted","on-failure","on-request","never"].into_iter().collect::<Vec<&'static str>>())]
    pub approval_mode: Option<String>,
}

pub async fn run_main(cli: Cli, _sandbox_exe: Option<PathBuf>) -> Result<()> {
    if cli.debug {
        println!("Debug mode enabled");
    }
    if let Some(model) = cli.model.clone() {
        std::env::set_var("SLIDE_MODEL", model);
    }
    if let Some(mode) = cli.approval_mode.clone() {
        std::env::set_var("SLIDE_APPROVAL_MODE", mode);
    }
    
    run_interactive().await
}

/// Run slide preview for a markdown file
pub async fn run_preview<P: AsRef<Path>>(file_path: P) -> Result<()> {
    let content = tokio::fs::read_to_string(file_path).await?;
    let slides = parse_slides(&content);
    
    let mut preview = SlidePreview::new(slides);
    preview.run().await
}

/// Run interactive slide creation mode
pub async fn run_interactive() -> Result<()> {
    let mut app = InteractiveApp::new();
    app.run().await
}

/// Parse markdown content into slides
fn parse_slides(content: &str) -> Vec<String> {
    let mut slides = Vec::new();
    let mut current_slide = String::new();
    
    for line in content.lines() {
        if line.starts_with("## ") && !current_slide.is_empty() {
            slides.push(current_slide.trim().to_string());
            current_slide = String::new();
        }
        current_slide.push_str(line);
        current_slide.push('\n');
    }
    
    if !current_slide.trim().is_empty() {
        slides.push(current_slide.trim().to_string());
    }
    
    if slides.is_empty() {
        slides.push(content.to_string());
    }
    
    slides
}
