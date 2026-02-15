use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use miette::{IntoDiagnostic, Result};
use vm_manager::{Hypervisor, RouterHypervisor, VmState};

use super::state;

#[derive(Args)]
pub struct ProvisionArgs {
    /// Path to VMFile.kdl
    #[arg(long)]
    file: Option<PathBuf>,

    /// Only provision a specific VM by name
    #[arg(long)]
    name: Option<String>,
}

pub async fn run(args: ProvisionArgs) -> Result<()> {
    let path = vm_manager::vmfile::discover(args.file.as_deref()).into_diagnostic()?;
    let vmfile = vm_manager::vmfile::parse(&path).into_diagnostic()?;

    let store = state::load_store().await?;
    let hv = RouterHypervisor::new(None, None);

    for def in &vmfile.vms {
        if let Some(ref filter) = args.name {
            if &def.name != filter {
                continue;
            }
        }

        if def.provisions.is_empty() {
            println!("VM '{}' has no provisioners — skipping", def.name);
            continue;
        }

        let handle = store.get(&def.name).ok_or_else(|| {
            miette::miette!(
                "VM '{}' not found in store — run `vmctl up` first",
                def.name
            )
        })?;

        let state = hv.state(handle).await.into_diagnostic()?;
        if state != VmState::Running {
            miette::bail!(
                "VM '{}' is not running (state: {state}) — start it first",
                def.name
            );
        }

        let ssh_def = def.ssh.as_ref().ok_or_else(|| {
            miette::miette!(
                "VM '{}' has provisioners but no ssh block — add an ssh {{ }} section to VMFile.kdl",
                def.name
            )
        })?;

        let ip = hv.guest_ip(handle).await.into_diagnostic()?;
        let port = super::ssh_port_for_handle(handle);

        let config = super::build_ssh_config(ssh_def, &vmfile.base_dir, handle)?;

        println!("Provisioning VM '{}'...", def.name);
        let sess =
            vm_manager::ssh::connect_with_retry(&ip, port, &config, Duration::from_secs(120))
                .await
                .into_diagnostic()?;

        let provisions = def.provisions.clone();
        let base_dir = vmfile.base_dir.clone();
        let name = def.name.clone();
        let log_dir = handle.work_dir.clone();
        tokio::task::spawn_blocking(move || {
            vm_manager::provision::run_provisions(
                &sess,
                &provisions,
                &base_dir,
                &name,
                Some(&log_dir),
            )
        })
        .await
        .into_diagnostic()?
        .into_diagnostic()?;

        println!("VM '{}' provisioned", def.name);
    }

    Ok(())
}
