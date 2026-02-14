use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use miette::{IntoDiagnostic, Result};
use vm_manager::{Hypervisor, RouterHypervisor, SshConfig};

use super::state;

#[derive(Args)]
pub struct SshArgs {
    /// VM name
    name: String,

    /// SSH user
    #[arg(long, default_value = "vm")]
    user: String,

    /// Path to SSH private key
    #[arg(long)]
    key: Option<PathBuf>,
}

pub async fn run(args: SshArgs) -> Result<()> {
    let store = state::load_store().await?;
    let handle = store
        .get(&args.name)
        .ok_or_else(|| miette::miette!("VM '{}' not found", args.name))?;

    let hv = RouterHypervisor::new(None, None);
    let ip = hv.guest_ip(handle).await.into_diagnostic()?;

    let key_path = args.key.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/root"))
            .join(".ssh")
            .join("id_ed25519")
    });

    let config = SshConfig {
        user: args.user.clone(),
        public_key: None,
        private_key_path: Some(key_path),
        private_key_pem: None,
    };

    println!("Connecting to {}@{}...", args.user, ip);

    let sess = vm_manager::ssh::connect_with_retry(&ip, &config, Duration::from_secs(30))
        .await
        .into_diagnostic()?;

    // Drop the libssh2 session (just used to verify connectivity) and exec system ssh.
    // We use the system ssh binary for interactive terminal support.
    drop(sess);

    let status = tokio::process::Command::new("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null")
        .args(
            config
                .private_key_path
                .iter()
                .flat_map(|p| ["-i".to_string(), p.display().to_string()]),
        )
        .arg(format!("{}@{}", args.user, ip))
        .status()
        .await
        .into_diagnostic()?;

    if !status.success() {
        miette::bail!("SSH exited with status {}", status);
    }

    Ok(())
}
