pub mod console;
pub mod create;
pub mod destroy;
pub mod image;
pub mod list;
pub mod ssh;
pub mod start;
pub mod state;
pub mod status;
pub mod stop;

use clap::{Parser, Subcommand};
use miette::Result;

#[derive(Parser)]
#[command(name = "vmctl", about = "Manage virtual machines", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new VM (and optionally start it)
    Create(create::CreateArgs),
    /// Start an existing VM
    Start(start::StartArgs),
    /// Stop a running VM
    Stop(stop::StopArgs),
    /// Destroy a VM and clean up all resources
    Destroy(destroy::DestroyArgs),
    /// List all VMs
    List(list::ListArgs),
    /// Show VM status
    Status(status::StatusArgs),
    /// Attach to a VM's serial console
    Console(console::ConsoleArgs),
    /// SSH into a VM
    Ssh(ssh::SshArgs),
    /// Suspend a running VM (pause vCPUs)
    Suspend(start::SuspendArgs),
    /// Resume a suspended VM
    Resume(start::ResumeArgs),
    /// Manage VM images
    Image(image::ImageCommand),
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Command::Create(args) => create::run(args).await,
            Command::Start(args) => start::run_start(args).await,
            Command::Stop(args) => stop::run(args).await,
            Command::Destroy(args) => destroy::run(args).await,
            Command::List(args) => list::run(args).await,
            Command::Status(args) => status::run(args).await,
            Command::Console(args) => console::run(args).await,
            Command::Ssh(args) => ssh::run(args).await,
            Command::Suspend(args) => start::run_suspend(args).await,
            Command::Resume(args) => start::run_resume(args).await,
            Command::Image(args) => image::run(args).await,
        }
    }
}
