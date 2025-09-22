/// Minimal settings persistence for model selection.
pub fn save_model(model: &str) {
    // For now, store in env var for current session; extend to config file if needed.
    std::env::set_var("SLIDE_MODEL", model);
}

pub fn current_model() -> Option<String> {
    std::env::var("SLIDE_MODEL").ok()
}

use slide_core::protocol::ReasoningEffort as ReasoningEffortConfig;

pub fn save_effort(effort: Option<ReasoningEffortConfig>) {
    if let Some(e) = effort {
        std::env::set_var("SLIDE_REASONING_EFFORT", e.to_string());
    } else {
        // Clear to default
        std::env::remove_var("SLIDE_REASONING_EFFORT");
    }
}

pub fn current_effort() -> Option<ReasoningEffortConfig> {
    if let Ok(s) = std::env::var("SLIDE_REASONING_EFFORT") {
        match s.as_str() {
            "minimal" => Some(ReasoningEffortConfig::Minimal),
            "low" => Some(ReasoningEffortConfig::Low),
            "medium" => Some(ReasoningEffortConfig::Medium),
            "high" => Some(ReasoningEffortConfig::High),
            _ => None,
        }
    } else {
        None
    }
}


