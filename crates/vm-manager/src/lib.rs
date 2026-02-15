pub mod backends;
pub mod cloudinit;
pub mod error;
pub mod image;
pub mod oci;
pub mod provision;
pub mod ssh;
pub mod traits;
pub mod types;
pub mod vmfile;

// Re-export key types at crate root for convenience.
pub use backends::RouterHypervisor;
pub use error::{Result, VmError};
pub use traits::{ConsoleEndpoint, Hypervisor};
pub use types::*;
