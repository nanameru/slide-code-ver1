use clap::Parser;
use std::num::NonZeroUsize;
use std::path::PathBuf;

#[derive(Debug, Parser, Clone)]
#[command(name = "slide-file-search")]
#[command(about = "Fuzzy file search (ripgrep walker + nucleo-matcher)")]
pub struct Cli {
    /// Search pattern (fuzzy)
    pub pattern: Option<String>,

    /// Maximum number of results to return
    #[arg(long, default_value_t = NonZeroUsize::new(100).unwrap())]
    pub limit: NonZeroUsize,

    /// Working directory to search from
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Compute match indices for highlighting
    #[arg(long, default_value_t = false)]
    pub compute_indices: bool,

    /// Output JSON (reserved for CLI)
    #[arg(long, default_value_t = true)]
    pub json: bool,

    /// Exclude patterns (gitignore style). Prefix with ! for exclude semantics.
    #[arg(long, value_delimiter = ',')]
    pub exclude: Vec<String>,

    /// Number of worker threads
    #[arg(long, default_value_t = NonZeroUsize::new(4).unwrap())]
    pub threads: NonZeroUsize,
}
