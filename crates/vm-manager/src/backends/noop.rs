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
            vcpus: spec.vcpus,
            memory_mb: spec.memory_mb,
            disk_gb: spec.disk_gb,
            network: spec.network.clone(),
            ssh_host_port: None,
            mac_addr: None,
            uefi: false,
        })
    }

    async fn start(&self, vm: &VmHandle) -> Result<VmHandle> {
        info!(id = %vm.id, name = %vm.name, "noop: start");
        Ok(vm.clone())
    }

    async fn stop(&self, vm: &VmHandle, _timeout: Duration) -> Result<VmHandle> {
        info!(id = %vm.id, name = %vm.name, "noop: stop");
        Ok(vm.clone())
    }

    async fn suspend(&self, vm: &VmHandle) -> Result<VmHandle> {
        info!(id = %vm.id, name = %vm.name, "noop: suspend");
        Ok(vm.clone())
    }

    async fn resume(&self, vm: &VmHandle) -> Result<VmHandle> {
        info!(id = %vm.id, name = %vm.name, "noop: resume");
        Ok(vm.clone())
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
            uefi: false,
        }
    }

    #[tokio::test]
    async fn noop_lifecycle() {
        let backend = NoopBackend;
        let spec = test_spec();

        let handle = backend.prepare(&spec).await.unwrap();
        assert_eq!(handle.backend, BackendTag::Noop);
        assert!(handle.id.starts_with("noop-"));

        let handle = backend.start(&handle).await.unwrap();
        assert_eq!(backend.state(&handle).await.unwrap(), VmState::Prepared);

        let handle = backend.suspend(&handle).await.unwrap();
        let handle = backend.resume(&handle).await.unwrap();

        let ip = backend.guest_ip(&handle).await.unwrap();
        assert_eq!(ip, "127.0.0.1");

        let endpoint = backend.console_endpoint(&handle).unwrap();
        assert!(matches!(endpoint, ConsoleEndpoint::None));

        let handle = backend.stop(&handle, Duration::from_secs(5)).await.unwrap();
        backend.destroy(handle).await.unwrap();
    }

    #[test]
    fn network_config_roundtrip() {
        let configs = vec![
            NetworkConfig::User,
            NetworkConfig::Tap {
                bridge: "br0".into(),
            },
            NetworkConfig::Vnic {
                name: "vnic0".into(),
            },
            NetworkConfig::None,
        ];
        for cfg in configs {
            let json = serde_json::to_string(&cfg).unwrap();
            let parsed: NetworkConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(format!("{:?}", cfg), format!("{:?}", parsed));
        }
    }

    #[test]
    fn vmhandle_roundtrip() {
        let handle = VmHandle {
            id: "test-123".into(),
            name: "my-vm".into(),
            backend: BackendTag::Noop,
            work_dir: "/tmp/test".into(),
            overlay_path: None,
            seed_iso_path: None,
            pid: Some(1234),
            qmp_socket: None,
            console_socket: None,
            vnc_addr: Some("127.0.0.1:5900".into()),
            vcpus: 4,
            memory_mb: 2048,
            disk_gb: Some(20),
            network: NetworkConfig::User,
            ssh_host_port: Some(10022),
            mac_addr: Some("52:54:00:ab:cd:ef".into()),
            uefi: false,
        };
        let json = serde_json::to_string_pretty(&handle).unwrap();
        let parsed: VmHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(handle.id, parsed.id);
        assert_eq!(handle.vcpus, parsed.vcpus);
        assert_eq!(handle.memory_mb, parsed.memory_mb);
        assert_eq!(handle.ssh_host_port, parsed.ssh_host_port);
        assert_eq!(handle.mac_addr, parsed.mac_addr);
    }

    #[test]
    fn vmhandle_backward_compat() {
        // Simulate a JSON from before the new fields were added
        let old_json = r#"{
            "id": "old-123",
            "name": "old-vm",
            "backend": "noop",
            "work_dir": "/tmp/old",
            "overlay_path": null,
            "seed_iso_path": null,
            "pid": null,
            "qmp_socket": null,
            "console_socket": null,
            "vnc_addr": null
        }"#;
        let handle: VmHandle = serde_json::from_str(old_json).unwrap();
        assert_eq!(handle.vcpus, 1);
        assert_eq!(handle.memory_mb, 1024);
        assert_eq!(handle.disk_gb, None);
        assert!(handle.ssh_host_port.is_none());
        assert!(handle.mac_addr.is_none());
    }
}
