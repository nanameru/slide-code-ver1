use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalMode {
    /// Always prompt for approval
    Suggest,
    /// Auto-approve edits, prompt for new files
    AutoEdit,
    /// Auto-approve everything
    FullAuto,
}

impl Default for ApprovalMode {
    fn default() -> Self {
        ApprovalMode::Suggest
    }
}

impl ApprovalMode {
    pub fn from_str(s: &str) -> Result<Self, &'static str> {
        match s.to_lowercase().as_str() {
            "suggest" => Ok(ApprovalMode::Suggest),
            "auto-edit" => Ok(ApprovalMode::AutoEdit),
            "full-auto" => Ok(ApprovalMode::FullAuto),
            _ => Err("Invalid approval mode. Use: suggest, auto-edit, full-auto"),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalMode::Suggest => "suggest",
            ApprovalMode::AutoEdit => "auto-edit",
            ApprovalMode::FullAuto => "full-auto",
        }
    }

    pub fn should_prompt_for_new_file(&self) -> bool {
        match self {
            ApprovalMode::Suggest => true,
            ApprovalMode::AutoEdit => true,
            ApprovalMode::FullAuto => false,
        }
    }

    pub fn should_prompt_for_edit(&self) -> bool {
        match self {
            ApprovalMode::Suggest => true,
            ApprovalMode::AutoEdit => false,
            ApprovalMode::FullAuto => false,
        }
    }
}