use std::cmp::min;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tracing::info;

use crate::error::{Result, VmError};

/// Returns the default image cache directory: `{XDG_DATA_HOME}/vmctl/images/`.
pub fn cache_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("vmctl")
        .join("images")
}

/// Streaming image downloader with progress logging and zstd decompression support.
pub struct ImageManager {
    client: reqwest::Client,
    cache: PathBuf,
}

impl Default for ImageManager {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: cache_dir(),
        }
    }
}

impl ImageManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cache_dir(cache: PathBuf) -> Self {
        Self {
            client: reqwest::Client::new(),
            cache,
        }
    }

    /// Download an image from `url` to `destination`.
    ///
    /// If the file already exists at `destination`, the download is skipped.
    /// URLs ending in `.zst` or `.zstd` are automatically decompressed.
    pub async fn download(&self, url: &str, destination: &Path) -> Result<()> {
        if destination.exists() {
            info!(url = %url, dest = %destination.display(), "image already present; skipping download");
            return Ok(());
        }

        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let is_zstd = url.ends_with(".zst") || url.ends_with(".zstd");

        if is_zstd {
            self.download_zstd(url, destination).await
        } else {
            self.download_raw(url, destination).await
        }
    }

    /// Pull an image from a URL into the cache directory, returning the cached path.
    pub async fn pull(&self, url: &str, name: Option<&str>) -> Result<PathBuf> {
        let file_name = name.map(|n| n.to_string()).unwrap_or_else(|| {
            url.rsplit('/')
                .next()
                .unwrap_or("image")
                .trim_end_matches(".zst")
                .trim_end_matches(".zstd")
                .to_string()
        });
        let dest = self.cache.join(&file_name);
        self.download(url, &dest).await?;
        Ok(dest)
    }

    /// List all cached images.
    pub async fn list(&self) -> Result<Vec<CachedImage>> {
        let mut entries = Vec::new();
        let cache = &self.cache;
        if !cache.exists() {
            return Ok(entries);
        }
        let mut dir = tokio::fs::read_dir(cache).await?;
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                let metadata = entry.metadata().await?;
                entries.push(CachedImage {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path,
                    size_bytes: metadata.len(),
                });
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    async fn download_zstd(&self, url: &str, destination: &Path) -> Result<()> {
        let res = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| VmError::ImageDownloadFailed {
                url: url.into(),
                detail: e.to_string(),
            })?;

        let total_size = res.content_length().unwrap_or(0);

        let tmp_name = format!(
            "{}.zst.tmp",
            destination
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default()
        );
        let tmp_path = destination
            .parent()
            .map(|p| p.join(&tmp_name))
            .unwrap_or_else(|| PathBuf::from(&tmp_name));

        info!(url = %url, dest = %destination.display(), size_bytes = total_size, "downloading image (zstd)");

        // Stream to temp compressed file
        {
            let mut tmp_file = std::fs::File::create(&tmp_path)?;
            let mut downloaded: u64 = 0;
            let mut stream = res.bytes_stream();
            let mut last_logged_pct: u64 = 0;
            while let Some(item) = stream.next().await {
                let chunk = item.map_err(|e| VmError::ImageDownloadFailed {
                    url: url.into(),
                    detail: e.to_string(),
                })?;
                std::io::Write::write_all(&mut tmp_file, &chunk)?;
                if total_size > 0 {
                    downloaded = min(downloaded + (chunk.len() as u64), total_size);
                    let pct = downloaded.saturating_mul(100) / total_size.max(1);
                    if pct >= last_logged_pct + 5 || pct == 100 {
                        info!(
                            percent = pct,
                            downloaded_mb = (downloaded as f64) / 1_000_000.0,
                            "downloading (zstd)..."
                        );
                        last_logged_pct = pct;
                    }
                }
            }
        }

        info!(tmp = %tmp_path.display(), "download complete; decompressing zstd");

        // Decompress
        let infile = std::fs::File::open(&tmp_path)?;
        let mut decoder =
            zstd::stream::Decoder::new(infile).map_err(|e| VmError::ImageDownloadFailed {
                url: url.into(),
                detail: format!("zstd decoder init: {e}"),
            })?;
        let mut outfile = std::fs::File::create(destination)?;
        std::io::copy(&mut decoder, &mut outfile)?;
        let _ = decoder.finish();
        let _ = std::fs::remove_file(&tmp_path);

        info!(dest = %destination.display(), "decompression completed");
        Ok(())
    }

    async fn download_raw(&self, url: &str, destination: &Path) -> Result<()> {
        let res = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| VmError::ImageDownloadFailed {
                url: url.into(),
                detail: e.to_string(),
            })?;

        let total_size = res.content_length().unwrap_or(0);

        info!(url = %url, dest = %destination.display(), size_bytes = total_size, "downloading image");

        let mut file = std::fs::File::create(destination)?;
        let mut downloaded: u64 = 0;
        let mut stream = res.bytes_stream();
        let mut last_logged_pct: u64 = 0;

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| VmError::ImageDownloadFailed {
                url: url.into(),
                detail: e.to_string(),
            })?;
            std::io::Write::write_all(&mut file, &chunk)?;
            if total_size > 0 {
                downloaded = min(downloaded + (chunk.len() as u64), total_size);
                let pct = downloaded.saturating_mul(100) / total_size.max(1);
                if pct >= last_logged_pct + 5 || pct == 100 {
                    info!(
                        percent = pct,
                        downloaded_mb = (downloaded as f64) / 1_000_000.0,
                        "downloading..."
                    );
                    last_logged_pct = pct;
                }
            }
        }

        info!(dest = %destination.display(), "download completed");
        Ok(())
    }
}

