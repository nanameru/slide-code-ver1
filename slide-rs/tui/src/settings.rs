/// Minimal settings persistence for model selection.
pub fn save_model(model: &str) {
    // For now, store in env var for current session; extend to config file if needed.
    std::env::set_var("SLIDE_MODEL", model);
}

pub fn current_model() -> Option<String> {
    std::env::var("SLIDE_MODEL").ok()
}


