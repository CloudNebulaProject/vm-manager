use oci_client::client::{ClientConfig, ClientProtocol};
use oci_client::secrets::RegistryAuth;
use oci_client::{Client, Reference};
use tracing::info;

use crate::error::{Result, VmError};

const QCOW2_LAYER_MEDIA_TYPE: &str = "application/vnd.cloudnebula.qcow2.layer.v1";

fn build_client() -> Client {
    Client::new(ClientConfig {
        protocol: ClientProtocol::Https,
        ..Default::default()
    })
}

/// Fetch the manifest digest for a reference without downloading any blob.
///
/// Used by callers that want to check whether a tag has rolled to a new
/// digest before re-pulling a (potentially large) layer. Returns the
/// digest string from the registry (e.g. `sha256:…`).
pub async fn fetch_manifest_digest(reference_str: &str) -> Result<String> {
    let reference: Reference =
        reference_str
            .parse()
            .map_err(|e: oci_client::ParseError| VmError::OciPullFailed {
                reference: reference_str.to_string(),
                detail: format!("invalid OCI reference: {e}"),
            })?;
    let auth = resolve_auth(&reference);
    let client = build_client();
    let (_manifest, digest) = client
        .pull_manifest(&reference, &auth)
        .await
        .map_err(|e| VmError::OciPullFailed {
            reference: reference_str.to_string(),
            detail: e.to_string(),
        })?;
    Ok(digest)
}

/// Pull a QCOW2 image stored as an OCI artifact from a registry.
pub async fn pull_qcow2(reference_str: &str) -> Result<Vec<u8>> {
    pull_artifact_layer(reference_str).await
}

/// Pull the first layer of an arbitrary OCI artifact (qcow2 or generic
/// `application/octet-stream` payload like an image-catalog YAML). The
/// response body is returned as `Vec<u8>`.
pub async fn pull_artifact_layer(reference_str: &str) -> Result<Vec<u8>> {
    let reference: Reference =
        reference_str
            .parse()
            .map_err(|e: oci_client::ParseError| VmError::OciPullFailed {
                reference: reference_str.to_string(),
                detail: format!("invalid OCI reference: {e}"),
            })?;

    let auth = resolve_auth(&reference);
    let client = build_client();

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
///
/// Resolution order (first match wins):
///
/// 1. **`OCI_AUTH_FILE`** — path to a JSON file shaped like forger's auth
///    file: `{"username": "...", "password": "..."}` or `{"token": "..."}`
///    (with optional `username`; defaults to `"forger"`). Applied to every
///    non-ghcr registry call. Fields `password` and `token` are
///    interchangeable; this matches refraction-forger's `resolve_auth`.
/// 2. **`REGISTRY_USERNAME` / `REGISTRY_TOKEN`** env vars — same effect
///    as `OCI_AUTH_FILE` but inline. Useful for compose-style deployments
///    that prefer secrets via env. Applied when the file isn't set.
/// 3. **`GITHUB_TOKEN`** — used only for `ghcr.io`, with username
///    `_token`. Preserved for back-compat.
/// 4. **`RegistryAuth::Anonymous`** — fallback.
///
/// The env-driven path makes the orchestrator able to pull from private
/// Forgejo / Harbor / Quay packages without code changes per registry.
fn resolve_auth(reference: &Reference) -> RegistryAuth {
    let registry = reference.registry();

    if registry != "ghcr.io" {
        if let Some(auth) = auth_from_file_env() {
            return auth;
        }
        if let Some(auth) = auth_from_inline_env() {
            return auth;
        }
    }

    if registry == "ghcr.io" {
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            return RegistryAuth::Basic("_token".to_string(), token);
        }
    }
    RegistryAuth::Anonymous
}

fn auth_from_file_env() -> Option<RegistryAuth> {
    let path = std::env::var("OCI_AUTH_FILE").ok()?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| tracing::warn!(path = %path, error = %e, "OCI_AUTH_FILE unreadable"))
        .ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| tracing::warn!(path = %path, error = %e, "OCI_AUTH_FILE not valid JSON"))
        .ok()?;
    let username = v
        .get("username")
        .and_then(|x| x.as_str())
        .unwrap_or("forger")
        .to_string();
    let password = v
        .get("password")
        .or_else(|| v.get("token"))
        .and_then(|x| x.as_str())?
        .to_string();
    Some(RegistryAuth::Basic(username, password))
}

fn auth_from_inline_env() -> Option<RegistryAuth> {
    // Token comes from REGISTRY_TOKEN; username from REGISTRY_USERNAME
    // (default "forger" — matches forger's default and the common
    // Forgejo-bot convention).
    let password = std::env::var("REGISTRY_TOKEN").ok()?;
    let username = std::env::var("REGISTRY_USERNAME").unwrap_or_else(|_| "forger".to_string());
    Some(RegistryAuth::Basic(username, password))
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
