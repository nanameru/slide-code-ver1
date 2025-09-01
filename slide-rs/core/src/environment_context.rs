use crate::seatbelt::SandboxPolicy;

pub fn build_environment_context(cwd: &str, approval_policy: &str, sandbox: SandboxPolicy, network: bool) -> String {
    format!(
        "<environment_context>\n  <cwd>{}</cwd>\n  <approval_policy>{}</approval_policy>\n  <sandbox_policy>{:?}</sandbox_policy>\n  <network_access>{}</network_access>\n</environment_context>",
        cwd,
        approval_policy,
        sandbox,
        if network { "enabled" } else { "restricted" }
    )
}

