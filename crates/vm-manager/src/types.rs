use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Identifies which backend manages a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendTag {
    Noop,
    Qemu,
    Propolis,
}

impl std::fmt::Display for BackendTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Noop => write!(f, "noop"),
            Self::Qemu => write!(f, "qemu"),
            Self::Propolis => write!(f, "propolis"),
        }
    }
}

/// Full specification for creating a VM.
#[derive(Debug, Clone)]
pub struct VmSpec {
    pub name: String,
    pub image_path: PathBuf,
    pub vcpus: u16,
    pub memory_mb: u64,
    pub disk_gb: Option<u32>,
    pub network: NetworkConfig,
    pub cloud_init: Option<CloudInitConfig>,
    pub ssh: Option<SshConfig>,
}

/// Network configuration for a VM.
#[derive(Debug, Clone, Default)]
pub enum NetworkConfig {
    /// TAP device bridged to a host bridge (default on Linux).
    Tap { bridge: String },
    /// SLIRP user-mode networking (no root required).
    #[default]
    User,
    /// illumos VNIC for exclusive-IP zones.
    Vnic { name: String },
    /// No networking.
    None,
}

/// Cloud-init NoCloud configuration.
#[derive(Debug, Clone)]
pub struct CloudInitConfig {
    /// Raw user-data content (typically a cloud-config YAML).
    pub user_data: Vec<u8>,
    /// Instance ID for cloud-init metadata.
    pub instance_id: Option<String>,
    /// Hostname for the guest.
    pub hostname: Option<String>,
}

/// SSH connection configuration.
#[derive(Debug, Clone)]
pub struct SshConfig {
    /// Username to connect as.
    pub user: String,
    /// OpenSSH public key (for cloud-init authorized_keys injection).
    pub public_key: Option<String>,
    /// Path to a private key file on the host.
    pub private_key_path: Option<PathBuf>,
    /// In-memory PEM-encoded private key.
    pub private_key_pem: Option<String>,
}

/// Runtime handle for a managed VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmHandle {
    /// Unique identifier for this VM instance.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Which backend manages this VM.
    pub backend: BackendTag,
    /// Working directory for this VM's files.
    pub work_dir: PathBuf,
    /// Path to the QCOW2 overlay (QEMU) or raw disk.
    pub overlay_path: Option<PathBuf>,
    /// Path to the cloud-init seed ISO.
    pub seed_iso_path: Option<PathBuf>,
    /// QEMU process PID (Linux).
    pub pid: Option<u32>,
    /// Path to the QMP Unix socket (QEMU).
    pub qmp_socket: Option<PathBuf>,
    /// Path to the serial console Unix socket (QEMU).
    pub console_socket: Option<PathBuf>,
    /// VNC listen address (e.g. "127.0.0.1:5900").
    pub vnc_addr: Option<String>,
}

/// Observed VM lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmState {
    /// Backend is setting up resources.
    Preparing,
    /// Resources allocated, ready to start.
    Prepared,
    /// VM is running.
    Running,
    /// VM has been stopped (gracefully or forcibly).
    Stopped,
    /// VM encountered an error.
    Failed,
    /// VM and resources have been cleaned up.
    Destroyed,
}

impl std::fmt::Display for VmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preparing => write!(f, "preparing"),
            Self::Prepared => write!(f, "prepared"),
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::Failed => write!(f, "failed"),
            Self::Destroyed => write!(f, "destroyed"),
        }
    }
}
