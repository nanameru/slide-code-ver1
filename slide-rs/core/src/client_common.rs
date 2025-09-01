#[derive(Debug, Clone, Default)]
pub struct Prompt {
    pub system: String,
    pub user: String,
    pub tools: Vec<String>,
}

impl Prompt {
    pub fn render(&self) -> String {
        format!("{}\n{}", self.system, self.user)
    }
}

