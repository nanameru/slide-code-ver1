use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tracing;

use crate::bash_parser::is_safe_bash_command;
use crate::safety_impl::{SandboxPolicy, SafetyCheck};
use crate::seatbelt::SandboxConfig;

#[derive(Debug, Clone)]
pub struct SpawnConfig {
    pub working_dir: Option<PathBuf>,
    pub environment: HashMap<String, String>,
    pub sandbox: bool,
    pub network_access: bool,
    pub sandbox_policy: SandboxPolicy,
    pub inherit_stdio: bool,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            working_dir: None,
            environment: HashMap::new(),
            sandbox: true,
            network_access: false,
            sandbox_policy: SandboxPolicy::ReadOnly,
            inherit_stdio: false,
        }
    }
}

#[derive(Debug)]
pub struct ProcessHandle {
    pub child: Child,
    pub stdout: Option<mpsc::Receiver<String>>,
    pub stderr: Option<mpsc::Receiver<String>>,
}

impl ProcessHandle {
    pub async fn wait(mut self) -> Result<i32> {
        let status = self.child.wait().await?;
        Ok(status.code().unwrap_or(-1))
    }
    
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill_on_drop(true);
        Ok(())
    }
    
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }
}

/// Spawn a process with comprehensive safety and monitoring
pub async fn spawn_process_safe(cmd: &str, config: SpawnConfig) -> Result<ProcessHandle> {
    tracing::info!("Spawning process: {}", cmd);
    
    // Safety validation
    if !is_safe_bash_command(cmd) {
        bail!("Command failed safety analysis: {}", cmd);
    }

    let mut command = prepare_spawn_command(cmd, &config).await?;
    
    // Configure stdio based on needs
    if config.inherit_stdio {
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());
        command.stdin(Stdio::inherit());
    } else {
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(Stdio::null());
    }

    // Spawn the process
    let mut child = command.spawn().context("Failed to spawn process")?;
    
    // Set up output streaming if not inheriting stdio
    let (stdout_rx, stderr_rx) = if !config.inherit_stdio {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        
        let stdout_rx = if let Some(stdout) = stdout {
            Some(setup_output_stream(stdout).await)
        } else {
            None
        };
        
        let stderr_rx = if let Some(stderr) = stderr {
            Some(setup_output_stream(stderr).await)
        } else {
            None
        };
        
        (stdout_rx, stderr_rx)
    } else {
        (None, None)
    };

    let handle = ProcessHandle {
        child,
        stdout: stdout_rx,
        stderr: stderr_rx,
    };

    tracing::info!("Process spawned successfully with ID: {:?}", handle.id());
    Ok(handle)
}

async fn prepare_spawn_command(cmd: &str, config: &SpawnConfig) -> Result<Command> {
    let mut command;

    if config.sandbox {
        command = prepare_sandboxed_spawn(cmd, config).await?;
    } else {
        command = Command::new("sh");
        command.arg("-c").arg(cmd);
    }

    // Set working directory
    if let Some(ref cwd) = config.working_dir {
        command.current_dir(cwd);
    }

    // Set environment variables
    for (key, value) in &config.environment {
        command.env(key, value);
    }

    Ok(command)
}

async fn prepare_sandboxed_spawn(cmd: &str, config: &SpawnConfig) -> Result<Command> {
    let sandbox_config = SandboxConfig::new(config.sandbox_policy)
        .with_workspace(config.working_dir.clone().unwrap_or_else(|| PathBuf::from(".")))
        .with_network_access(config.network_access);

    if let Some(sandbox_args) = sandbox_config.build_command() {
        let mut command = Command::new(&sandbox_args[0]);
        
        // Add sandbox arguments
        for arg in &sandbox_args[1..] {
            command.arg(arg);
        }
        
        // Add the actual command
        command.arg("--");
        command.arg("sh");
        command.arg("-c");
        command.arg(cmd);

        Ok(command)
    } else {
        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);
        Ok(command)
    }
}

async fn setup_output_stream(
    stream: tokio::process::ChildStdout,
) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel(1024);
    
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        
        let reader = BufReader::new(stream);
        let mut lines = reader.lines();
        
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(line).await.is_err() {
                break;
            }
        }
    });
    
    rx
}

