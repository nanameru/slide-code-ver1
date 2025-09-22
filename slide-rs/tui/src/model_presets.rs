use slide_core::protocol::ReasoningEffort as ReasoningEffortConfig;

#[derive(Debug, Clone, Copy)]
pub struct ModelPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub model: &'static str,
    pub effort: Option<ReasoningEffortConfig>,
}

pub fn builtin_model_presets() -> Vec<ModelPreset> {
    vec![
        ModelPreset {
            id: "gpt-5-minimal",
            label: "gpt-5 minimal",
            description: "— fastest responses with limited reasoning; ideal for coding, instructions, or lightweight tasks",
            model: "gpt-5",
            effort: Some(ReasoningEffortConfig::Minimal),
        },
        ModelPreset {
            id: "gpt-5-low",
            label: "gpt-5 low",
            description: "— balances speed with some reasoning; useful for straightforward queries and short explanations",
            model: "gpt-5",
            effort: Some(ReasoningEffortConfig::Low),
        },
        ModelPreset {
            id: "gpt-5-medium",
            label: "gpt-5 medium",
            description: "— default setting; provides a solid balance of reasoning depth and latency for general-purpose tasks",
            model: "gpt-5",
            effort: Some(ReasoningEffortConfig::Medium),
        },
        ModelPreset {
            id: "gpt-5-high",
            label: "gpt-5 high",
            description: "— maximizes reasoning depth for complex or ambiguous problems",
            model: "gpt-5",
            effort: Some(ReasoningEffortConfig::High),
        },
    ]
}


