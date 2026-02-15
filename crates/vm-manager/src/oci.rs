use oci_client::client::{ClientConfig, ClientProtocol};
use oci_client::secrets::RegistryAuth;
use oci_client::{Client, Reference};
use tracing::info;

use crate::error::{Result, VmError};

const QCOW2_LAYER_MEDIA_TYPE: &str = "application/vnd.cloudnebula.qcow2.layer.v1";

/// Pull a QCOW2 image stored as an OCI artifact from a registry.
pub async fn pull_qcow2(reference_str: &str) -> Result<Vec<u8>> {
    let reference: Reference = reference_str.parse().map_err(|e: oci_client::ParseError| {
        VmError::OciPullFailed {
            reference: reference_str.to_string(),
            detail: format!("invalid OCI reference: {e}"),
        }
    })?;

    let auth = resolve_auth(&reference);

    let client_config = ClientConfig {
        protocol: ClientProtocol::Https,
        ..Default::default()
    };
    let client = Client::new(client_config);

    info!(reference = %reference, "Pulling QCOW2 artifact from OCI registry");

    let image_data = client
        .pull(
            &reference,
            &auth,
            vec![QCOW2_LAYER_MEDIA_TYPE, "application/octet-stream"],
        )
        .await
        .map_err(|e| VmError::OciPullFailed {
            reference: reference_str.to_string(),
            detail: e.to_string(),
        })?;

    // Find the QCOW2 layer
    let layer = image_data
        .layers
        .into_iter()
        .next()
        .ok_or_else(|| VmError::OciPullFailed {
            reference: reference_str.to_string(),
            detail: "artifact contains no layers".to_string(),
        })?;

    info!(
        reference = %reference,
        size_bytes = layer.data.len(),
        "QCOW2 artifact pulled successfully"
    );

    Ok(layer.data)
}

/// Resolve authentication for the given registry.
/// Uses GITHUB_TOKEN for ghcr.io, Anonymous for everything else.
fn resolve_auth(reference: &Reference) -> RegistryAuth {
    let registry = reference.registry();
    if registry == "ghcr.io" {
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            return RegistryAuth::Basic("_token".to_string(), token);
        }
    }
    RegistryAuth::Anonymous
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_auth_ghcr_without_token() {
        // Without GITHUB_TOKEN set, ghcr.io should use Anonymous
        // SAFETY: This test is not run in parallel with other tests that
        // depend on GITHUB_TOKEN.
        unsafe { std::env::remove_var("GITHUB_TOKEN") };
        let reference: Reference = "ghcr.io/test/image:latest".parse().unwrap();
        let auth = resolve_auth(&reference);
        assert!(matches!(auth, RegistryAuth::Anonymous));
    }

    #[test]
    fn test_resolve_auth_other_registry() {
        let reference: Reference = "docker.io/library/ubuntu:latest".parse().unwrap();
        let auth = resolve_auth(&reference);
        assert!(matches!(auth, RegistryAuth::Anonymous));
    }
}
