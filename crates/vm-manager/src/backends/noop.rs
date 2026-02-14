use std::time::Duration;

use tracing::info;

use crate::error::Result;
use crate::traits::{ConsoleEndpoint, Hypervisor};
use crate::types::{BackendTag, VmHandle, VmSpec, VmState};

/// No-op hypervisor for development and testing on hosts without VM capabilities.
#[derive(Debug, Clone, Default)]
pub struct NoopBackend;

impl Hypervisor for NoopBackend {
    async fn prepare(&self, spec: &VmSpec) -> Result<VmHandle> {
        let id = format!("noop-{}", uuid::Uuid::new_v4());
        let work_dir = std::env::temp_dir().join("vmctl-noop").join(&id);
        tokio::fs::create_dir_all(&work_dir).await?;
        info!(id = %id, name = %spec.name, image = ?spec.image_path, "noop: prepare");
        Ok(VmHandle {
            id,
            name: spec.name.clone(),
            backend: BackendTag::Noop,
            work_dir,
            overlay_path: None,
            seed_iso_path: None,
            pid: None,
            qmp_socket: None,
            console_socket: None,
            vnc_addr: None,
        })
    }

    async fn start(&self, vm: &VmHandle) -> Result<()> {
        info!(id = %vm.id, name = %vm.name, "noop: start");
        Ok(())
    }

    async fn stop(&self, vm: &VmHandle, _timeout: Duration) -> Result<()> {
        info!(id = %vm.id, name = %vm.name, "noop: stop");
        Ok(())
    }

    async fn suspend(&self, vm: &VmHandle) -> Result<()> {
        info!(id = %vm.id, name = %vm.name, "noop: suspend");
        Ok(())
    }

    async fn resume(&self, vm: &VmHandle) -> Result<()> {
        info!(id = %vm.id, name = %vm.name, "noop: resume");
        Ok(())
    }

    async fn destroy(&self, vm: VmHandle) -> Result<()> {
        info!(id = %vm.id, name = %vm.name, "noop: destroy");
        let _ = tokio::fs::remove_dir_all(&vm.work_dir).await;
        Ok(())
    }

    async fn state(&self, _vm: &VmHandle) -> Result<VmState> {
        Ok(VmState::Prepared)
    }

    async fn guest_ip(&self, _vm: &VmHandle) -> Result<String> {
        Ok("127.0.0.1".to_string())
    }

    fn console_endpoint(&self, _vm: &VmHandle) -> Result<ConsoleEndpoint> {
        Ok(ConsoleEndpoint::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::types::NetworkConfig;

    fn test_spec() -> VmSpec {
        VmSpec {
            name: "test-vm".into(),
            image_path: PathBuf::from("/tmp/test.qcow2"),
            vcpus: 1,
            memory_mb: 512,
            disk_gb: None,
            network: NetworkConfig::None,
            cloud_init: None,
            ssh: None,
        }
    }

    #[tokio::test]
    async fn noop_lifecycle() {
        let backend = NoopBackend;
        let spec = test_spec();

        let handle = backend.prepare(&spec).await.unwrap();
        assert_eq!(handle.backend, BackendTag::Noop);
        assert!(handle.id.starts_with("noop-"));

        backend.start(&handle).await.unwrap();
        assert_eq!(backend.state(&handle).await.unwrap(), VmState::Prepared);

        backend.suspend(&handle).await.unwrap();
        backend.resume(&handle).await.unwrap();

        let ip = backend.guest_ip(&handle).await.unwrap();
        assert_eq!(ip, "127.0.0.1");

        let endpoint = backend.console_endpoint(&handle).unwrap();
        assert!(matches!(endpoint, ConsoleEndpoint::None));

        backend.stop(&handle, Duration::from_secs(5)).await.unwrap();
        backend.destroy(handle).await.unwrap();
    }
}
