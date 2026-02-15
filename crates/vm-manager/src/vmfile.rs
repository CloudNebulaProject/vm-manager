use std::collections::HashSet;
use std::path::{Path, PathBuf};

use kdl::KdlDocument;
use tracing::info;

use crate::cloudinit::build_cloud_config;
use crate::error::{Result, VmError};
use crate::image::ImageManager;
use crate::types::{CloudInitConfig, NetworkConfig, SshConfig, VmSpec};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A parsed VMFile containing one or more VM definitions.
#[derive(Debug, Clone)]
pub struct VmFile {
    /// Directory containing the VMFile (used for relative path resolution).
    pub base_dir: PathBuf,
    /// Ordered list of VM definitions.
    pub vms: Vec<VmDef>,
}

/// A single VM definition from a VMFile.
#[derive(Debug, Clone)]
pub struct VmDef {
    pub name: String,
    pub image: ImageSource,
    pub vcpus: u16,
    pub memory_mb: u64,
    pub disk_gb: Option<u32>,
    pub network: NetworkDef,
    pub cloud_init: Option<CloudInitDef>,
    pub ssh: Option<SshDef>,
    pub provisions: Vec<ProvisionDef>,
}

/// Where to source the VM image from.
#[derive(Debug, Clone)]
pub enum ImageSource {
    Local(String),
    Url(String),
    Oci(String),
}

/// Network mode as declared in the VMFile.
#[derive(Debug, Clone, Default)]
pub enum NetworkDef {
    #[default]
    User,
    Tap {
        bridge: String,
    },
    None,
}

/// Cloud-init configuration block.
#[derive(Debug, Clone)]
pub struct CloudInitDef {
    pub hostname: Option<String>,
    pub ssh_key: Option<String>,
    pub user_data: Option<String>,
}

/// SSH connection configuration block.
#[derive(Debug, Clone)]
pub struct SshDef {
    pub user: String,
    /// Path to an existing private key file. When `None`, a per-VM Ed25519
    /// keypair is generated at resolve time and used via in-memory PEM.
    pub private_key: Option<String>,
}

/// A provisioning step.
#[derive(Debug, Clone)]
pub enum ProvisionDef {
    Shell(ShellProvision),
    File(FileProvision),
}

#[derive(Debug, Clone)]
pub struct ShellProvision {
    pub inline: Option<String>,
    pub script: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileProvision {
    pub source: String,
    pub destination: String,
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Expand `~` at the start of a path to the user's home directory.
pub fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/root"))
            .join(rest)
    } else if s == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"))
    } else {
        PathBuf::from(s)
    }
}

