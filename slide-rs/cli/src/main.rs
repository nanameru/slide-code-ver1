use slide_arg0::arg0_dispatch_or_else;
use slide_tui::{Cli as TuiCli, Command};
use clap::Parser;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    // Check if we're in Slide mode via environment variable
    let is_slide_mode = std::env::var("SLIDE_APP").is_ok();
    
    arg0_dispatch_or_else(|slide_linux_sandbox_exe| async move {
        cli_main(slide_linux_sandbox_exe, is_slide_mode).await?;
        Ok(())
    })
}

async fn cli_main(slide_linux_sandbox_exe: Option<PathBuf>, is_slide_mode: bool) -> anyhow::Result<()> {
    println!("Slide CLI v0.0.1");
    
    if is_slide_mode {
        println!("Running in Slide mode");
    }
    
    let tui_cli = TuiCli::parse();
    
    match &tui_cli.command {
        Some(Command::Preview { file_path }) => {
            slide_tui::run_preview(file_path).await?;
        }
        None => {
            slide_tui::run_main(tui_cli, slide_linux_sandbox_exe).await?;
        }
    }
    
    Ok(())
}
