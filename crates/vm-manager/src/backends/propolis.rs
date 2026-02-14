//! Propolis (Oxide's bhyve VMM) backend for illumos.
//!
//! Manages VMs inside `nebula-vm` branded zones with propolis-server.

use std::path::PathBuf;
use std::time::Duration;

use tracing::{info, warn};

use crate::error::{Result, VmError};
use crate::traits::{ConsoleEndpoint, Hypervisor};
use crate::types::{BackendTag, NetworkConfig, VmHandle, VmSpec, VmState};

/// Propolis backend for illumos zones.
pub struct PropolisBackend {
    data_dir: PathBuf,
    zfs_pool: String,
}

impl PropolisBackend {
    pub fn new(data_dir: Option<PathBuf>, zfs_pool: String) -> Self {
        let data_dir = data_dir.unwrap_or_else(|| PathBuf::from("/var/lib/vmctl/vms"));
        Self { data_dir, zfs_pool }
    }

    fn work_dir(&self, name: &str) -> PathBuf {
        self.data_dir.join(name)
    }

    /// Run a shell command and return (success, stdout, stderr).
    async fn run_cmd(cmd: &str, args: &[&str]) -> Result<(bool, String, String)> {
        let output = tokio::process::Command::new(cmd)
            .args(args)
            .output()
            .await?;
        Ok((
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }

    /// Poll propolis-server until it responds, up to timeout.
    async fn wait_for_propolis(addr: &str, timeout: Duration) -> Result<()> {
        let client = reqwest::Client::new();
        let deadline = tokio::time::Instant::now() + timeout;
        let url = format!("http://{addr}/instance");

        loop {
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() || resp.status().as_u16() == 404 {
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(VmError::PropolisUnreachable {
                    addr: addr.into(),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "propolis-server did not become available",
                    )),
                });
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

impl Hypervisor for PropolisBackend {
    async fn prepare(&self, spec: &VmSpec) -> Result<VmHandle> {
        let work_dir = self.work_dir(&spec.name);
        tokio::fs::create_dir_all(&work_dir).await?;

        // Clone ZFS dataset for the VM disk
        let base_dataset = format!("{}/images/{}", self.zfs_pool, spec.name);
        let vm_dataset = format!("{}/vms/{}", self.zfs_pool, spec.name);

        let (ok, _, stderr) = Self::run_cmd(
            "zfs",
            &["clone", &format!("{base_dataset}@latest"), &vm_dataset],
        )
        .await?;
        if !ok {
            warn!(name = %spec.name, stderr = %stderr, "ZFS clone failed (may already exist)");
        }

        // Create cloud-init seed ISO if configured
        let mut seed_iso_path = None;
        if let Some(ref ci) = spec.cloud_init {
            let iso_path = work_dir.join("seed.iso");
            let instance_id = ci.instance_id.as_deref().unwrap_or(&spec.name);
            let hostname = ci.hostname.as_deref().unwrap_or(&spec.name);
            let meta_data = format!("instance-id: {instance_id}\nlocal-hostname: {hostname}\n");
            crate::cloudinit::create_nocloud_iso_raw(
                &ci.user_data,
                meta_data.as_bytes(),
                &iso_path,
            )?;
            seed_iso_path = Some(iso_path);
        }

        // Determine VNIC name
        let vnic_name = match &spec.network {
            NetworkConfig::Vnic { name } => name.clone(),
            _ => format!("vnic_{}", spec.name),
        };

        // Configure and install zone
        let zone_name = &spec.name;
        let zonecfg_cmds = format!(
            "create -b; set brand=nebula-vm; set zonepath={work_dir}; set ip-type=exclusive; \
             add net; set physical={vnic_name}; end; commit",
            work_dir = work_dir.display()
        );
        let (ok, _, stderr) = Self::run_cmd("zonecfg", &["-z", zone_name, &zonecfg_cmds]).await?;
        if !ok {
            warn!(name = %zone_name, stderr = %stderr, "zonecfg failed (zone may already exist)");
        }

        let (ok, _, stderr) = Self::run_cmd("zoneadm", &["-z", zone_name, "install"]).await?;
        if !ok {
            warn!(name = %zone_name, stderr = %stderr, "zone install failed");
        }

        let handle = VmHandle {
            id: format!("propolis-{}", uuid::Uuid::new_v4()),
            name: spec.name.clone(),
            backend: BackendTag::Propolis,
            work_dir,
            overlay_path: None,
            seed_iso_path,
            pid: None,
            qmp_socket: None,
            console_socket: None,
            vnc_addr: None,
        };

        info!(name = %spec.name, id = %handle.id, "Propolis: prepared");
        Ok(handle)
    }

    async fn start(&self, vm: &VmHandle) -> Result<()> {
        // Boot zone
        let (ok, _, stderr) = Self::run_cmd("zoneadm", &["-z", &vm.name, "boot"]).await?;
        if !ok {
            return Err(VmError::QemuSpawnFailed {
                source: std::io::Error::other(format!("zone boot failed: {stderr}")),
            });
        }

        // The brand boot script starts propolis-server inside the zone.
        // Wait for it to become available.
        let propolis_addr = format!("127.0.0.1:12400"); // default propolis port
        Self::wait_for_propolis(&propolis_addr, Duration::from_secs(30)).await?;

        // PUT /instance with instance spec
        let client = reqwest::Client::new();
        let instance_spec = serde_json::json!({
            "properties": {
                "id": vm.id,
                "name": vm.name,
                "description": "managed by vmctl"
            },
            "nics": [],
            "disks": [],
            "boot_settings": {
                "order": [{"name": "disk0"}]
            }
        });

        client
            .put(format!("http://{propolis_addr}/instance"))
            .json(&instance_spec)
            .send()
            .await
            .map_err(|e| VmError::PropolisUnreachable {
                addr: propolis_addr.clone(),
                source: Box::new(e),
            })?;

        // PUT /instance/state → Run
        client
            .put(format!("http://{propolis_addr}/instance/state"))
            .json(&serde_json::json!("Run"))
            .send()
            .await
            .map_err(|e| VmError::PropolisUnreachable {
                addr: propolis_addr.clone(),
                source: Box::new(e),
            })?;

        info!(name = %vm.name, "Propolis: started");
        Ok(())
    }

    async fn stop(&self, vm: &VmHandle, _timeout: Duration) -> Result<()> {
        let propolis_addr = "127.0.0.1:12400";
        let client = reqwest::Client::new();

        // PUT /instance/state → Stop
        let _ = client
            .put(format!("http://{propolis_addr}/instance/state"))
            .json(&serde_json::json!("Stop"))
            .send()
            .await;

        // Halt the zone
        let _ = Self::run_cmd("zoneadm", &["-z", &vm.name, "halt"]).await;

        info!(name = %vm.name, "Propolis: stopped");
        Ok(())
    }

    async fn suspend(&self, vm: &VmHandle) -> Result<()> {
        info!(name = %vm.name, "Propolis: suspend (not yet implemented)");
        Ok(())
    }

    async fn resume(&self, vm: &VmHandle) -> Result<()> {
        info!(name = %vm.name, "Propolis: resume (not yet implemented)");
        Ok(())
    }

    async fn destroy(&self, vm: VmHandle) -> Result<()> {
        // Stop first
        self.stop(&vm, Duration::from_secs(10)).await?;

        // Uninstall and delete zone
        let _ = Self::run_cmd("zoneadm", &["-z", &vm.name, "uninstall", "-F"]).await;
        let _ = Self::run_cmd("zonecfg", &["-z", &vm.name, "delete", "-F"]).await;

        // Destroy ZFS dataset
        let vm_dataset = format!("{}/vms/{}", self.zfs_pool, vm.name);
        let _ = Self::run_cmd("zfs", &["destroy", "-r", &vm_dataset]).await;

        // Remove work directory
        let _ = tokio::fs::remove_dir_all(&vm.work_dir).await;

        info!(name = %vm.name, "Propolis: destroyed");
        Ok(())
    }

    async fn state(&self, vm: &VmHandle) -> Result<VmState> {
        let (ok, stdout, _) = Self::run_cmd("zoneadm", &["-z", &vm.name, "list", "-p"]).await?;
        if !ok {
            return Ok(VmState::Destroyed);
        }

        // Output format: zoneid:zonename:state:zonepath:uuid:brand:ip-type
        let state_field = stdout.split(':').nth(2).unwrap_or("").trim();

        Ok(match state_field {
            "running" => VmState::Running,
            "installed" => VmState::Prepared,
            "configured" => VmState::Prepared,
            _ => VmState::Stopped,
        })
    }

    async fn guest_ip(&self, vm: &VmHandle) -> Result<String> {
        // For exclusive-IP zones, the IP is configured inside the zone.
        // Try to query it via zlogin.
        let (ok, stdout, _) = Self::run_cmd(
            "zlogin",
            &[&vm.name, "ipadm", "show-addr", "-p", "-o", "ADDR"],
        )
        .await?;

        if ok {
            for line in stdout.lines() {
                let addr = line
                    .trim()
                    .trim_end_matches(|c: char| c == '/' || c.is_ascii_digit());
                let addr = line.split('/').next().unwrap_or("").trim();
                if !addr.is_empty() && addr != "127.0.0.1" && addr.contains('.') {
                    return Ok(addr.to_string());
                }
            }
        }

        Err(VmError::IpDiscoveryTimeout {
            name: vm.name.clone(),
        })
    }

    fn console_endpoint(&self, vm: &VmHandle) -> Result<ConsoleEndpoint> {
        // Propolis serial console is available via WebSocket
        Ok(ConsoleEndpoint::WebSocket(format!(
            "ws://127.0.0.1:12400/instance/serial"
        )))
    }
}