/// Resolve a raw path string: expand tilde, then make relative paths absolute against `base_dir`.
pub fn resolve_path(raw: &str, base_dir: &Path) -> PathBuf {
    let expanded = expand_tilde(raw);
    if expanded.is_absolute() {
        expanded
    } else {
        base_dir.join(expanded)
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Find a VMFile.kdl — use the explicit path if given, otherwise look in the current directory.
pub fn discover(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        let p = path.to_path_buf();
        if p.exists() {
            Ok(p)
        } else {
            Err(VmError::VmFileNotFound { path: p })
        }
    } else {
        let p = PathBuf::from("VMFile.kdl");
        if p.exists() {
            Ok(p)
        } else {
            Err(VmError::VmFileNotFound { path: p })
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a VMFile.kdl at the given path into a `VmFile`.
pub fn parse(path: &Path) -> Result<VmFile> {
    let content = std::fs::read_to_string(path).map_err(|e| VmError::VmFileParseFailed {
        location: path.display().to_string(),
        detail: format!("could not read file: {e}"),
    })?;

    let doc: KdlDocument =
        content
            .parse()
            .map_err(|e: kdl::KdlError| VmError::VmFileParseFailed {
                location: path.display().to_string(),
                detail: e.to_string(),
            })?;

    let base_dir = path
        .parent()
        .map(|p| {
            if p.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                p.to_path_buf()
            }
        })
        .unwrap_or_else(|| PathBuf::from("."));

    let mut vms = Vec::new();
    let mut seen_names = HashSet::new();

    for node in doc.nodes() {
        if node.name().to_string() != "vm" {
            continue;
        }

        let name = node
            .get(0)
            .and_then(|v| v.as_string())
            .ok_or_else(|| VmError::VmFileValidation {
                vm: "<unknown>".into(),
                detail: "vm node must have a name argument".into(),
                hint: "add a name: vm \"my-server\" { ... }".into(),
            })?
            .to_string();

        if !seen_names.insert(name.clone()) {
            return Err(VmError::VmFileValidation {
                vm: name,
                detail: "duplicate VM name".into(),
                hint: "each vm must have a unique name".into(),
            });
        }

        let children = node.children().ok_or_else(|| VmError::VmFileValidation {
            vm: name.clone(),
            detail: "vm node must have a body".into(),
            hint: "add configuration inside braces: vm \"name\" { ... }".into(),
        })?;

        let vm_def = parse_vm_def(&name, children)?;
        vms.push(vm_def);
    }

    if vms.is_empty() {
        return Err(VmError::VmFileParseFailed {
            location: path.display().to_string(),
            detail: "no vm definitions found".into(),
        });
    }

    Ok(VmFile { base_dir, vms })
}

fn parse_vm_def(name: &str, doc: &KdlDocument) -> Result<VmDef> {
    // Image: local or URL
    let local_image = doc
        .get_arg("image")
        .and_then(|v| v.as_string())
        .map(String::from);
    let url_image = doc
        .get_arg("image-url")
        .and_then(|v| v.as_string())
        .map(String::from);

    let image = match (local_image, url_image) {
        (Some(path), None) => ImageSource::Local(path),
        (None, Some(url)) if url.starts_with("oci://") => ImageSource::Oci(url[6..].to_string()),
        (None, Some(url)) => ImageSource::Url(url),
        (Some(_), Some(_)) => {
            return Err(VmError::VmFileValidation {
                vm: name.into(),
                detail: "both image and image-url specified".into(),
                hint: "use either image or image-url, not both".into(),
            });
        }
        (None, None) => {
            return Err(VmError::VmFileValidation {
                vm: name.into(),
                detail: "no image specified".into(),
                hint: "add image \"/path/to/image.qcow2\" or image-url \"https://...\"".into(),
            });
        }
    };

    let vcpus = doc
        .get_arg("vcpus")
        .and_then(|v| v.as_integer())
        .map(|v| v as u16)
        .unwrap_or(1);

    let memory_mb = doc
        .get_arg("memory")
        .and_then(|v| v.as_integer())
        .map(|v| v as u64)
        .unwrap_or(1024);

    let disk_gb = doc
        .get_arg("disk")
        .and_then(|v| v.as_integer())
        .map(|v| v as u32);

    // Network
    let network = if let Some(net_node) = doc.get("network") {
        let net_type = net_node
            .get(0)
            .and_then(|v| v.as_string())
            .unwrap_or("user");
        match net_type {
            "user" => NetworkDef::User,
            "tap" => {
                let bridge = net_node
                    .get("bridge")
                    .and_then(|v| v.as_string())
                    .unwrap_or("br0")
                    .to_string();
                NetworkDef::Tap { bridge }
            }
            "none" => NetworkDef::None,
            other => {
                return Err(VmError::VmFileValidation {
                    vm: name.into(),
                    detail: format!("unknown network type: {other}"),
                    hint: "use \"user\", \"tap\", or \"none\"".into(),
                });
            }
        }
    } else {
        NetworkDef::default()
    };

    // Cloud-init
    let cloud_init = if let Some(ci_node) = doc.get("cloud-init") {
        let ci_doc = ci_node.children();
        let hostname = ci_doc
            .and_then(|d| d.get_arg("hostname"))
            .and_then(|v| v.as_string())
            .map(String::from);
        let ssh_key = ci_doc
            .and_then(|d| d.get_arg("ssh-key"))
            .and_then(|v| v.as_string())
            .map(String::from);
        let user_data = ci_doc
            .and_then(|d| d.get_arg("user-data"))
            .and_then(|v| v.as_string())
            .map(String::from);

        Some(CloudInitDef {
            hostname,
            ssh_key,
            user_data,
        })
    } else {
        None
    };

    // SSH
    let ssh = if let Some(ssh_node) = doc.get("ssh") {
        let ssh_doc = ssh_node.children().ok_or_else(|| VmError::VmFileValidation {
            vm: name.into(),
            detail: "ssh block must have a body".into(),
            hint: "add at least a user: ssh { user \"vm\" }".into(),
        })?;
        let user = ssh_doc
            .get_arg("user")
            .and_then(|v| v.as_string())
            .unwrap_or("vm")
            .to_string();
        let private_key = ssh_doc
            .get_arg("private-key")
            .and_then(|v| v.as_string())
            .map(String::from);
        Some(SshDef { user, private_key })
    } else {
        None
    };

    // Provisions
    let mut provisions = Vec::new();
    for node in doc.nodes() {
        if node.name().to_string() != "provision" {
            continue;
        }
        let ptype = node.get(0).and_then(|v| v.as_string()).unwrap_or("shell");

        let prov_doc = node.children().ok_or_else(|| VmError::VmFileValidation {
            vm: name.into(),
            detail: "provision block must have a body".into(),
            hint: "add content inside: provision \"shell\" { inline \"...\" }".into(),
        })?;

        match ptype {
            "shell" => {
                let inline = prov_doc
                    .get_arg("inline")
                    .and_then(|v| v.as_string())
                    .map(String::from);
                let script = prov_doc
                    .get_arg("script")
                    .and_then(|v| v.as_string())
                    .map(String::from);

                if inline.is_none() && script.is_none() {
                    return Err(VmError::VmFileValidation {
                        vm: name.into(),
                        detail: "shell provision requires inline or script".into(),
                        hint: "add: inline \"command\" or script \"./setup.sh\"".into(),
                    });
                }
                if inline.is_some() && script.is_some() {
                    return Err(VmError::VmFileValidation {
                        vm: name.into(),
                        detail: "shell provision cannot have both inline and script".into(),
                        hint: "use either inline or script, not both".into(),
                    });
                }

                provisions.push(ProvisionDef::Shell(ShellProvision { inline, script }));
            }
            "file" => {
                let source = prov_doc
                    .get_arg("source")
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| VmError::VmFileValidation {
                        vm: name.into(),
                        detail: "file provision requires source".into(),
                        hint: "add: source \"./local-file.conf\"".into(),
                    })?
                    .to_string();
                let destination = prov_doc
                    .get_arg("destination")
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| VmError::VmFileValidation {
                        vm: name.into(),
                        detail: "file provision requires destination".into(),
                        hint: "add: destination \"/etc/app/config.conf\"".into(),
                    })?
                    .to_string();
                provisions.push(ProvisionDef::File(FileProvision {
                    source,
                    destination,
                }));
            }
            other => {
                return Err(VmError::VmFileValidation {
                    vm: name.into(),
                    detail: format!("unknown provision type: {other}"),
                    hint: "use \"shell\" or \"file\"".into(),
                });
            }
        }
    }

    Ok(VmDef {
        name: name.to_string(),
        image,
        vcpus,
        memory_mb,
        disk_gb,
        network,
        cloud_init,
        ssh,
        provisions,
    })
}

// ---------------------------------------------------------------------------
// Resolve: VmDef -> VmSpec
// ---------------------------------------------------------------------------

/// Resolve a `VmDef` into a ready-to-use `VmSpec` by downloading images, reading keys, etc.
pub async fn resolve(def: &VmDef, base_dir: &Path) -> Result<VmSpec> {
    // Resolve image
    let image_path = match &def.image {
        ImageSource::Local(raw) => {
            let p = resolve_path(raw, base_dir);
            if !p.exists() {
                return Err(VmError::VmFileValidation {
                    vm: def.name.clone(),
                    detail: format!("image not found: {}", p.display()),
                    hint: "check the image path is correct and the file exists".into(),
                });
            }
            p
        }
        ImageSource::Url(url) => {
            info!(vm = %def.name, url = %url, "downloading image");
            let mgr = ImageManager::new();
            mgr.pull(url, Some(&def.name)).await?
        }
        ImageSource::Oci(oci_ref) => {
            let mgr = ImageManager::new();
            mgr.pull_oci(oci_ref, Some(&def.name)).await?
        }
    };

    // Network
    let network = match &def.network {
        NetworkDef::User => NetworkConfig::User,
        NetworkDef::Tap { bridge } => NetworkConfig::Tap {
            bridge: bridge.clone(),
        },
        NetworkDef::None => NetworkConfig::None,
    };

    // Cloud-init + SSH config (resolved together because key generation affects both)
    let (cloud_init, ssh) = resolve_cloud_init_and_ssh(def, base_dir).await?;

    Ok(VmSpec {
        name: def.name.clone(),
        image_path,
        vcpus: def.vcpus,
        memory_mb: def.memory_mb,
        disk_gb: def.disk_gb,
        network,
        cloud_init,
        ssh,
    })
}

/// Generate an Ed25519 SSH keypair and return `(public_key_openssh, private_key_pem)`.
fn generate_ssh_keypair(vm_name: &str) -> Result<(String, String)> {
    use ssh_key::{Algorithm, LineEnding, PrivateKey, rand_core::OsRng};

    let sk = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).map_err(|e| {
        VmError::SshKeygenFailed {
            detail: format!("Ed25519 key generation for VM '{vm_name}': {e}"),
        }
    })?;

    let pub_openssh = sk.public_key().to_openssh().map_err(|e| {
        VmError::SshKeygenFailed {
            detail: format!("serialize public key: {e}"),
        }
    })?;

    let priv_pem = sk.to_openssh(LineEnding::LF).map_err(|e| {
        VmError::SshKeygenFailed {
            detail: format!("serialize private key: {e}"),
        }
    })?;

    Ok((pub_openssh, priv_pem.to_string()))
}

