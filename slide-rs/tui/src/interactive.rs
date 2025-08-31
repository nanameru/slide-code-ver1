use anyhow::Result;
use crate::app::run_app;

pub struct InteractiveApp {
}

impl InteractiveApp {
    pub fn new() -> Self {
        Self {}
    }
    
    pub async fn run(&mut self) -> Result<()> {
        run_app().await
    }
}