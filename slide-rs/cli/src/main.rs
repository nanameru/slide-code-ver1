use slide_arg0::arg0_dispatch_or_else;
use slide_tui::Cli as TuiCli;
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    // Check if we're in Slide mode via environment variable
    let is_slide_mode = std::env::var("SLIDE_APP").is_ok();
    // Load env.local if present (OPENAI_API_KEY etc.)
    try_load_env_local();
    
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
    
    // For now, just run the TUI
    let tui_cli = TuiCli::default();
    slide_tui::run_main(tui_cli, slide_linux_sandbox_exe).await?;
    
    Ok(())
}

fn try_load_env_local() {
    if std::env::var("OPENAI_API_KEY").is_ok() {
        return;
    }
    let candidates = [
        std::env::current_dir().ok(),
        std::env::current_dir().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())),
        std::env::current_dir().ok().and_then(|p| p.parent().and_then(|q| q.parent()).map(|p| p.to_path_buf())),
    ];
    for base in candidates.into_iter().flatten() {
        for name in ["env.local", ".env.local"] {
            let path = base.join(name);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for line in content.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') { continue; }
                        if let Some((k,v)) = parse_env_line(line) {
                            if std::env::var(&k).is_err() {
                                std::env::set_var(k, v);
                            }
                        }
                    }
                }
                return;
            }
        }
    }
}

fn parse_env_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.splitn(2, '=');
    let key = parts.next()?.trim();
    let val_raw = parts.next()?.trim();
    let val = val_raw.trim_matches('"').trim_matches('\'');
    if key.is_empty() { return None; }
    Some((key.to_string(), val.to_string()))
}