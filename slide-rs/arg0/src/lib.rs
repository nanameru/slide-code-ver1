use std::future::Future;
use std::path::PathBuf;

/// arg0 dispatch function - simplified version for MVP
pub fn arg0_dispatch_or_else<F, Fut>(main_fn: F) -> !
where
    F: FnOnce(Option<PathBuf>) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    // Create Tokio runtime
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create Tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    // Run the main function
    let result = runtime.block_on(main_fn(None));
    
    match result {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}