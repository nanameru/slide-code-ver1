use crate::config::Config;
use std::fs;
use std::path::Path;
use tracing::error;

const PROJECT_DOC_SEPARATOR: &str = "\n\n---\n\n";

pub fn generate_overview_doc() -> String {
    "Core overview (stub)".into()
}

/// Get user instructions by combining project documentation with existing instructions
pub async fn get_user_instructions(config: &Config) -> Option<String> {
    match read_project_docs(config).await {
        Ok(Some(project_doc)) => match &config.user_instructions {
            Some(original_instructions) => Some(format!(
                "{original_instructions}{PROJECT_DOC_SEPARATOR}{project_doc}"
            )),
            None => Some(project_doc),
        },
        Ok(None) => config.user_instructions.clone(),
        Err(e) => {
            error!("error trying to find project doc: {e:#}");
            config.user_instructions.clone()
        }
    }
}

/// Read project documentation from AGENTS.md files
async fn read_project_docs(config: &Config) -> Result<Option<String>, std::io::Error> {
    let mut docs = Vec::new();
    
    // Check for AGENTS.md in the current directory
    let agents_path = config.cwd.join("AGENTS.md");
    if agents_path.exists() {
        if let Ok(content) = fs::read_to_string(&agents_path) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                let truncated = if trimmed.len() > config.project_doc_max_bytes {
                    &trimmed[..config.project_doc_max_bytes]
                } else {
                    trimmed
                };
                docs.push(truncated.to_string());
            }
        }
    }

    // Check for CLAUDE.md in the current directory
    let claude_path = config.cwd.join("CLAUDE.md");
    if claude_path.exists() {
        if let Ok(content) = fs::read_to_string(&claude_path) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                let truncated = if trimmed.len() > config.project_doc_max_bytes {
                    &trimmed[..config.project_doc_max_bytes]
                } else {
                    trimmed
                };
                docs.push(truncated.to_string());
            }
        }
    }

    if docs.is_empty() {
        Ok(None)
    } else {
        Ok(Some(docs.join(PROJECT_DOC_SEPARATOR)))
    }
}

