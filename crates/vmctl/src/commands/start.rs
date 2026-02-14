use clap::Args;
use miette::{IntoDiagnostic, Result};
use vm_manager::{Hypervisor, RouterHypervisor};

use super::state;

#[derive(Args)]
pub struct StartArgs {
    /// VM name
    name: String,
}

pub async fn run_start(args: StartArgs) -> Result<()> {
    let store = state::load_store().await?;
    let handle = store.get(&args.name).ok_or_else(|| {
        miette::miette!(
            "VM '{}' not found — run `vmctl list` to see available VMs",
            args.name
        )
    })?;

    let hv = RouterHypervisor::new(None, None);
    hv.start(handle).await.into_diagnostic()?;
    println!("VM '{}' started", args.name);
    Ok(())
}

#[derive(Args)]
pub struct SuspendArgs {
    /// VM name
    name: String,
}

pub async fn run_suspend(args: SuspendArgs) -> Result<()> {
    let store = state::load_store().await?;
    let handle = store
        .get(&args.name)
        .ok_or_else(|| miette::miette!("VM '{}' not found", args.name))?;

    let hv = RouterHypervisor::new(None, None);
    hv.suspend(handle).await.into_diagnostic()?;
    println!("VM '{}' suspended", args.name);
    Ok(())
}

#[derive(Args)]
pub struct ResumeArgs {
    /// VM name
    name: String,
}

pub async fn run_resume(args: ResumeArgs) -> Result<()> {
    let store = state::load_store().await?;
    let handle = store
        .get(&args.name)
        .ok_or_else(|| miette::miette!("VM '{}' not found", args.name))?;

    let hv = RouterHypervisor::new(None, None);
    hv.resume(handle).await.into_diagnostic()?;
    println!("VM '{}' resumed", args.name);
    Ok(())
}
