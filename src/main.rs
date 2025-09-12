use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    slide_cli::run_cli().await
}