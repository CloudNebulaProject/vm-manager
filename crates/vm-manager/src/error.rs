// The `unused_assignments` warnings are false positives from thiserror 2's derive macro
// on Rust edition 2024 — it generates destructuring assignments that the compiler considers
// "never read" even though they are used in the Display implementation.
#![allow(unused_assignments)]

use miette::Diagnostic;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum VmError {
    #[error("failed to spawn QEMU process: {source}")]
    #[diagnostic(
        code(vm_manager::qemu::spawn_failed),
        help(
            "ensure qemu-system-x86_64 is installed and in PATH, and that KVM is available (/dev/kvm)"
        )
    )]
    QemuSpawnFailed { source: std::io::Error },

    #[error("failed to connect to QMP socket at {}: {source}", path.display())]
    #[diagnostic(
        code(vm_manager::qemu::qmp_connect_failed),
        help(
            "the QEMU process may have crashed before the QMP socket was ready — check the work directory for logs"
        )
    )]
    QmpConnectionFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("QMP command failed: {message}")]
    #[diagnostic(code(vm_manager::qemu::qmp_command_failed))]
    QmpCommandFailed { message: String },

    #[error("failed to create QCOW2 overlay from base image {}: {detail}", base.display())]
    #[diagnostic(
        code(vm_manager::image::overlay_creation_failed),
        help("ensure qemu-img is installed and the base image exists and is readable")
    )]
    OverlayCreationFailed { base: PathBuf, detail: String },

    #[error("timed out waiting for guest IP address for VM {name}")]
    #[diagnostic(
        code(vm_manager::network::ip_discovery_timeout),
        help(
            "the guest may not have obtained a DHCP lease — check bridge/network configuration and that the guest cloud-init is configured correctly"
        )
    )]
    IpDiscoveryTimeout { name: String },

    #[error("propolis server at {addr} is unreachable: {source}")]
    #[diagnostic(
        code(vm_manager::propolis::unreachable),
        help(
            "ensure the propolis-server process is running inside the zone and listening on the expected address"
        )
    )]
    PropolisUnreachable {
        addr: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("failed to create cloud-init seed ISO: {detail}")]
    #[diagnostic(
        code(vm_manager::cloudinit::iso_failed),
        help(
            "ensure genisoimage or mkisofs is installed, or enable the `pure-iso` feature for a Rust-only fallback"
        )
    )]
    CloudInitIsoFailed { detail: String },

    #[error("SSH operation failed: {detail}")]
    #[diagnostic(
        code(vm_manager::ssh::failed),
        help("check that the SSH key is correct, the guest is reachable, and sshd is running")
    )]
    SshFailed { detail: String },

    #[error("failed to download image from {url}: {detail}")]
    #[diagnostic(
        code(vm_manager::image::download_failed),
        help("check network connectivity and that the URL is correct")
    )]
    ImageDownloadFailed { url: String, detail: String },

    #[error("image format detection failed for {}: {detail}", path.display())]
    #[diagnostic(
        code(vm_manager::image::format_detection_failed),
        help("ensure qemu-img is installed and the file is a valid disk image")
    )]
    ImageFormatDetectionFailed { path: PathBuf, detail: String },

    #[error("image conversion failed: {detail}")]
    #[diagnostic(
        code(vm_manager::image::conversion_failed),
        help("ensure qemu-img is installed and there is enough disk space")
    )]
    ImageConversionFailed { detail: String },

    #[error("VM {name} not found")]
    #[diagnostic(
        code(vm_manager::vm::not_found),
        help("run `vmctl list` to see available VMs")
    )]
    VmNotFound { name: String },

    #[error("VM {name} is in state {state} which does not allow this operation")]
    #[diagnostic(code(vm_manager::vm::invalid_state))]
    InvalidState { name: String, state: String },

    #[error("backend not available: {backend}")]
    #[diagnostic(
        code(vm_manager::backend::not_available),
        help("this backend is not supported on the current platform")
    )]
    BackendNotAvailable { backend: String },

    #[error(transparent)]
    #[diagnostic(code(vm_manager::io))]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, VmError>;
