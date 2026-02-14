pub mod noop;

#[cfg(target_os = "linux")]
pub mod qemu;
#[cfg(target_os = "linux")]
pub mod qmp;

#[cfg(target_os = "illumos")]
pub mod propolis;

use std::time::Duration;

use crate::error::{Result, VmError};
use crate::traits::{ConsoleEndpoint, Hypervisor};
use crate::types::{BackendTag, VmHandle, VmSpec, VmState};

/// Platform-aware router that delegates to the appropriate backend.
pub struct RouterHypervisor {
    pub noop: noop::NoopBackend,
    #[cfg(target_os = "linux")]
    pub qemu: Option<qemu::QemuBackend>,
    #[cfg(target_os = "illumos")]
    pub propolis: Option<propolis::PropolisBackend>,
}

impl RouterHypervisor {
    /// Build a router with platform defaults.
    ///
    /// On Linux, creates a QemuBackend with the given bridge.
    /// On illumos, creates a PropolisBackend with the given ZFS pool.
    #[allow(unused_variables)]
    pub fn new(bridge: Option<String>, zfs_pool: Option<String>) -> Self {
        #[cfg(target_os = "linux")]
        {
            RouterHypervisor {
                noop: noop::NoopBackend,
                qemu: Some(qemu::QemuBackend::new(None, None, bridge)),
            }
        }
        #[cfg(target_os = "illumos")]
        {
            RouterHypervisor {
                noop: noop::NoopBackend,
                propolis: Some(propolis::PropolisBackend::new(
                    None,
                    zfs_pool.unwrap_or_else(|| "rpool".into()),
                )),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "illumos")))]
        {
            RouterHypervisor {
                noop: noop::NoopBackend,
            }
        }
    }

    /// Build a router that only has the noop backend (for dev/testing).
    pub fn noop_only() -> Self {
        #[cfg(target_os = "linux")]
        {
            RouterHypervisor {
                noop: noop::NoopBackend,
                qemu: None,
            }
        }
        #[cfg(target_os = "illumos")]
        {
            RouterHypervisor {
                noop: noop::NoopBackend,
                propolis: None,
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "illumos")))]
        {
            RouterHypervisor {
                noop: noop::NoopBackend,
            }
        }
    }
}

impl Hypervisor for RouterHypervisor {
    async fn prepare(&self, spec: &VmSpec) -> Result<VmHandle> {
        #[cfg(target_os = "linux")]
        if let Some(ref qemu) = self.qemu {
            return qemu.prepare(spec).await;
        }
        #[cfg(target_os = "illumos")]
        if let Some(ref propolis) = self.propolis {
            return propolis.prepare(spec).await;
        }
        self.noop.prepare(spec).await
    }

