use crate::apply_patch::{assess_patch_safety, PatchAssessment, SafetyDecision};

#[derive(Debug, Clone)]
pub struct ApplyPatchInput {
    pub patch: String,
}

#[derive(Debug, Clone)]
pub struct ApplyPatchResult {
    pub assessment: PatchAssessment,
    pub applied: bool,
}

pub fn tool_apply_patch(input: ApplyPatchInput, workspace_write: bool) -> ApplyPatchResult {
    let assessment = assess_patch_safety(&input.patch, workspace_write);
    // If auto-approved, attempt to apply immediately using slide-apply-patch crate
    let mut applied = false;
    if matches!(assessment.decision, SafetyDecision::AutoApprove) {
        if let Ok(()) = slide_apply_patch::apply_patch_to_files(&input.patch, false) {
            applied = true;
        }
    }
    ApplyPatchResult { assessment, applied }
}
