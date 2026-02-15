use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use miette::{IntoDiagnostic, Result};
use vm_manager::{Hypervisor, NetworkConfig, RouterHypervisor, SshConfig};

use super::state;

/// SSH key filenames to try, in order of preference.
const SSH_KEY_NAMES: &[&str] = &["id_ed25519", "id_ecdsa", "id_rsa"];

#[derive(Args)]
pub struct SshArgs {
    /// VM name
    name: String,

    /// SSH user (overrides VMFile ssh block)
    #[arg(long)]
    user: Option<String>,

    /// Path to SSH private key
    #[arg(long)]
    key: Option<PathBuf>,

    /// Path to VMFile.kdl (for reading ssh user)
    #[arg(long)]
    file: Option<PathBuf>,
}

/// Find the first existing SSH key in the user's .ssh directory.
fn find_ssh_key() -> Option<PathBuf> {
    let ssh_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/root"))
        .join(".ssh");
    for name in SSH_KEY_NAMES {
        let path = ssh_dir.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Try to read the ssh user from the VMFile for the given VM name.
fn ssh_user_from_vmfile(vm_name: &str, explicit_file: Option<&std::path::Path>) -> Option<String> {
    let path = vm_manager::vmfile::discover(explicit_file).ok()?;
    let vmfile = vm_manager::vmfile::parse(&path).ok()?;
    let def = vmfile.vms.iter().find(|d| d.name == vm_name)?;
    Some(def.ssh.as_ref()?.user.clone())
}

pub async fn run(args: SshArgs) -> Result<()> {
    let store = state::load_store().await?;
    let handle = store
        .get(&args.name)
        .ok_or_else(|| miette::miette!("VM '{}' not found", args.name))?;

    let hv = RouterHypervisor::new(None, None);
    let ip = hv.guest_ip(handle).await.into_diagnostic()?;

    // Determine SSH port: use the forwarded host port for user-mode networking
    let port = match handle.network {
        NetworkConfig::User => handle.ssh_host_port.unwrap_or(22),
        _ => 22,
    };

    // Resolve user: CLI flag → VMFile → default "vm"
    let user = args
        .user
        .or_else(|| ssh_user_from_vmfile(&args.name, args.file.as_deref()))
        .unwrap_or_else(|| "vm".to_string());

    // Check for a generated key in the VM's work directory first, then user keys
    let generated_key = handle.work_dir.join(super::GENERATED_KEY_FILE);
    let key_path = args
        .key
        .or_else(|| generated_key.exists().then_some(generated_key))
        .or_else(find_ssh_key)
        .ok_or_else(|| {
            miette::miette!(
                "no SSH key found — provide one with --key or ensure ~/.ssh/id_ed25519, \
                 ~/.ssh/id_ecdsa, or ~/.ssh/id_rsa exists"
            )
        })?;

    let config = SshConfig {
        user: user.clone(),
        public_key: None,
        private_key_path: Some(key_path),
        private_key_pem: None,
    };

    println!("Connecting to {user}@{ip}:{port}...");

    let sess = vm_manager::ssh::connect_with_retry(&ip, port, &config, Duration::from_secs(30))
        .await
        .into_diagnostic()?;

    // Drop the libssh2 session (just used to verify connectivity) and exec system ssh.
    // We use the system ssh binary for interactive terminal support.
    drop(sess);

    let mut cmd = tokio::process::Command::new("ssh");
    cmd.arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null");

    // Add port if non-standard
    if port != 22 {
        cmd.arg("-p").arg(port.to_string());
    }

    // Add key
    if let Some(ref key) = config.private_key_path {
        cmd.arg("-i").arg(key);
    }

    cmd.arg(format!("{user}@{ip}"));

    let status = cmd.status().await.into_diagnostic()?;

    if !status.success() {
        miette::bail!("SSH exited with status {}", status);
    }

    Ok(())
}
