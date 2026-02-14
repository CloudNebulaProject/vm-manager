use clap::Args;
use miette::{IntoDiagnostic, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vm_manager::{ConsoleEndpoint, Hypervisor, RouterHypervisor};

use super::state;

#[derive(Args)]
pub struct ConsoleArgs {
    /// VM name
    name: String,
}

pub async fn run(args: ConsoleArgs) -> Result<()> {
    let store = state::load_store().await?;
    let handle = store
        .get(&args.name)
        .ok_or_else(|| miette::miette!("VM '{}' not found", args.name))?;

    let hv = RouterHypervisor::new(None, None);
    let endpoint = hv.console_endpoint(handle).into_diagnostic()?;

    match endpoint {
        ConsoleEndpoint::UnixSocket(path) => {
            println!(
                "Connecting to console at {} (Ctrl+] to detach)...",
                path.display()
            );
            let mut sock = tokio::net::UnixStream::connect(&path)
                .await
                .into_diagnostic()?;

            let mut stdin = tokio::io::stdin();
            let mut stdout = tokio::io::stdout();

            let (mut read_half, mut write_half) = sock.split();

            // Bridge stdin/stdout to socket
            let to_sock = async {
                let mut buf = [0u8; 1024];
                loop {
                    let n = stdin.read(&mut buf).await?;
                    if n == 0 {
                        break;
                    }
                    // Check for Ctrl+] (0x1d) to detach
                    if buf[..n].contains(&0x1d) {
                        break;
                    }
                    write_half.write_all(&buf[..n]).await?;
                }
                Ok::<_, std::io::Error>(())
            };

            let from_sock = async {
                let mut buf = [0u8; 1024];
                loop {
                    let n = read_half.read(&mut buf).await?;
                    if n == 0 {
                        break;
                    }
                    stdout.write_all(&buf[..n]).await?;
                    stdout.flush().await?;
                }
                Ok::<_, std::io::Error>(())
            };

            tokio::select! {
                r = to_sock => { let _ = r; }
                r = from_sock => { let _ = r; }
            }

            println!("\nDetached from console.");
        }
        ConsoleEndpoint::WebSocket(url) => {
            println!("Console available at WebSocket: {url}");
            println!("Use a WebSocket client to connect.");
        }
        ConsoleEndpoint::None => {
            println!("No console available for this backend.");
        }
    }

    Ok(())
}
