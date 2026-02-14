use clap::Args;
use miette::{IntoDiagnostic, Result};
use vm_manager::{Hypervisor, RouterHypervisor};

use super::state;

#[derive(Args)]
pub struct StatusArgs {
    /// VM name
    name: String,
}

pub async fn run(args: StatusArgs) -> Result<()> {
    let store = state::load_store().await?;
    let handle = store
        .get(&args.name)
        .ok_or_else(|| miette::miette!("VM '{}' not found", args.name))?;

    let hv = RouterHypervisor::new(None, None);
    let state = hv.state(handle).await.into_diagnostic()?;

    println!("Name:    {}", handle.name);
    println!("ID:      {}", handle.id);
    println!("Backend: {}", handle.backend);
    println!("State:   {}", state);
    println!("WorkDir: {}", handle.work_dir.display());

    if let Some(ref overlay) = handle.overlay_path {
        println!("Overlay: {}", overlay.display());
    }
    if let Some(ref seed) = handle.seed_iso_path {
        println!("Seed:    {}", seed.display());
    }
    if let Some(pid) = handle.pid {
        println!("PID:     {}", pid);
    }
    if let Some(ref vnc) = handle.vnc_addr {
        println!("VNC:     {}", vnc);
    }

    Ok(())
}
