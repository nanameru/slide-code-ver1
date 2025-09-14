use clap::Parser;
use std::num::NonZero;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
pub struct Cli {
    /// Search pattern for files
    pub pattern: Option<String>,

    /// Maximum number of matches to return
    #[arg(long, default_value = "100")]
    pub limit: NonZero<usize>,

    /// Working directory to search in
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Compute indices for highlighting
    #[arg(long)]
    pub compute_indices: bool,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Exclude patterns
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Number of threads
    #[arg(long, default_value = "4")]
    pub threads: NonZero<usize>,
}