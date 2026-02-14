use std::path::PathBuf;

use clap::Args;
use miette::{IntoDiagnostic, Result};
use tracing::info;
use vm_manager::{CloudInitConfig, Hypervisor, NetworkConfig, RouterHypervisor, SshConfig, VmSpec};

use super::state;

#[derive(Args)]
pub struct CreateArgs {
    /// VM name
    #[arg(long)]
    name: String,

    /// Path to a local disk image
    #[arg(long)]
    image: Option<PathBuf>,

    /// URL to download an image from
    #[arg(long)]
    image_url: Option<String>,

    /// Number of vCPUs
    #[arg(long, default_value = "1")]
    vcpus: u16,

    /// Memory in MB
    #[arg(long, default_value = "1024")]
    memory: u64,

    /// Disk size in GB (overlay resize)
    #[arg(long)]
    disk: Option<u32>,

    /// Bridge name for TAP networking
    #[arg(long)]
    bridge: Option<String>,

    /// Path to cloud-init user-data file
    #[arg(long)]
    cloud_init: Option<PathBuf>,

    /// Path to SSH public key file (injected via cloud-init)
    #[arg(long)]
    ssh_key: Option<PathBuf>,

    /// Also start the VM after creation
    #[arg(long)]
    start: bool,
}

pub async fn run(args: CreateArgs) -> Result<()> {
    // Resolve image
    let image_path = if let Some(ref path) = args.image {
        path.clone()
    } else if let Some(ref url) = args.image_url {
        let mgr = vm_manager::image::ImageManager::new();
        mgr.pull(url, Some(&args.name)).await.into_diagnostic()?
    } else {
        miette::bail!("either --image or --image-url must be specified");
    };

    // Build cloud-init config if user-data or ssh key provided
    let cloud_init = if args.cloud_init.is_some() || args.ssh_key.is_some() {
        let user_data = if let Some(ref path) = args.cloud_init {
            tokio::fs::read(path).await.into_diagnostic()?
        } else if let Some(ref key_path) = args.ssh_key {
            let pubkey = tokio::fs::read_to_string(key_path)
                .await
                .into_diagnostic()?;
            let (ud, _) = vm_manager::cloudinit::build_cloud_config(
                "vm",
                pubkey.trim(),
                &args.name,
                &args.name,
            );
            ud
        } else {
            Vec::new()
        };

        Some(CloudInitConfig {
            user_data,
            instance_id: Some(args.name.clone()),
            hostname: Some(args.name.clone()),
        })
    } else {
        None
    };

    // Build SSH config if key provided
    let ssh = args.ssh_key.as_ref().map(|key_path| SshConfig {
        user: "vm".into(),
        public_key: None,
        private_key_path: Some(key_path.clone()),
        private_key_pem: None,
    });

    // Network config
    let network = if let Some(bridge) = args.bridge {
        NetworkConfig::Tap { bridge }
    } else {
        NetworkConfig::User
    };

    let spec = VmSpec {
        name: args.name.clone(),
        image_path,
        vcpus: args.vcpus,
        memory_mb: args.memory,
        disk_gb: args.disk,
        network,
        cloud_init,
        ssh,
    };

    let hv = RouterHypervisor::new(None, None);
    let handle = hv.prepare(&spec).await.into_diagnostic()?;

    info!(name = %args.name, id = %handle.id, "VM created");

    // Persist handle
    let mut store = state::load_store().await?;
    store.insert(args.name.clone(), handle.clone());
    state::save_store(&store).await?;

    println!("VM '{}' created (id: {})", args.name, handle.id);

    if args.start {
        hv.start(&handle).await.into_diagnostic()?;
        println!("VM '{}' started", args.name);
    }

    Ok(())
}
