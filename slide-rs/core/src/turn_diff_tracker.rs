#[derive(Debug, Clone, Default)]
pub struct TurnDiffTracker {
    started: bool,
    unified: Option<String>,
}

impl TurnDiffTracker {
    pub fn new() -> Self { Self { started: false, unified: None } }

    pub fn on_patch_begin<T: ToString>(&mut self, changes: &[T]) {
        self.started = true;
        // Minimal representation: join change descriptions
        let summary = changes.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("\n");
        self.unified = Some(format!("--- turn diff (summary) ---\n{}\n", summary));
    }

    pub fn get_unified_diff(&self) -> Result<Option<String>, ()> {
        Ok(self.unified.clone())
    }
}
