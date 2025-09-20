use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MACOS_SEATBELT_BASE_POLICY: &str = include_str!("seatbelt_base_policy.sbpl");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxPolicy {
    /// No restrictions whatsoever. Use with caution.
    #[serde(rename = "danger-full-access")]
    DangerFullAccess,
    /// Read-only access to the entire file-system.
    #[serde(rename = "read-only")]
    ReadOnly,
    /// Same as `ReadOnly` but additionally grants write access to the current
    /// working directory ("workspace").
    #[serde(rename = "workspace-write")]
    WorkspaceWrite {
        /// Additional folders (beyond cwd and possibly TMPDIR) that should be
        /// writable from within the sandbox.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        writable_roots: Vec<PathBuf>,
        /// When set to `true`, outbound network access is allowed. `false` by
        /// default.
        #[serde(default)]
        network_access: bool,
        /// When set to `true`, will NOT include the per-user `TMPDIR`
        /// environment variable among the default writable roots. Defaults to
        /// `false`.
        #[serde(default)]
        exclude_tmpdir_env_var: bool,
        /// When set to `true`, will NOT include the `/tmp` among the default
        /// writable roots on UNIX. Defaults to `false`.
        #[serde(default)]
        exclude_system_tmp: bool,
    },
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: Vec::new(),
            network_access: false,
            exclude_tmpdir_env_var: false,
            exclude_system_tmp: false,
        }
    }
}

impl SandboxPolicy {
    /// Get writable directories for this policy given a working directory
    pub fn get_writable_roots_with_cwd(&self, cwd: &std::path::Path) -> Vec<PathBuf> {
        match self {
            SandboxPolicy::DangerFullAccess => {
                // Full access, no restrictions
                vec![PathBuf::from("/")]
            }
            SandboxPolicy::ReadOnly => {
                // No writable roots
                Vec::new()
            }
            SandboxPolicy::WorkspaceWrite {
                writable_roots,
                exclude_tmpdir_env_var,
                exclude_system_tmp,
                ..
            } => {
                let mut roots = vec![cwd.to_path_buf()];

                // Add custom writable roots
                roots.extend(writable_roots.clone());

                // Add temp directories unless excluded
                if !exclude_tmpdir_env_var {
                    if let Ok(tmpdir) = std::env::var("TMPDIR") {
                        roots.push(PathBuf::from(tmpdir));
                    }
                }

                if !exclude_system_tmp {
                    #[cfg(unix)]
                    roots.push(PathBuf::from("/tmp"));
                    #[cfg(windows)]
                    if let Ok(temp) = std::env::var("TEMP") {
                        roots.push(PathBuf::from(temp));
                    }
                }

                roots
            }
        }
    }

    /// Check if network access is allowed
    pub fn allows_network(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ReadOnly => false,
            SandboxPolicy::WorkspaceWrite { network_access, .. } => *network_access,
        }
    }
}

pub fn build_seatbelt_policy(policy: SandboxPolicy, workspace_root: Option<&Path>) -> String {
    match policy {
        SandboxPolicy::DangerFullAccess => {
            // No restrictions
            "(version 1)\n(allow default)".to_string()
        }
        SandboxPolicy::ReadOnly => {
            let mut policy = String::from(MACOS_SEATBELT_BASE_POLICY);
            policy.push_str(
                "\n; allow read-only file operations\n(allow file-read*)\n(allow process-info*)\n(allow system-info)\n(allow mach-lookup)\n",
            );
            policy
        }
        SandboxPolicy::WorkspaceWrite {
            writable_roots,
            network_access,
            exclude_tmpdir_env_var,
            exclude_system_tmp,
        } => {
            let mut policy = String::from(MACOS_SEATBELT_BASE_POLICY);
            policy.push_str(
                "\n; allow read/write operations within workspace roots\n(allow file-read*)\n(allow process-info*)\n(allow system-info)\n(allow mach-lookup)\n",
            );

            let mut roots: Vec<PathBuf> = Vec::new();
            if let Some(root) = workspace_root {
                roots.push(root.to_path_buf());
            }
            roots.extend(writable_roots.clone());

            if !exclude_system_tmp {
                let tmp = PathBuf::from("/tmp");
                if tmp.is_dir() {
                    roots.push(tmp);
                }
            }

            if !exclude_tmpdir_env_var {
                if let Some(tmpdir) = std::env::var_os("TMPDIR") {
                    if !tmpdir.is_empty() {
                        roots.push(PathBuf::from(tmpdir));
                    }
                }
            }

            let mut seen = HashSet::new();
            for root in roots {
                let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
                if seen.insert(canonical_root.clone()) {
                    policy.push_str(&format!(
                        "(allow file-write* (subpath \"{}\"))\n",
                        canonical_root.display()
                    ));
                }
            }

            if network_access {
                policy.push_str(
                    "(allow network-outbound)\n(allow network-inbound)\n(allow system-socket)\n",
                );
            }

            policy
        }
    }
}
