//! Persistent state for vmctl: maps VM name → VmHandle in a JSON file.

use std::collections::HashMap;
use std::path::PathBuf;

use miette::{IntoDiagnostic, Result};
use vm_manager::VmHandle;

/// State file location: `{XDG_DATA_HOME}/vmctl/vms.json`
fn state_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("vmctl")
        .join("vms.json")
}

pub type Store = HashMap<String, VmHandle>;

/// Load the VM store from disk. Returns an empty map if the file doesn't exist.
pub async fn load_store() -> Result<Store> {
    let path = state_path();
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = tokio::fs::read_to_string(&path).await.into_diagnostic()?;
    let store: Store = serde_json::from_str(&data).into_diagnostic()?;
    Ok(store)
}

/// Save the VM store to disk.
pub async fn save_store(store: &Store) -> Result<()> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.into_diagnostic()?;
    }
    let data = serde_json::to_string_pretty(store).into_diagnostic()?;
    tokio::fs::write(&path, data).await.into_diagnostic()?;
    Ok(())
}
