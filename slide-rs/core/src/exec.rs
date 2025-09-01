use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub async fn exec_command(_cmd: &str, _sandbox: bool, _network: bool) -> Result<ExecResult> {
    // Stub: not actually executing to keep core safe here.
    bail!("exec not implemented in core stub")
}

