use std::path::PathBuf;

use clap::{Args, Subcommand};
use miette::{IntoDiagnostic, Result};

#[derive(Args)]
pub struct ImageCommand {
    #[command(subcommand)]
    action: ImageAction,
}

#[derive(Subcommand)]
enum ImageAction {
    /// Download an image to the local cache
    Pull(PullArgs),
    /// List cached images
    List,
    /// Show image format and details
    Inspect(InspectArgs),
}

#[derive(Args)]
struct PullArgs {
    /// URL to download
    url: String,

    /// Name to save as in the cache
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args)]
struct InspectArgs {
    /// Path to the image file
    path: PathBuf,
}

pub async fn run(args: ImageCommand) -> Result<()> {
    match args.action {
        ImageAction::Pull(pull) => {
            let mgr = vm_manager::image::ImageManager::new();
            let path = mgr
                .pull(&pull.url, pull.name.as_deref())
                .await
                .into_diagnostic()?;
            println!("Image cached at: {}", path.display());
        }
        ImageAction::List => {
            let mgr = vm_manager::image::ImageManager::new();
            let images = mgr.list().await.into_diagnostic()?;

            if images.is_empty() {
                println!("No cached images.");
                return Ok(());
            }

            println!("{:<40} {:<12} PATH", "NAME", "SIZE");
            println!("{}", "-".repeat(80));

            for img in images {
                let size = if img.size_bytes >= 1_073_741_824 {
                    format!("{:.1} GB", img.size_bytes as f64 / 1_073_741_824.0)
                } else {
                    format!("{:.1} MB", img.size_bytes as f64 / 1_048_576.0)
                };
                println!("{:<40} {:<12} {}", img.name, size, img.path.display());
            }
        }
        ImageAction::Inspect(inspect) => {
            let fmt = vm_manager::image::detect_format(&inspect.path)
                .await
                .into_diagnostic()?;
            println!("Format: {}", fmt);
            println!("Path:   {}", inspect.path.display());

            if let Ok(meta) = tokio::fs::metadata(&inspect.path).await {
                let size = meta.len();
                if size >= 1_073_741_824 {
                    println!("Size:   {:.1} GB", size as f64 / 1_073_741_824.0);
                } else {
                    println!("Size:   {:.1} MB", size as f64 / 1_048_576.0);
                }
            }
        }
    }

    Ok(())
}
