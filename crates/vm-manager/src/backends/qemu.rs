use std::path::{Path, PathBuf};
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::cloudinit;
use crate::error::{Result, VmError};
use crate::image;
use crate::traits::{ConsoleEndpoint, Hypervisor};
use crate::types::{BackendTag, VmHandle, VmSpec, VmState};

use super::qmp::QmpClient;

/// QEMU-KVM backend for Linux.
///
/// Manages VMs as QEMU processes with QMP control sockets.
pub struct QemuBackend {
    qemu_binary: PathBuf,
    data_dir: PathBuf,
    default_bridge: Option<String>,
}

impl QemuBackend {
    pub fn new(
        qemu_binary: Option<PathBuf>,
        data_dir: Option<PathBuf>,
        default_bridge: Option<String>,
    ) -> Self {
        let data_dir = data_dir.unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("vmctl")
                .join("vms")
        });
        Self {
            qemu_binary: qemu_binary.unwrap_or_else(|| "qemu-system-x86_64".into()),
            data_dir,
            default_bridge,
        }
    }

    fn work_dir(&self, name: &str) -> PathBuf {
        self.data_dir.join(name)
    }

    /// Generate a random locally-administered MAC address.
    pub fn generate_mac() -> String {
        let bytes: [u8; 6] = rand_mac();
        format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
        )
    }

    /// Read PID from the pidfile in the work directory.
    async fn read_pid(work_dir: &Path) -> Option<u32> {
        let pid_path = work_dir.join("qemu.pid");
        tokio::fs::read_to_string(&pid_path)
            .await
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    /// Check if a process with the given PID is alive.
    fn pid_alive(pid: u32) -> bool {
        // Signal 0 checks if process exists without sending a signal
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
}

/// Generate a locally-administered unicast MAC address using random bytes.
fn rand_mac() -> [u8; 6] {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let s = RandomState::new();
    let mut h = s.build_hasher();
    h.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
    );
    let v = h.finish();

    let mut mac = [0u8; 6];
    mac[0] = 0x52; // locally administered, unicast
    mac[1] = 0x54;
    mac[2] = (v >> 24) as u8;
    mac[3] = (v >> 16) as u8;
    mac[4] = (v >> 8) as u8;
    mac[5] = v as u8;
    mac
}

impl Hypervisor for QemuBackend {
    async fn prepare(&self, spec: &VmSpec) -> Result<VmHandle> {
        let work_dir = self.work_dir(&spec.name);
        tokio::fs::create_dir_all(&work_dir).await?;

        // Create QCOW2 overlay
        let overlay = work_dir.join("overlay.qcow2");
        image::create_overlay(&spec.image_path, &overlay, spec.disk_gb).await?;

        // Generate cloud-init seed ISO if configured
        let mut seed_iso_path = None;
        if let Some(ref ci) = spec.cloud_init {
            let iso_path = work_dir.join("seed.iso");
            let instance_id = ci.instance_id.as_deref().unwrap_or(&spec.name);
            let hostname = ci.hostname.as_deref().unwrap_or(&spec.name);
            let meta_data = format!("instance-id: {instance_id}\nlocal-hostname: {hostname}\n");

            cloudinit::create_nocloud_iso_raw(&ci.user_data, meta_data.as_bytes(), &iso_path)?;
            seed_iso_path = Some(iso_path);
        }

        let qmp_socket = work_dir.join("qmp.sock");
        let console_socket = work_dir.join("console.sock");

        let handle = VmHandle {
            id: format!("qemu-{}", uuid::Uuid::new_v4()),
            name: spec.name.clone(),
            backend: BackendTag::Qemu,
            work_dir,
            overlay_path: Some(overlay),
            seed_iso_path,
            pid: None,
            qmp_socket: Some(qmp_socket),
            console_socket: Some(console_socket),
            vnc_addr: None,
        };

        info!(
            name = %spec.name,
            id = %handle.id,
            overlay = ?handle.overlay_path,
            seed = ?handle.seed_iso_path,
            "QEMU: prepared"
        );

        Ok(handle)
    }

