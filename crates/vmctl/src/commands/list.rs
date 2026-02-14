use clap::Args;
use miette::Result;

use super::state;

#[derive(Args)]
pub struct ListArgs;

pub async fn run(_args: ListArgs) -> Result<()> {
    let store = state::load_store().await?;

    if store.is_empty() {
        println!("No VMs found.");
        return Ok(());
    }

    println!("{:<20} {:<12} {:<40} WORK DIR", "NAME", "BACKEND", "ID");
    println!("{}", "-".repeat(90));

    let mut entries: Vec<_> = store.iter().collect();
    entries.sort_by_key(|(name, _)| (*name).clone());

    for (name, handle) in entries {
        println!(
            "{:<20} {:<12} {:<40} {}",
            name,
            handle.backend,
            handle.id,
            handle.work_dir.display()
        );
    }

    Ok(())
}
