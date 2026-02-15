pub mod console;
pub mod create;
pub mod destroy;
pub mod down;
pub mod image;
pub mod list;
pub mod log;
pub mod provision_cmd;
pub mod reload;
pub mod ssh;
pub mod start;
pub mod state;
pub mod status;
pub mod stop;
pub mod up;

use clap::{Parser, Subcommand};
use miette::Result;
use vm_manager::{NetworkConfig, VmHandle};

#[derive(Parser)]
#[command(name = "vmctl", about = "Manage virtual machines", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new VM (and optionally start it)
    Create(create::CreateArgs),
    /// Start an existing VM
    Start(start::StartArgs),
    /// Stop a running VM
    Stop(stop::StopArgs),
    /// Destroy a VM and clean up all resources
    Destroy(destroy::DestroyArgs),
    /// List all VMs
    List(list::ListArgs),
    /// Show VM status
    Status(status::StatusArgs),
    /// Attach to a VM's serial console
    Console(console::ConsoleArgs),
    /// SSH into a VM
    Ssh(ssh::SshArgs),
    /// Suspend a running VM (pause vCPUs)
    Suspend(start::SuspendArgs),
    /// Resume a suspended VM
    Resume(start::ResumeArgs),
    /// Manage VM images
    Image(image::ImageCommand),
    /// Bring up VMs defined in VMFile.kdl
    Up(up::UpArgs),
    /// Bring down VMs defined in VMFile.kdl
    Down(down::DownArgs),
    /// Destroy and recreate VMs defined in VMFile.kdl
    Reload(reload::ReloadArgs),
    /// Re-run provisioners on running VMs from VMFile.kdl
    Provision(provision_cmd::ProvisionArgs),
    /// Show VM console and provision logs
    Log(log::LogArgs),
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Command::Create(args) => create::run(args).await,
            Command::Start(args) => start::run_start(args).await,
            Command::Stop(args) => stop::run(args).await,
            Command::Destroy(args) => destroy::run(args).await,
            Command::List(args) => list::run(args).await,
            Command::Status(args) => status::run(args).await,
            Command::Console(args) => console::run(args).await,
            Command::Ssh(args) => ssh::run(args).await,
            Command::Suspend(args) => start::run_suspend(args).await,
            Command::Resume(args) => start::run_resume(args).await,
            Command::Image(args) => image::run(args).await,
            Command::Up(args) => up::run(args).await,
            Command::Down(args) => down::run(args).await,
            Command::Reload(args) => reload::run(args).await,
            Command::Provision(args) => provision_cmd::run(args).await,
            Command::Log(args) => log::run(args).await,
        }
    }
}

/// Determine the SSH port for a VM handle: use the forwarded host port for user-mode networking,
/// or 22 for all other network types.
fn ssh_port_for_handle(handle: &VmHandle) -> u16 {
    match handle.network {
        NetworkConfig::User => handle.ssh_host_port.unwrap_or(22),
        _ => 22,
    }
}

/// Well-known filename for a generated SSH private key, stored in the VM's work directory.
const GENERATED_KEY_FILE: &str = "id_ed25519_generated";

/// Persist a generated SSH private key PEM to the VM's work directory (if present).
///
/// The file is written with 0600 permissions so that OpenSSH accepts it.
async fn save_generated_ssh_key(
    spec: &vm_manager::VmSpec,
    handle: &VmHandle,
) -> miette::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(ref ssh) = spec.ssh {
        if let Some(ref pem) = ssh.private_key_pem {
            let key_path = handle.work_dir.join(GENERATED_KEY_FILE);
            tokio::fs::write(&key_path, pem)
                .await
                .map_err(|e| miette::miette!("failed to save generated SSH key: {e}"))?;
            tokio::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                .await
                .map_err(|e| miette::miette!("failed to set SSH key permissions: {e}"))?;
        }
    }
    Ok(())
}

/// Build an `SshConfig` from a VMFile ssh block and (optionally) a persisted generated key.
///
/// If the ssh block specifies `private-key`, use that file. Otherwise, look for a previously
/// generated key in the VM's work directory (written during `vmctl up`).
fn build_ssh_config(
    ssh_def: &vm_manager::vmfile::SshDef,
    base_dir: &std::path::Path,
    handle: &VmHandle,
) -> miette::Result<vm_manager::SshConfig> {
    if let Some(ref key_path) = ssh_def.private_key {
        return Ok(vm_manager::SshConfig {
            user: ssh_def.user.clone(),
            public_key: None,
            private_key_path: Some(vm_manager::vmfile::resolve_path(key_path, base_dir)),
            private_key_pem: None,
        });
    }

    // Look for a generated key in the VM's work directory
    let gen_key_path = handle.work_dir.join(GENERATED_KEY_FILE);
    if gen_key_path.exists() {
        let pem = std::fs::read_to_string(&gen_key_path).map_err(|e| {
            miette::miette!(
                "cannot read generated SSH key at {}: {e}",
                gen_key_path.display()
            )
        })?;
        Ok(vm_manager::SshConfig {
            user: ssh_def.user.clone(),
            public_key: None,
            private_key_path: None,
            private_key_pem: Some(pem),
        })
    } else {
        Err(miette::miette!(
            "no SSH private-key configured and no generated key found for VM — run `vmctl up` first"
        ))
    }
}