    async fn start(&self, vm: &VmHandle) -> Result<()> {
        match vm.backend {
            #[cfg(target_os = "linux")]
            BackendTag::Qemu => match self.qemu {
                Some(ref q) => q.start(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "qemu".into(),
                }),
            },
            #[cfg(target_os = "illumos")]
            BackendTag::Propolis => match self.propolis {
                Some(ref p) => p.start(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "propolis".into(),
                }),
            },
            BackendTag::Noop => self.noop.start(vm).await,
            #[allow(unreachable_patterns)]
            _ => Err(VmError::BackendNotAvailable {
                backend: vm.backend.to_string(),
            }),
        }
    }

    async fn stop(&self, vm: &VmHandle, timeout: Duration) -> Result<()> {
        match vm.backend {
            #[cfg(target_os = "linux")]
            BackendTag::Qemu => match self.qemu {
                Some(ref q) => q.stop(vm, timeout).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "qemu".into(),
                }),
            },
            #[cfg(target_os = "illumos")]
            BackendTag::Propolis => match self.propolis {
                Some(ref p) => p.stop(vm, timeout).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "propolis".into(),
                }),
            },
            BackendTag::Noop => self.noop.stop(vm, timeout).await,
            #[allow(unreachable_patterns)]
            _ => Err(VmError::BackendNotAvailable {
                backend: vm.backend.to_string(),
            }),
        }
    }

    async fn suspend(&self, vm: &VmHandle) -> Result<()> {
        match vm.backend {
            #[cfg(target_os = "linux")]
            BackendTag::Qemu => match self.qemu {
                Some(ref q) => q.suspend(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "qemu".into(),
                }),
            },
            #[cfg(target_os = "illumos")]
            BackendTag::Propolis => match self.propolis {
                Some(ref p) => p.suspend(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "propolis".into(),
                }),
            },
            BackendTag::Noop => self.noop.suspend(vm).await,
            #[allow(unreachable_patterns)]
            _ => Err(VmError::BackendNotAvailable {
                backend: vm.backend.to_string(),
            }),
        }
    }

    async fn resume(&self, vm: &VmHandle) -> Result<()> {
        match vm.backend {
            #[cfg(target_os = "linux")]
            BackendTag::Qemu => match self.qemu {
                Some(ref q) => q.resume(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "qemu".into(),
                }),
            },
            #[cfg(target_os = "illumos")]
            BackendTag::Propolis => match self.propolis {
                Some(ref p) => p.resume(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "propolis".into(),
                }),
            },
            BackendTag::Noop => self.noop.resume(vm).await,
            #[allow(unreachable_patterns)]
            _ => Err(VmError::BackendNotAvailable {
                backend: vm.backend.to_string(),
            }),
        }
    }

    async fn destroy(&self, vm: VmHandle) -> Result<()> {
        match vm.backend {
            #[cfg(target_os = "linux")]
            BackendTag::Qemu => match self.qemu {
                Some(ref q) => q.destroy(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "qemu".into(),
                }),
            },
            #[cfg(target_os = "illumos")]
            BackendTag::Propolis => match self.propolis {
                Some(ref p) => p.destroy(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "propolis".into(),
                }),
            },
            BackendTag::Noop => self.noop.destroy(vm).await,
            #[allow(unreachable_patterns)]
            _ => Err(VmError::BackendNotAvailable {
                backend: vm.backend.to_string(),
            }),
        }
    }

    async fn state(&self, vm: &VmHandle) -> Result<VmState> {
        match vm.backend {
            #[cfg(target_os = "linux")]
            BackendTag::Qemu => match self.qemu {
                Some(ref q) => q.state(vm).await,
                None => Ok(VmState::Destroyed),
            },
            #[cfg(target_os = "illumos")]
            BackendTag::Propolis => match self.propolis {
                Some(ref p) => p.state(vm).await,
                None => Ok(VmState::Destroyed),
            },
            BackendTag::Noop => self.noop.state(vm).await,
            #[allow(unreachable_patterns)]
            _ => Ok(VmState::Destroyed),
        }
    }

    async fn guest_ip(&self, vm: &VmHandle) -> Result<String> {
        match vm.backend {
            #[cfg(target_os = "linux")]
            BackendTag::Qemu => match self.qemu {
                Some(ref q) => q.guest_ip(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "qemu".into(),
                }),
            },
            #[cfg(target_os = "illumos")]
            BackendTag::Propolis => match self.propolis {
                Some(ref p) => p.guest_ip(vm).await,
                None => Err(VmError::BackendNotAvailable {
                    backend: "propolis".into(),
                }),
            },
            BackendTag::Noop => self.noop.guest_ip(vm).await,
            #[allow(unreachable_patterns)]
            _ => Err(VmError::BackendNotAvailable {
                backend: vm.backend.to_string(),
            }),
        }
    }

    fn console_endpoint(&self, vm: &VmHandle) -> Result<ConsoleEndpoint> {
        match vm.backend {
            #[cfg(target_os = "linux")]
            BackendTag::Qemu => match self.qemu {
                Some(ref q) => q.console_endpoint(vm),
                None => Err(VmError::BackendNotAvailable {
                    backend: "qemu".into(),
                }),
            },
            #[cfg(target_os = "illumos")]
            BackendTag::Propolis => match self.propolis {
                Some(ref p) => p.console_endpoint(vm),
                None => Err(VmError::BackendNotAvailable {
                    backend: "propolis".into(),
                }),
            },
            BackendTag::Noop => self.noop.console_endpoint(vm),
            #[allow(unreachable_patterns)]
            _ => Err(VmError::BackendNotAvailable {
                backend: vm.backend.to_string(),
            }),
        }
    }
}