    async fn start(&self, vm: &VmHandle) -> Result<()> {
        let overlay = vm
            .overlay_path
            .as_ref()
            .ok_or_else(|| VmError::InvalidState {
                name: vm.name.clone(),
                state: "no overlay path".into(),
            })?;

        // Read the VmSpec vcpus/memory from the overlay's qemu-img info? No — we need
        // to reconstruct from VmHandle. For now, use defaults if not stored.
        // The CLI will re-read spec and pass to prepare+start in sequence.

        let qmp_sock = vm.qmp_socket.as_ref().unwrap();
        let console_sock = vm.console_socket.as_ref().unwrap();

        let mut args: Vec<String> = vec![
            "-enable-kvm".into(),
            "-machine".into(),
            "q35,accel=kvm".into(),
            "-cpu".into(),
            "host".into(),
            "-nodefaults".into(),
            // QMP socket
            "-qmp".into(),
            format!("unix:{},server,nowait", qmp_sock.display()),
            // Serial console socket
            "-serial".into(),
            format!("unix:{},server,nowait", console_sock.display()),
            // VNC on localhost with auto-port
            "-vnc".into(),
            "127.0.0.1:0".into(),
            // Virtio RNG
            "-device".into(),
            "virtio-rng-pci".into(),
            // Main disk
            "-drive".into(),
            format!(
                "file={},format=qcow2,if=none,id=drive0,discard=unmap",
                overlay.display()
            ),
            "-device".into(),
            "virtio-blk-pci,drive=drive0".into(),
        ];

        // Seed ISO (cloud-init)
        if let Some(ref iso) = vm.seed_iso_path {
            args.extend([
                "-drive".into(),
                format!(
                    "file={},format=raw,if=none,id=seed,readonly=on",
                    iso.display()
                ),
                "-device".into(),
                "virtio-blk-pci,drive=seed".into(),
            ]);
        }

        // Daemonize and pidfile
        args.extend([
            "-daemonize".into(),
            "-pidfile".into(),
            vm.work_dir.join("qemu.pid").display().to_string(),
        ]);

        info!(
            name = %vm.name,
            binary = %self.qemu_binary.display(),
            "QEMU: starting"
        );
        debug!(args = ?args, "QEMU command line");

        let status = tokio::process::Command::new(&self.qemu_binary)
            .args(&args)
            .status()
            .await
            .map_err(|e| VmError::QemuSpawnFailed { source: e })?;

        if !status.success() {
            return Err(VmError::QemuSpawnFailed {
                source: std::io::Error::other(format!("QEMU exited with status {}", status)),
            });
        }

        // Wait for QMP socket and verify connection
        let mut qmp = QmpClient::connect(qmp_sock, Duration::from_secs(10)).await?;
        let status = qmp.query_status().await?;
        info!(name = %vm.name, status = %status, "QEMU: started");

        Ok(())
    }

