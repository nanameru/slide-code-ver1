use crate::safety_impl::SandboxPolicy;
use std::path::PathBuf;

/// Build a sandbox policy configuration string
pub fn build_seatbelt_policy(policy: SandboxPolicy, workspace_root: Option<&PathBuf>) -> String {
    match policy {
        SandboxPolicy::ReadOnly => build_readonly_policy(),
        SandboxPolicy::WorkspaceWrite => build_workspace_write_policy(workspace_root),
        SandboxPolicy::DangerFullAccess => build_full_access_policy(),
    }
}

fn build_readonly_policy() -> String {
    r#"(version 1)
(allow default)
(deny file-write*)
(deny process-exec*)
(deny network*)
(allow file-read*)
(allow process-info*)
(allow system-info)
"#.to_string()
}

fn build_workspace_write_policy(workspace_root: Option<&PathBuf>) -> String {
    let workspace_path = workspace_root
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/tmp/slide-workspace".to_string());
        
    format!(r#"(version 1)
(allow default)
(deny file-write*)
(deny process-exec*)
(deny network*)
(allow file-read*)
(allow file-write* (subpath "{}"))
(allow file-write* (subpath "/tmp/slide-"))
(allow process-info*)
(allow system-info)
(allow process-exec (literal "/bin/ls"))
(allow process-exec (literal "/bin/cat"))
(allow process-exec (literal "/bin/grep"))
(allow process-exec (literal "/bin/find"))
(allow process-exec (literal "/bin/echo"))
(allow process-exec (literal "/usr/bin/git"))
(allow process-exec (literal "/usr/local/bin/git"))
"#, workspace_path)
}

fn build_full_access_policy() -> String {
    r#"(version 1)
(allow default)
"#.to_string()
}

/// Check if sandboxing is available on this platform
pub fn is_sandboxing_available() -> bool {
    cfg!(target_os = "macos") || cfg!(target_os = "linux")
}

/// Get the appropriate sandbox command for this platform
pub fn get_sandbox_command(policy: SandboxPolicy, workspace_root: Option<&PathBuf>) -> Option<Vec<String>> {
    if !is_sandboxing_available() {
        return None;
    }
    
    #[cfg(target_os = "macos")]
    {
        let policy_content = build_seatbelt_policy(policy, workspace_root);
        Some(vec![
            "sandbox-exec".to_string(),
            "-p".to_string(),
            policy_content,
        ])
    }
    
    #[cfg(target_os = "linux")]
    {
        // Use firejail on Linux if available
        match policy {
            SandboxPolicy::ReadOnly => Some(vec![
                "firejail".to_string(),
                "--read-only=/".to_string(),
                "--private-tmp".to_string(),
                "--net=none".to_string(),
            ]),
            SandboxPolicy::WorkspaceWrite => {
                let workspace_path = workspace_root
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/tmp/slide-workspace".to_string());
                Some(vec![
                    "firejail".to_string(),
                    "--read-only=/".to_string(),
                    format!("--read-write={}", workspace_path),
                    "--private-tmp".to_string(),
                    "--net=none".to_string(),
                ])
            },
            SandboxPolicy::DangerFullAccess => None, // No sandboxing
        }
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Validate that sandboxing tools are available
pub fn validate_sandbox_tools() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use which::which;
        which("sandbox-exec").map_err(|_| "sandbox-exec not found on macOS".to_string())?;
        Ok(())
    }
    
    #[cfg(target_os = "linux")]
    {
        use which::which;
        which("firejail").map_err(|_| "firejail not found on Linux. Install with: sudo apt install firejail".to_string())?;
        Ok(())
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err("Sandboxing not supported on this platform".to_string())
    }
}

#[derive(Debug)]
pub struct SandboxConfig {
    pub policy: SandboxPolicy,
    pub workspace_root: Option<PathBuf>,
    pub temp_dir: Option<PathBuf>,
    pub network_access: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            policy: SandboxPolicy::ReadOnly,
            workspace_root: None,
            temp_dir: None,
            network_access: false,
        }
    }
}

impl SandboxConfig {
    pub fn new(policy: SandboxPolicy) -> Self {
        Self {
            policy,
            ..Default::default()
        }
    }
    
    pub fn with_workspace<P: Into<PathBuf>>(mut self, workspace: P) -> Self {
        self.workspace_root = Some(workspace.into());
        self
    }
    
    pub fn with_temp_dir<P: Into<PathBuf>>(mut self, temp_dir: P) -> Self {
        self.temp_dir = Some(temp_dir.into());
        self
    }
    
    pub fn with_network_access(mut self, network_access: bool) -> Self {
        self.network_access = network_access;
        self
    }
    
    pub fn build_command(&self) -> Option<Vec<String>> {
        get_sandbox_command(self.policy, self.workspace_root.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readonly_policy() {
        let policy = build_readonly_policy();
        assert!(policy.contains("(deny file-write*)"));
        assert!(policy.contains("(allow file-read*)"));
    }

    #[test]
    fn test_workspace_write_policy() {
        let workspace = PathBuf::from("/test/workspace");
        let policy = build_workspace_write_policy(Some(&workspace));
        assert!(policy.contains("/test/workspace"));
        assert!(policy.contains("(allow file-write*"));
    }

    #[test]
    fn test_sandbox_config() {
        let config = SandboxConfig::new(SandboxPolicy::WorkspaceWrite)
            .with_workspace("/test")
            .with_network_access(false);
            
        assert_eq!(config.policy, SandboxPolicy::WorkspaceWrite);
        assert_eq!(config.workspace_root, Some(PathBuf::from("/test")));
        assert!(!config.network_access);
    }
}