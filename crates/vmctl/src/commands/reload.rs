use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use miette::{IntoDiagnostic, Result};
use tracing::info;
use vm_manager::vmfile::{ProvisionDef, SshDef};
use vm_manager::{Hypervisor, RouterHypervisor};

use super::state;

#[derive(Args)]
pub struct ReloadArgs {
    /// Path to VMFile.kdl
    #[arg(long)]
    file: Option<PathBuf>,

    /// Only reload a specific VM by name
    #[arg(long)]
    name: Option<String>,

    /// Skip provisioning after reload
    #[arg(long)]
    no_provision: bool,
}

pub async fn run(args: ReloadArgs) -> Result<()> {
    let path = vm_manager::vmfile::discover(args.file.as_deref()).into_diagnostic()?;
    let vmfile = vm_manager::vmfile::parse(&path).into_diagnostic()?;

    let mut store = state::load_store().await?;
    let hv = RouterHypervisor::new(None, None);

    for def in &vmfile.vms {
        if let Some(ref filter) = args.name {
            if &def.name != filter {
                continue;
            }
        }

        // Destroy existing if present
        if let Some(handle) = store.remove(&def.name) {
            info!(vm = %def.name, "destroying existing VM for reload");
            hv.destroy(handle).await.into_diagnostic()?;
            state::save_store(&store).await?;
        }

        // Resolve, prepare, start
        info!(vm = %def.name, "creating and starting VM");
        let spec = vm_manager::vmfile::resolve(def, &vmfile.base_dir)
            .await
            .into_diagnostic()?;

        let handle = hv.prepare(&spec).await.into_diagnostic()?;
        super::save_generated_ssh_key(&spec, &handle).await?;
        store.insert(def.name.clone(), handle.clone());
        state::save_store(&store).await?;

        let updated = hv.start(&handle).await.into_diagnostic()?;
        store.insert(def.name.clone(), updated);
        state::save_store(&store).await?;
        println!("VM '{}' reloaded", def.name);

        // Provision
        if !args.no_provision && !def.provisions.is_empty() {
            run_provision_for_vm(
                &hv,
                &store,
                &def.name,
                &def.provisions,
                def.ssh.as_ref(),
                &vmfile.base_dir,
            )
            .await?;
        }
    }

    Ok(())
}

async fn run_provision_for_vm(
    hv: &RouterHypervisor,
    store: &state::Store,
    vm_name: &str,
    provisions: &[ProvisionDef],
    ssh_def: Option<&SshDef>,
    base_dir: &std::path::Path,
) -> Result<()> {
    let ssh_def = ssh_def.ok_or_else(|| {
        miette::miette!(
            "VM '{vm_name}' has provisioners but no ssh block — add an ssh {{ }} section to VMFile.kdl"
        )
    })?;

    let handle = store
        .get(vm_name)
        .ok_or_else(|| miette::miette!("VM '{vm_name}' not found in store"))?;

    let ip = hv.guest_ip(handle).await.into_diagnostic()?;
    let port = super::ssh_port_for_handle(handle);

    let config = super::build_ssh_config(ssh_def, base_dir, handle)?;

    println!("Provisioning VM '{vm_name}'...");
    let sess = vm_manager::ssh::connect_with_retry(&ip, port, &config, Duration::from_secs(120))
        .await
        .into_diagnostic()?;

    let provisions = provisions.to_vec();
    let base_dir = base_dir.to_path_buf();
    let name = vm_name.to_string();
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

    println!("VM '{vm_name}' provisioned");
    Ok(())
}