/// Resolve cloud-init and SSH config together.
///
/// When the VMFile provides a `cloud-init` block but no `ssh-key` (and no `user-data`), and the
/// `ssh` block omits `private-key`, we generate a per-VM Ed25519 keypair: the public key is
/// injected into cloud-config and the private key PEM is used for SSH auth.
async fn resolve_cloud_init_and_ssh(
    def: &VmDef,
    base_dir: &Path,
) -> Result<(Option<CloudInitConfig>, Option<SshConfig>)> {
    let ssh_user = def.ssh.as_ref().map(|s| s.user.as_str()).unwrap_or("vm");
    let hostname = def
        .cloud_init
        .as_ref()
        .and_then(|ci| ci.hostname.as_deref())
        .unwrap_or(&def.name);

    // --- Cloud-init: raw user-data file ---
    if let Some(ci) = &def.cloud_init {
        if let Some(raw_path) = &ci.user_data {
            let p = resolve_path(raw_path, base_dir);
            let data =
                tokio::fs::read(&p)
                    .await
                    .map_err(|e| VmError::VmFileValidation {
                        vm: def.name.clone(),
                        detail: format!("cannot read user-data at {}: {e}", p.display()),
                        hint: "check the user-data path".into(),
                    })?;
            let cloud_init = Some(CloudInitConfig {
                user_data: data,
                instance_id: Some(def.name.clone()),
                hostname: ci.hostname.clone().or_else(|| Some(def.name.clone())),
            });
            // SSH config from explicit key (if any)
            let ssh = resolve_ssh_config_from_def(def, base_dir);
            return Ok((cloud_init, ssh));
        }
    }

    // --- Cloud-init: explicit ssh-key file ---
    if let Some(ci) = &def.cloud_init {
        if let Some(key_raw) = &ci.ssh_key {
            let key_path = resolve_path(key_raw, base_dir);
            let pubkey =
                tokio::fs::read_to_string(&key_path)
                    .await
                    .map_err(|e| VmError::VmFileValidation {
                        vm: def.name.clone(),
                        detail: format!("cannot read ssh-key at {}: {e}", key_path.display()),
                        hint: "check the ssh-key path".into(),
                    })?;
            let (user_data, _meta) =
                build_cloud_config(ssh_user, pubkey.trim(), &def.name, hostname);
            let cloud_init = Some(CloudInitConfig {
                user_data,
                instance_id: Some(def.name.clone()),
                hostname: Some(hostname.to_string()),
            });
            let ssh = resolve_ssh_config_from_def(def, base_dir);
            return Ok((cloud_init, ssh));
        }
    }

    // --- Cloud-init block present but no ssh-key / no user-data → generate keypair ---
    if def.cloud_init.is_some() {
        info!(vm = %def.name, "generating Ed25519 SSH keypair for cloud-init");
        let (pub_openssh, priv_pem) = generate_ssh_keypair(&def.name)?;

        let (user_data, _meta) =
            build_cloud_config(ssh_user, &pub_openssh, &def.name, hostname);
        let cloud_init = Some(CloudInitConfig {
            user_data,
            instance_id: Some(def.name.clone()),
            hostname: Some(hostname.to_string()),
        });
        let ssh = Some(SshConfig {
            user: ssh_user.to_string(),
            public_key: Some(pub_openssh),
            private_key_path: None,
            private_key_pem: Some(priv_pem),
        });
        return Ok((cloud_init, ssh));
    }

    // --- No cloud-init at all ---
    let ssh = resolve_ssh_config_from_def(def, base_dir);
    Ok((None, ssh))
}

