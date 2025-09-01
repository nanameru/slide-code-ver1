#[derive(Debug, Clone)]
pub struct LandlockPolicy {
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub allow_network: bool,
}

impl Default for LandlockPolicy {
    fn default() -> Self {
        Self { read_paths: vec![], write_paths: vec![], allow_network: false }
    }
}

