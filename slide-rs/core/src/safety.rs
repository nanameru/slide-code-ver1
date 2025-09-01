use crate::apply_patch::SafetyDecision;

pub fn decide_command_safety(command: &str, network_allowed: bool) -> SafetyDecision {
    let cmd = command.trim();
    let safe = super::is_safe_command::is_known_safe(cmd) && !cmd.contains(" rm ");
    if safe && !network_allowed {
        SafetyDecision::AutoApprove
    } else if safe {
        SafetyDecision::AskUser
    } else {
        SafetyDecision::AskUser
    }
}

