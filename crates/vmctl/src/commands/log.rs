use clap::Args;
use miette::{IntoDiagnostic, Result};

use super::state;

#[derive(Args)]
pub struct LogArgs {
    /// VM name
    name: String,

    /// Show only console log (boot / cloud-init)
    #[arg(long)]
    console: bool,

    /// Show only provision log
    #[arg(long)]
    provision: bool,

    /// Show the last N lines (0 = all)
    #[arg(long, short = 'n', default_value = "0")]
    tail: usize,
}

pub async fn run(args: LogArgs) -> Result<()> {
    let store = state::load_store().await?;
    let handle = store
        .get(&args.name)
        .ok_or_else(|| miette::miette!("VM '{}' not found", args.name))?;

    // If neither flag is set, show both
    let show_console = args.console || !args.provision;
    let show_provision = args.provision || !args.console;

    if show_console {
        let path = handle.work_dir.join("console.log");
        print_log("console", &path, args.tail).await?;
    }

    if show_provision {
        let path = handle.work_dir.join("provision.log");
        print_log("provision", &path, args.tail).await?;
    }

    Ok(())
}

async fn print_log(label: &str, path: &std::path::Path, tail: usize) -> Result<()> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => {
            println!("=== {label} log ({}) ===", path.display());
            if tail > 0 {
                let lines: Vec<&str> = content.lines().collect();
                let start = lines.len().saturating_sub(tail);
                for line in &lines[start..] {
                    println!("{line}");
                }
            } else {
                print!("{content}");
            }
            println!();
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("=== {label} log: not found (VM may not have been started yet) ===");
            println!();
        }
        Err(e) => {
            return Err(e).into_diagnostic();
        }
    }
    Ok(())
}
