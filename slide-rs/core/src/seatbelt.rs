// Re-export from protocol to maintain consistency
pub use protocol::protocol::{SandboxPolicy, WritableRoot};

// Default and methods are now provided by protocol crate

pub fn build_seatbelt_policy(policy: SandboxPolicy, cwd: &std::path::Path) -> String {
    match policy {
        SandboxPolicy::DangerFullAccess => {
            // No restrictions
            "(version 1)\n(allow default)".to_string()
        }
        SandboxPolicy::ReadOnly => {
            // Read-only access
            format!(
                r#"(version 1)
(deny default)
(allow file-read*)
(allow process-info*)
(allow sysctl-read)
(allow mach-lookup)
"#
            )
        }
        SandboxPolicy::WorkspaceWrite {
            network_access,
            ..
        } => {
            let mut policy_str = format!(
                r#"(version 1)
(deny default)
(allow file-read*)
(allow process-info*)
(allow sysctl-read)
(allow mach-lookup)
"#
            );

            // Get writable roots using the unified method
            let writable_roots = policy.get_writable_roots_with_cwd(cwd);
            
            // Add write access to all writable roots
            for root in writable_roots {
                policy_str.push_str(&format!(
                    "(allow file-write* (subpath \"{}\"))\n",
                    root.root.display()
                ));
            }

            if network_access {
                policy_str.push_str("(allow network*)\n");
            }

            policy_str
        }
    }
}
