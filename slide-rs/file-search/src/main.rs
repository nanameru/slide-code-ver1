use clap::Parser;
use slide_file_search::{run_main, Cli, FileMatch, Reporter};

struct JsonReporter;

impl Reporter for JsonReporter {
    fn report_match(&self, file_match: &FileMatch) {
        println!("{}", serde_json::to_string(file_match).unwrap());
    }
    fn warn_matches_truncated(&self, total_match_count: usize, shown_match_count: usize) {
        eprintln!("Warning: showing {shown_match_count} of {total_match_count} matches");
    }
    fn warn_no_search_pattern(&self, search_directory: &std::path::Path) {
        eprintln!(
            "No pattern provided; listing files in {}",
            search_directory.display()
        );
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run_main(cli, JsonReporter).await
}
