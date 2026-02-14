use std::time::Duration;

use clap::Args;
use miette::{IntoDiagnostic, Result};
use vm_manager::{Hypervisor, RouterHypervisor};

use super::state;

#[derive(Args)]
pub struct StopArgs {
    /// VM name
    name: String,

    /// Graceful shutdown timeout in seconds
    #[arg(long, default_value = "30")]
    timeout: u64,
}

pub async fn run(args: StopArgs) -> Result<()> {
    let store = state::load_store().await?;
    let handle = store
        .get(&args.name)
        .ok_or_else(|| miette::miette!("VM '{}' not found", args.name))?;

    let hv = RouterHypervisor::new(None, None);
    hv.stop(handle, Duration::from_secs(args.timeout))
        .await
        .into_diagnostic()?;
    println!("VM '{}' stopped", args.name);
    Ok(())
}
