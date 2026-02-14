use clap::Args;
use miette::{IntoDiagnostic, Result};
use vm_manager::{Hypervisor, RouterHypervisor};

use super::state;

#[derive(Args)]
pub struct DestroyArgs {
    /// VM name
    name: String,
}

pub async fn run(args: DestroyArgs) -> Result<()> {
    let mut store = state::load_store().await?;
    let handle = store
        .remove(&args.name)
        .ok_or_else(|| miette::miette!("VM '{}' not found", args.name))?;

    let hv = RouterHypervisor::new(None, None);
    hv.destroy(handle).await.into_diagnostic()?;

    state::save_store(&store).await?;
    println!("VM '{}' destroyed", args.name);
    Ok(())
}
