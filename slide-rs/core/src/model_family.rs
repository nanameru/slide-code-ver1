// Temporary simplified model family for slide-code-test
// To be fully implemented based on codex-1

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelFamily {
    pub slug: String,
    pub family: String,
    pub needs_special_apply_patch_instructions: bool,
    pub supports_reasoning_summaries: bool,
    pub uses_local_shell_tool: bool,
    pub apply_patch_tool_type: Option<crate::tool_apply_patch::ApplyPatchToolType>,
    pub base_instructions: String,
}

// Simplified implementation for now
pub fn find_family_for_model(slug: &str) -> Option<ModelFamily> {
    Some(ModelFamily {
        slug: slug.to_string(),
        family: slug.to_string(),
        needs_special_apply_patch_instructions: true,
        supports_reasoning_summaries: false,
        uses_local_shell_tool: false,
        apply_patch_tool_type: Some(crate::tool_apply_patch::ApplyPatchToolType::Freeform),
        base_instructions: "You are a helpful assistant.".to_string(),
    })
}

pub fn derive_default_model_family(model: &str) -> ModelFamily {
    ModelFamily {
        slug: model.to_string(),
        family: model.to_string(),
        needs_special_apply_patch_instructions: false,
        supports_reasoning_summaries: false,
        uses_local_shell_tool: false,
        apply_patch_tool_type: None,
        base_instructions: "You are a helpful assistant.".to_string(),
    }
}