/// Spawn a background daemon process
pub async fn spawn_daemon(cmd: &str, config: SpawnConfig) -> Result<u32> {
    tracing::info!("Spawning daemon process: {}", cmd);
    
    let mut spawn_config = config;
    spawn_config.inherit_stdio = false;
    
    let handle = spawn_process_safe(cmd, spawn_config).await?;
    let pid = handle.id().context("Failed to get process ID")?;
    
    // Detach the process (don't wait for it)
    std::mem::forget(handle);
    
    tracing::info!("Daemon spawned with PID: {}", pid);
    Ok(pid)
}

/// Kill a process by PID
pub async fn kill_process(pid: u32) -> Result<()> {
    tracing::info!("Killing process with PID: {}", pid);
    
    #[cfg(unix)]
    {
        use nix::sys::signal::{self, Signal};
        use nix::unistd::Pid;
        
        let pid = Pid::from_raw(pid as i32);
        signal::kill(pid, Signal::SIGTERM)
            .context("Failed to send SIGTERM")?;
        
        // Wait a bit, then force kill if necessary
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        
        // Check if process is still alive
        if signal::kill(pid, None).is_ok() {
            signal::kill(pid, Signal::SIGKILL)
                .context("Failed to send SIGKILL")?;
        }
    }
    
    #[cfg(windows)]
    {
        // Windows implementation would go here
        bail!("Process killing not implemented for Windows");
    }
    
    tracing::info!("Process {} killed successfully", pid);
    Ok(())
}

/// Check if a process is running
pub fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::{self, Signal};
        use nix::unistd::Pid;
        
        let pid = Pid::from_raw(pid as i32);
        signal::kill(pid, None).is_ok()
    }
    
    #[cfg(windows)]
    {
        // Windows implementation would go here
        false
    }
}

/// Get system process information
pub fn get_process_info(pid: u32) -> Option<ProcessInfo> {
    #[cfg(unix)]
    {
        // Read from /proc/PID/stat on Linux
        let stat_path = format!("/proc/{}/stat", pid);
        if let Ok(content) = std::fs::read_to_string(&stat_path) {
            let parts: Vec<&str> = content.split_whitespace().collect();
            if parts.len() >= 3 {
                return Some(ProcessInfo {
                    pid,
                    name: parts[1].trim_matches('(').trim_matches(')').to_string(),
                    state: parts[2].chars().next().unwrap_or('?'),
                });
            }
        }
    }
    
    None
}

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub state: char,
}

/// Process manager for tracking spawned processes
#[derive(Debug, Default)]
pub struct ProcessManager {
    processes: HashMap<u32, ProcessInfo>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn register_process(&mut self, pid: u32, name: String) {
        self.processes.insert(pid, ProcessInfo {
            pid,
            name,
            state: 'R',
        });
    }
    
    pub fn unregister_process(&mut self, pid: u32) {
        self.processes.remove(&pid);
    }
    
    pub fn list_processes(&self) -> Vec<&ProcessInfo> {
        self.processes.values().collect()
    }
    
    pub async fn cleanup_dead_processes(&mut self) {
        let mut to_remove = Vec::new();
        
        for pid in self.processes.keys() {
            if !is_process_running(*pid) {
                to_remove.push(*pid);
            }
        }
        
        for pid in to_remove {
            self.unregister_process(pid);
            tracing::info!("Cleaned up dead process: {}", pid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_spawn_simple_command() {
        let config = SpawnConfig::default();
        let mut handle = spawn_process_safe("echo 'test'", config).await.unwrap();
        
        let status = handle.wait().await.unwrap();
        assert_eq!(status, 0);
    }

    #[tokio::test]
    async fn test_spawn_with_output_streaming() {
        let config = SpawnConfig::default();
        let mut handle = spawn_process_safe("echo 'line1'; echo 'line2'", config).await.unwrap();
        
        if let Some(mut stdout) = handle.stdout.take() {
            let mut lines = Vec::new();
            while let Some(line) = stdout.recv().await {
                lines.push(line);
                if lines.len() >= 2 {
                    break;
                }
            }
            assert_eq!(lines.len(), 2);
        }
        
        let _ = handle.wait().await;
    }

    #[test]
    fn test_process_manager() {
        let mut manager = ProcessManager::new();
        
        manager.register_process(1234, "test_process".to_string());
        assert_eq!(manager.list_processes().len(), 1);
        
        manager.unregister_process(1234);
        assert_eq!(manager.list_processes().len(), 0);
    }
}