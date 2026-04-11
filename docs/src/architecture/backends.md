# Hypervisor Backends

## The Hypervisor Trait

All backends implement the `Hypervisor` trait defined in `crates/vm-manager/src/traits.rs`:

```rust
pub trait Hypervisor: Send + Sync {
    fn prepare(&self, spec: &VmSpec) -> impl Future<Output = Result<VmHandle>>;
    fn start(&self, vm: &VmHandle) -> impl Future<Output = Result<VmHandle>>;
    fn stop(&self, vm: &VmHandle, timeout: Duration) -> impl Future<Output = Result<VmHandle>>;
    fn suspend(&self, vm: &VmHandle) -> impl Future<Output = Result<VmHandle>>;
    fn resume(&self, vm: &VmHandle) -> impl Future<Output = Result<VmHandle>>;
    fn destroy(&self, vm: VmHandle) -> impl Future<Output = Result<()>>;
    fn state(&self, vm: &VmHandle) -> impl Future<Output = Result<VmState>>;
    fn guest_ip(&self, vm: &VmHandle) -> impl Future<Output = Result<String>>;
    fn console_endpoint(&self, vm: &VmHandle) -> Result<ConsoleEndpoint>;
}
```

## QEMU Backend (Linux)

Located in `crates/vm-manager/src/backends/qemu.rs`.

**Prepare:**
- Creates work directory under `~/.local/share/vmctl/vms/<name>/`.
- Creates QCOW2 overlay on top of the base image.
- Generates cloud-init seed ISO (if configured).
- Allocates a deterministic SSH port (10022-10122 range, hash-based).
- Generates a locally-administered MAC address.

**Start:**
- Launches `qemu-system-x86_64` with KVM acceleration.
- CPU type: `host` (passthrough).
- Machine type: `q35,accel=kvm`.
- Devices: virtio-blk for disk, virtio-rng for entropy.
- Console: Unix socket + log file.
- VNC: localhost, auto-port.
- Networking: User-mode (SLIRP with port forwarding) or TAP (bridged).
- Daemonizes with PID file.
- Connects via QMP to verify startup and retrieve VNC address.

**Stop:**
1. ACPI power-down via QMP (`system_powerdown`).
2. Poll for process exit (500ms intervals) up to timeout.
3. SIGTERM if timeout exceeded.
4. SIGKILL as last resort.

**IP Discovery:**
- User-mode: returns `127.0.0.1` (SSH via forwarded port).
- TAP: parses ARP table (`ip neigh show`), falls back to dnsmasq lease files by MAC address.

## QMP Client

Located in `crates/vm-manager/src/backends/qmp.rs`. Async JSON-over-Unix-socket client implementing the QEMU Machine Protocol.

Commands: `system_powerdown`, `quit`, `stop`, `cont`, `query_status`, `query_vnc`.

## Propolis Backend (illumos)

Located in `crates/vm-manager/src/backends/propolis.rs`.

- Uses ZFS clones for VM disks.
- Manages zones with the `nebula-vm` brand.
- Communicates with `propolis-server` via REST API.
- Networking via illumos VNICs.

**Current limitations:**
- Suspend/resume returns an error (not yet implemented).
- Console tailing via WebSocket is not yet implemented (falls back to a warning).
- Guest IP discovery for VNIC networking may require manual verification.

## Noop Backend

Located in `crates/vm-manager/src/backends/noop.rs`. All operations succeed immediately. Used for testing.

## RouterHypervisor

Located in `crates/vm-manager/src/backends/mod.rs`. Dispatches `Hypervisor` trait calls to the correct backend based on the `VmHandle`'s `BackendTag`.

Construction:
- `RouterHypervisor::new(bridge, zfs_pool)` - Platform-aware, creates the appropriate backend.
- `RouterHypervisor::noop_only()` - Testing mode.
