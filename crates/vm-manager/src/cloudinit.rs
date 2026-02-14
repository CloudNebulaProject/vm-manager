use std::path::Path;

use crate::error::{Result, VmError};

/// Create a NoCloud seed ISO from raw user-data and meta-data byte slices.
///
/// If the `pure-iso` feature is enabled, uses the `isobemak` crate to build the ISO entirely in
/// Rust. Otherwise falls back to external `genisoimage` or `mkisofs`.
pub fn create_nocloud_iso_raw(user_data: &[u8], meta_data: &[u8], out_iso: &Path) -> Result<()> {
    use std::fs;
    use std::io::Write;

    // Ensure output directory exists
    if let Some(parent) = out_iso.parent() {
        fs::create_dir_all(parent)?;
    }

    #[cfg(feature = "pure-iso")]
    {
        use isobemak::{BootInfo, IsoImage, IsoImageFile, build_iso};
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom};
        use tempfile::NamedTempFile;
        use tracing::info;

        info!(path = %out_iso.display(), "creating cloud-init ISO via isobemak (pure Rust)");

        let mut tmp_user = NamedTempFile::new()?;
        tmp_user.write_all(user_data)?;
        let user_path = tmp_user.path().to_path_buf();

        let mut tmp_meta = NamedTempFile::new()?;
        tmp_meta.write_all(meta_data)?;
        let meta_path = tmp_meta.path().to_path_buf();

        let image = IsoImage {
            files: vec![
                IsoImageFile {
                    source: user_path,
                    destination: "user-data".to_string(),
                },
                IsoImageFile {
                    source: meta_path,
                    destination: "meta-data".to_string(),
                },
            ],
            boot_info: BootInfo {
                bios_boot: None,
                uefi_boot: None,
            },
        };

        build_iso(out_iso, &image, false).map_err(|e| VmError::CloudInitIsoFailed {
            detail: format!("isobemak: {e}"),
        })?;

        // Patch the PVD volume identifier to "CIDATA" (ISO 9660 Section 8.4.3).
        const SECTOR_SIZE: u64 = 2048;
        const PVD_LBA: u64 = 16;
        const VOLID_OFFSET: u64 = 40;
        const VOLID_LEN: usize = 32;

        let mut f = OpenOptions::new().read(true).write(true).open(out_iso)?;
        let offset = PVD_LBA * SECTOR_SIZE + VOLID_OFFSET;
        f.seek(SeekFrom::Start(offset))?;
        let mut buf = [b' '; VOLID_LEN];
        let label = b"CIDATA";
        buf[..label.len()].copy_from_slice(label);
        f.write_all(&buf)?;

        return Ok(());
    }

    #[cfg(not(feature = "pure-iso"))]
    {
        use std::fs::File;
        use std::process::{Command, Stdio};
        use tempfile::tempdir;

        let dir = tempdir()?;
        let seed_path = dir.path();

        let user_data_path = seed_path.join("user-data");
        let meta_data_path = seed_path.join("meta-data");

        {
            let mut f = File::create(&user_data_path)?;
            f.write_all(user_data)?;
        }
        {
            let mut f = File::create(&meta_data_path)?;
            f.write_all(meta_data)?;
        }

        // Try genisoimage first, then mkisofs.
        let status = Command::new("genisoimage")
            .arg("-quiet")
            .arg("-output")
            .arg(out_iso)
            .arg("-volid")
            .arg("cidata")
            .arg("-joliet")
            .arg("-rock")
            .arg(&user_data_path)
            .arg(&meta_data_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        let status = match status {
            Ok(s) => s,
            Err(_) => Command::new("mkisofs")
                .arg("-quiet")
                .arg("-output")
                .arg(out_iso)
                .arg("-volid")
                .arg("cidata")
                .arg("-joliet")
                .arg("-rock")
                .arg(&user_data_path)
                .arg(&meta_data_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?,
        };

        if !status.success() {
            return Err(VmError::CloudInitIsoFailed {
                detail: "genisoimage/mkisofs exited with non-zero status".into(),
            });
        }

        Ok(())
    }
}

/// Convenience: build cloud-config YAML from user/SSH key params, then create the ISO.
pub fn create_nocloud_iso(
    user: &str,
    ssh_pubkey: &str,
    instance_id: &str,
    hostname: &str,
    out_iso: &Path,
) -> Result<()> {
    let (user_data, meta_data) = build_cloud_config(user, ssh_pubkey, instance_id, hostname);
    create_nocloud_iso_raw(&user_data, &meta_data, out_iso)
}

/// Build a minimal cloud-config user-data and meta-data from parameters.
///
/// Returns `(user_data_bytes, meta_data_bytes)`.
pub fn build_cloud_config(
    user: &str,
    ssh_pubkey: &str,
    instance_id: &str,
    hostname: &str,
) -> (Vec<u8>, Vec<u8>) {
    let user_data = format!(
        r#"#cloud-config
users:
  - name: {user}
    groups: [sudo]
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    ssh_authorized_keys:
      - {ssh_pubkey}
ssh_pwauth: false
disable_root: true
chpasswd:
  expire: false
"#
    );

    let meta_data = format!("instance-id: {instance_id}\nlocal-hostname: {hostname}\n");

    (user_data.into_bytes(), meta_data.into_bytes())
}
