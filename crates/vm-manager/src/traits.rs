use std::time::Duration;

use crate::error::Result;
use crate::types::{VmHandle, VmSpec, VmState};

/// Async hypervisor trait implemented by each backend (QEMU, Propolis, Noop).
///
/// The lifecycle is: `prepare` -> `start` -> (optionally `suspend`/`resume`) -> `stop` -> `destroy`.
pub trait Hypervisor: Send + Sync {
    /// Allocate resources (overlay disk, cloud-init ISO, zone config, etc.) and return a handle.
    fn prepare(&self, spec: &VmSpec) -> impl Future<Output = Result<VmHandle>> + Send;

    /// Boot the VM.
    fn start(&self, vm: &VmHandle) -> impl Future<Output = Result<()>> + Send;

    /// Gracefully stop the VM. Falls back to forceful termination after `timeout`.
    fn stop(&self, vm: &VmHandle, timeout: Duration) -> impl Future<Output = Result<()>> + Send;

    /// Pause VM execution (freeze vCPUs).
    fn suspend(&self, vm: &VmHandle) -> impl Future<Output = Result<()>> + Send;

    /// Resume a suspended VM.
    fn resume(&self, vm: &VmHandle) -> impl Future<Output = Result<()>> + Send;

    /// Stop the VM (if running) and clean up all resources.
    fn destroy(&self, vm: VmHandle) -> impl Future<Output = Result<()>> + Send;

    /// Query the current state of the VM.
    fn state(&self, vm: &VmHandle) -> impl Future<Output = Result<VmState>> + Send;

    /// Attempt to discover the guest's IP address.
    fn guest_ip(&self, vm: &VmHandle) -> impl Future<Output = Result<String>> + Send;

    /// Return a path or address for attaching to the VM's serial console.
    fn console_endpoint(&self, vm: &VmHandle) -> Result<ConsoleEndpoint>;
}

/// Describes how to connect to a VM's serial console.
#[derive(Debug, Clone)]
pub enum ConsoleEndpoint {
    /// Unix domain socket path (QEMU).
    UnixSocket(std::path::PathBuf),
    /// WebSocket URL (Propolis).
    WebSocket(String),
    /// Not available (Noop).
    None,
}