/// Build an `SshConfig` from an explicit `private-key` path in the SSH block.
/// Returns `None` if there is no ssh block or no private-key specified.
fn resolve_ssh_config_from_def(def: &VmDef, base_dir: &Path) -> Option<SshConfig> {
    def.ssh.as_ref().and_then(|s| {
        let key_path = s.private_key.as_ref()?;
        Some(SshConfig {
            user: s.user.clone(),
            public_key: None,
            private_key_path: Some(resolve_path(key_path, base_dir)),
            private_key_pem: None,
        })
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_vmfile() {
        let kdl = r#"
vm "test" {
    image "/tmp/test.qcow2"
}
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".kdl").unwrap();
        std::fs::write(tmp.path(), kdl).unwrap();

        let vmfile = parse(tmp.path()).unwrap();
        assert_eq!(vmfile.vms.len(), 1);
        let vm = &vmfile.vms[0];
        assert_eq!(vm.name, "test");
        assert!(matches!(vm.image, ImageSource::Local(ref p) if p == "/tmp/test.qcow2"));
        assert_eq!(vm.vcpus, 1);
        assert_eq!(vm.memory_mb, 1024);
        assert!(vm.disk_gb.is_none());
        assert!(matches!(vm.network, NetworkDef::User));
        assert!(vm.cloud_init.is_none());
        assert!(vm.ssh.is_none());
        assert!(vm.provisions.is_empty());
    }

    #[test]
    fn parse_full_vmfile() {
        let kdl = r#"
vm "web" {
    image "/images/ubuntu.qcow2"
    vcpus 2
    memory 2048
    disk 20
    network "tap" bridge="br0"

    cloud-init {
        hostname "webhost"
        ssh-key "~/.ssh/id_ed25519.pub"
    }

    ssh {
        user "admin"
        private-key "~/.ssh/id_ed25519"
    }

    provision "shell" {
        inline "apt update"
    }

    provision "file" {
        source "./nginx.conf"
        destination "/etc/nginx/nginx.conf"
    }
}
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".kdl").unwrap();
        std::fs::write(tmp.path(), kdl).unwrap();

        let vmfile = parse(tmp.path()).unwrap();
        let vm = &vmfile.vms[0];
        assert_eq!(vm.name, "web");
        assert_eq!(vm.vcpus, 2);
        assert_eq!(vm.memory_mb, 2048);
        assert_eq!(vm.disk_gb, Some(20));
        assert!(matches!(vm.network, NetworkDef::Tap { ref bridge } if bridge == "br0"));

        let ci = vm.cloud_init.as_ref().unwrap();
        assert_eq!(ci.hostname.as_deref(), Some("webhost"));
        assert_eq!(ci.ssh_key.as_deref(), Some("~/.ssh/id_ed25519.pub"));

        let ssh = vm.ssh.as_ref().unwrap();
        assert_eq!(ssh.user, "admin");
        assert_eq!(ssh.private_key.as_deref(), Some("~/.ssh/id_ed25519"));

        assert_eq!(vm.provisions.len(), 2);
        assert!(
            matches!(&vm.provisions[0], ProvisionDef::Shell(s) if s.inline.as_deref() == Some("apt update"))
        );
        assert!(matches!(&vm.provisions[1], ProvisionDef::File(f) if f.source == "./nginx.conf"));
    }

    #[test]
    fn parse_multi_vm() {
        let kdl = r#"
vm "alpha" {
    image "/img/a.qcow2"
}

vm "beta" {
    image "/img/b.qcow2"
    vcpus 4
    memory 4096
}
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".kdl").unwrap();
        std::fs::write(tmp.path(), kdl).unwrap();

        let vmfile = parse(tmp.path()).unwrap();
        assert_eq!(vmfile.vms.len(), 2);
        assert_eq!(vmfile.vms[0].name, "alpha");
        assert_eq!(vmfile.vms[1].name, "beta");
        assert_eq!(vmfile.vms[1].vcpus, 4);
    }

    #[test]
    fn parse_image_url() {
        let kdl = r#"
vm "cloud" {
    image-url "https://example.com/image.qcow2"
}
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".kdl").unwrap();
        std::fs::write(tmp.path(), kdl).unwrap();

        let vmfile = parse(tmp.path()).unwrap();
        assert!(
            matches!(vmfile.vms[0].image, ImageSource::Url(ref u) if u == "https://example.com/image.qcow2")
        );
    }

    #[test]
    fn error_no_image() {
        let kdl = r#"
vm "broken" {
    vcpus 1
}
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".kdl").unwrap();
        std::fs::write(tmp.path(), kdl).unwrap();

        let err = parse(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no image specified"), "got: {msg}");
    }

    #[test]
    fn error_no_name() {
        let kdl = r#"
vm {
    image "/tmp/test.qcow2"
}
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".kdl").unwrap();
        std::fs::write(tmp.path(), kdl).unwrap();

        let err = parse(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("name argument"), "got: {msg}");
    }

    #[test]
    fn error_duplicate_names() {
        let kdl = r#"
vm "dup" {
    image "/tmp/a.qcow2"
}
vm "dup" {
    image "/tmp/b.qcow2"
}
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".kdl").unwrap();
        std::fs::write(tmp.path(), kdl).unwrap();

        let err = parse(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("duplicate"), "got: {msg}");
    }

    #[test]
    fn parse_oci_image_source() {
        let kdl = r#"
vm "ci" {
    image-url "oci://ghcr.io/cloudnebulaproject/ubuntu-rust:latest"
}
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".kdl").unwrap();
        std::fs::write(tmp.path(), kdl).unwrap();

        let vmfile = parse(tmp.path()).unwrap();
        assert!(
            matches!(vmfile.vms[0].image, ImageSource::Oci(ref r) if r == "ghcr.io/cloudnebulaproject/ubuntu-rust:latest")
        );
    }

    #[test]
    fn expand_tilde_works() {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
        let result = expand_tilde("~/foo/bar");
        assert_eq!(result, home.join("foo/bar"));

        let abs = expand_tilde("/absolute/path");
        assert_eq!(abs, PathBuf::from("/absolute/path"));
    }
}
