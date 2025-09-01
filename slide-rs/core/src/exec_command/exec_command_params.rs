#[derive(Debug, Clone)]
pub struct ExecCommandParams {
    pub cmd: String,
    pub yield_time_ms: u64,
}

