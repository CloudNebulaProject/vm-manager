use clap::Parser;
use miette::Result;
use tracing_subscriber::EnvFilter;

mod commands;
use commands::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing: compact format, no timestamps, no targets
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .without_time()
        .with_target(false)
        .init();

    let cli = Cli::parse();
    cli.run().await
}