    async fn stop(&self, vm: &VmHandle, timeout: Duration) -> Result<()> {
        // Try ACPI shutdown via QMP first
        if let Some(ref qmp_sock) = vm.qmp_socket {
            if qmp_sock.exists() {
                if let Ok(mut qmp) = QmpClient::connect(qmp_sock, Duration::from_secs(2)).await {
                    let _ = qmp.system_powerdown().await;
                }
            }
        }

        // Wait for process to exit
        let start = tokio::time::Instant::now();
        loop {
            if let Some(pid) = Self::read_pid(&vm.work_dir).await {
                if !Self::pid_alive(pid) {
                    info!(name = %vm.name, "QEMU: process exited after ACPI shutdown");
                    return Ok(());
                }
            } else {
                // No PID file, process likely already gone
                return Ok(());
            }

            if start.elapsed() >= timeout {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // SIGTERM fallback
        if let Some(pid) = Self::read_pid(&vm.work_dir).await {
            if Self::pid_alive(pid) {
                warn!(name = %vm.name, pid, "QEMU: ACPI shutdown timed out, sending SIGTERM");
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }

            // SIGKILL if still alive
            if Self::pid_alive(pid) {
                warn!(name = %vm.name, pid, "QEMU: SIGTERM failed, sending SIGKILL");
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }

        Ok(())
    }

    async fn suspend(&self, vm: &VmHandle) -> Result<()> {
        if let Some(ref qmp_sock) = vm.qmp_socket {
            let mut qmp = QmpClient::connect(qmp_sock, Duration::from_secs(5)).await?;
            qmp.stop().await?;
        }
        Ok(())
    }

    async fn resume(&self, vm: &VmHandle) -> Result<()> {
        if let Some(ref qmp_sock) = vm.qmp_socket {
            let mut qmp = QmpClient::connect(qmp_sock, Duration::from_secs(5)).await?;
            qmp.cont().await?;
        }
        Ok(())
    }

    async fn destroy(&self, vm: VmHandle) -> Result<()> {
        // Stop if running
        self.stop(&vm, Duration::from_secs(5)).await?;

        // QMP quit to ensure cleanup
        if let Some(ref qmp_sock) = vm.qmp_socket {
            if qmp_sock.exists() {
                if let Ok(mut qmp) = QmpClient::connect(qmp_sock, Duration::from_secs(2)).await {
                    let _ = qmp.quit().await;
                }
            }
        }

        // Remove work directory
        let _ = tokio::fs::remove_dir_all(&vm.work_dir).await;
        info!(name = %vm.name, "QEMU: destroyed");
        Ok(())
    }

    async fn state(&self, vm: &VmHandle) -> Result<VmState> {
        // Check if process is alive
        if let Some(pid) = Self::read_pid(&vm.work_dir).await {
            if Self::pid_alive(pid) {
                // Try QMP for detailed state
                if let Some(ref qmp_sock) = vm.qmp_socket {
                    if let Ok(mut qmp) = QmpClient::connect(qmp_sock, Duration::from_secs(2)).await
                    {
                        if let Ok(status) = qmp.query_status().await {
                            return Ok(match status.as_str() {
                                "running" => VmState::Running,
                                "paused" | "suspended" => VmState::Stopped,
                                _ => VmState::Running,
                            });
                        }
                    }
                }
                return Ok(VmState::Running);
            }
        }

        // Check if work dir exists (prepared but not running)
        if vm.work_dir.exists() {
            Ok(VmState::Stopped)
        } else {
            Ok(VmState::Destroyed)
        }
    }

    async fn guest_ip(&self, vm: &VmHandle) -> Result<String> {
        // Parse ARP table (`ip neigh`) looking for IPs on the bridge
        let output = tokio::process::Command::new("ip")
            .args(["neigh", "show"])
            .output()
            .await
            .map_err(|_| VmError::IpDiscoveryTimeout {
                name: vm.name.clone(),
            })?;

        let text = String::from_utf8_lossy(&output.stdout);

        // Try to find an IP from the ARP table. This is a best-effort heuristic:
        // look for REACHABLE or STALE entries on common bridge interfaces.
        for line in text.lines() {
            if line.contains("REACHABLE") || line.contains("STALE") {
                if let Some(ip) = line.split_whitespace().next() {
                    // Basic IPv4 check
                    if ip.contains('.') && !ip.starts_with("127.") {
                        return Ok(ip.to_string());
                    }
                }
            }
        }

        // Fallback: check dnsmasq leases if available
        if self.default_bridge.is_some() {
            let leases_path = "/var/lib/misc/dnsmasq.leases";
            if let Ok(content) = tokio::fs::read_to_string(leases_path).await {
                // Lease format: epoch MAC IP hostname clientid
                if let Some(line) = content.lines().last() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        return Ok(parts[2].to_string());
                    }
                }
            }
        }

        Err(VmError::IpDiscoveryTimeout {
            name: vm.name.clone(),
        })
    }

    fn console_endpoint(&self, vm: &VmHandle) -> Result<ConsoleEndpoint> {
        match vm.console_socket {
            Some(ref path) => Ok(ConsoleEndpoint::UnixSocket(path.clone())),
            None => Ok(ConsoleEndpoint::None),
        }
    }
}
