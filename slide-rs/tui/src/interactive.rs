use anyhow::Result;
use crate::app::{run_app, AppExit, RunResult};
use crate::run_preview;

pub struct InteractiveApp {
}

impl InteractiveApp {
    pub fn new() -> Self {
        Self {}
    }
    
    pub async fn run(&mut self) -> Result<()> {
        let mut recent_files: Vec<String> = Vec::new();
        loop {
            let RunResult { exit, recent_files: recents } = run_app(recent_files).await?;
            recent_files = recents;
            match exit {
                AppExit::Quit => break,
                AppExit::Preview(path) => {
                    // Run preview UI; when it exits, resume interactive loop
                    run_preview(path).await?;
                }
            }
        }
        Ok(())
    }
}
