#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxPolicy {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

pub fn build_seatbelt_policy(_policy: SandboxPolicy) -> String {
    // Returns a placeholder sbpl policy text.
    "(version 1)".into()
}

