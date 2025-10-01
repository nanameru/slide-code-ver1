use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Child;

use protocol::protocol::SandboxPolicy;
use crate::spawn::{spawn_child_async, StdioPolicy};

/// Spawn a shell tool command under the Linux Landlock+seccomp sandbox helper
/// (slide-linux-sandbox).
///
/// Unlike macOS Seatbelt where we directly embed the policy text, the Linux
/// helper accepts a JSON-serialized SandboxPolicy and the sandbox CWD.
/// 
/// Reference: codex-1/codex-rs/core/src/landlock.rs
pub async fn spawn_command_under_linux_sandbox<P>(
    linux_sandbox_exe: P,
    command: Vec<String>,
    command_cwd: PathBuf,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
    stdio_policy: StdioPolicy,
    env: HashMap<String, String>,
) -> std::io::Result<Child>
where
    P: AsRef<Path>,
{
    let args = create_linux_sandbox_command_args(command, sandbox_policy, sandbox_policy_cwd);
    let arg0 = Some("slide-linux-sandbox");
    
    spawn_child_async(
        linux_sandbox_exe.as_ref().to_path_buf(),
        args,
        arg0,
        command_cwd,
        sandbox_policy,
        stdio_policy,
        env,
    )
    .await
}

/// Converts the sandbox policy into the CLI invocation for `slide-linux-sandbox`.
/// 
/// The helper binary expects:
/// <sandbox_policy_cwd> <sandbox_policy_json> -- <command...>
fn create_linux_sandbox_command_args(
    command: Vec<String>,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
) -> Vec<String> {
    let sandbox_policy_cwd = sandbox_policy_cwd
        .to_str()
        .expect("cwd must be valid UTF-8")
        .to_string();

    let sandbox_policy_json =
        serde_json::to_string(sandbox_policy).expect("Failed to serialize SandboxPolicy to JSON");

    let mut linux_cmd: Vec<String> = vec![
        sandbox_policy_cwd,
        sandbox_policy_json,
        // Separator so that command arguments starting with `-` are not parsed as
        // options of the helper itself.
        "--".to_string(),
    ];

    // Append the original tool command.
    linux_cmd.extend(command);

    linux_cmd
}