/// Information about a cached image.
#[derive(Debug, Clone)]
pub struct CachedImage {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// Detect the format of a disk image using `qemu-img info`.
pub async fn detect_format(path: &Path) -> Result<String> {
    let output = tokio::process::Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(path)
        .output()
        .await
        .map_err(|e| VmError::ImageFormatDetectionFailed {
            path: path.into(),
            detail: format!("qemu-img not found: {e}"),
        })?;

    if !output.status.success() {
        return Err(VmError::ImageFormatDetectionFailed {
            path: path.into(),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let info: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| {
        VmError::ImageFormatDetectionFailed {
            path: path.into(),
            detail: format!("failed to parse qemu-img JSON: {e}"),
        }
    })?;

    Ok(info
        .get("format")
        .and_then(|f| f.as_str())
        .unwrap_or("raw")
        .to_string())
}

/// Convert an image from one format to another using `qemu-img convert`.
pub async fn convert(src: &Path, dst: &Path, output_format: &str) -> Result<()> {
    let output = tokio::process::Command::new("qemu-img")
        .args(["convert", "-O", output_format])
        .arg(src)
        .arg(dst)
        .output()
        .await
        .map_err(|e| VmError::ImageConversionFailed {
            detail: format!("qemu-img convert failed to start: {e}"),
        })?;

    if !output.status.success() {
        return Err(VmError::ImageConversionFailed {
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(())
}

/// Create a QCOW2 overlay backed by a base image.
///
/// Automatically detects the base image format. If `size_gb` is provided, the overlay is resized.
pub async fn create_overlay(base: &Path, overlay: &Path, size_gb: Option<u32>) -> Result<()> {
    let base_fmt = detect_format(base).await?;

    let mut args = vec![
        "create".to_string(),
        "-f".into(),
        "qcow2".into(),
        "-F".into(),
        base_fmt,
        "-b".into(),
        base.to_string_lossy().into_owned(),
        overlay.to_string_lossy().into_owned(),
    ];

    if let Some(gb) = size_gb {
        args.push(format!("{gb}G"));
    }

    let output = tokio::process::Command::new("qemu-img")
        .args(&args)
        .output()
        .await
        .map_err(|e| VmError::OverlayCreationFailed {
            base: base.into(),
            detail: format!("qemu-img not found: {e}"),
        })?;

    if !output.status.success() {
        return Err(VmError::OverlayCreationFailed {
            base: base.into(),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(())
}
