//! Serial console log tailing for VM backends.
//!
//! Provides an async interface to tail serial console output from a running VM.
//! The console is accessed via the backend's [`ConsoleEndpoint`] — for QEMU this
//! is a Unix domain socket, for Propolis a WebSocket.
//!
//! # Example
//!
//! ```rust,no_run
//! use vm_manager::console::ConsoleTailer;
//! use vm_manager::ConsoleEndpoint;
//! use tokio::sync::watch;
//!
//! # async fn example() -> vm_manager::Result<()> {
//! let endpoint = ConsoleEndpoint::UnixSocket("/tmp/vm/console.sock".into());
//! let (stop_tx, stop_rx) = watch::channel(false);
//! let (line_tx, mut line_rx) = tokio::sync::mpsc::channel(256);
//!
//! // Spawn the tailer
//! tokio::spawn(ConsoleTailer::tail(endpoint, line_tx, stop_rx));
//!
//! // Receive lines
//! while let Some(line) = line_rx.recv().await {
//!     println!("console: {}", line);
//! }
//! # Ok(())
//! # }
//! ```

use std::path::Path;

use tokio::io::AsyncBufReadExt;
use tokio::sync::{mpsc, watch};
use tracing::{debug, warn};

use crate::error::{Result, VmError};
use crate::traits::ConsoleEndpoint;

/// Tails a VM serial console and sends lines to a channel.
pub struct ConsoleTailer;

impl ConsoleTailer {
    /// Connect to the console endpoint and stream lines to `tx` until `stop`
    /// is signalled or the connection is closed.
    ///
    /// This function is designed to be spawned as a tokio task. It returns
    /// `Ok(())` when the stop signal is received or the console stream ends.
    pub async fn tail(
        endpoint: ConsoleEndpoint,
        tx: mpsc::Sender<String>,
        mut stop: watch::Receiver<bool>,
    ) -> Result<()> {
        match endpoint {
            ConsoleEndpoint::UnixSocket(path) => Self::tail_unix_socket(&path, tx, &mut stop).await,
            ConsoleEndpoint::WebSocket(_url) => {
                // TODO: WebSocket console tailing for Propolis
                warn!("WebSocket console tailing not yet implemented");
                Ok(())
            }
            ConsoleEndpoint::None => {
                debug!("no console endpoint available, skipping tail");
                Ok(())
            }
        }
    }

    /// Tail a QEMU serial console via Unix domain socket.
    ///
    /// QEMU's chardev socket is configured with `server=on,wait=off`, meaning
    /// QEMU listens and we connect as a client. The socket emits serial output
    /// as raw bytes — we buffer and split on newlines.
    async fn tail_unix_socket(
        path: &Path,
        tx: mpsc::Sender<String>,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        // Wait for the socket to appear (QEMU may not have created it yet)
        let stream = Self::connect_with_retry(path, stop).await?;
        let reader = tokio::io::BufReader::new(stream);
        let mut lines = reader.lines();

        debug!(path = %path.display(), "console tailer connected");

        loop {
            tokio::select! {
                _ = stop.changed() => {
                    if *stop.borrow() {
                        debug!("console tailer stopped by signal");
                        break;
                    }
                }
                result = lines.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            if tx.send(line).await.is_err() {
                                // Receiver dropped — stop tailing
                                debug!("console line receiver dropped, stopping tailer");
                                break;
                            }
                        }
                        Ok(None) => {
                            // EOF — socket closed (VM stopped or QEMU exited)
                            debug!("console socket closed (EOF)");
                            break;
                        }
                        Err(e) => {
                            // I/O error — log and stop
                            warn!(error = %e, "console read error");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Try to connect to the Unix socket, retrying until the socket appears
    /// or the stop signal is received. Retries every 500ms for up to 30s.
    async fn connect_with_retry(
        path: &Path,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<tokio::net::UnixStream> {
        let max_attempts = 60;
        let interval = std::time::Duration::from_millis(500);

        for attempt in 1..=max_attempts {
            if *stop.borrow() {
                return Err(VmError::Io(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "stopped before console connected",
                )));
            }

            match tokio::net::UnixStream::connect(path).await {
                Ok(stream) => return Ok(stream),
                Err(e) if attempt < max_attempts => {
                    debug!(
                        attempt,
                        path = %path.display(),
                        error = %e,
                        "console socket not ready, retrying"
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => {}
                        _ = stop.changed() => {
                            if *stop.borrow() {
                                return Err(VmError::Io(std::io::Error::new(
                                    std::io::ErrorKind::Interrupted,
                                    "stopped while waiting for console socket",
                                )));
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "console socket connect failed after {max_attempts} attempts"
                    );
                    return Err(VmError::Io(e));
                }
            }
        }

        unreachable!()
    }
}

/// Read the console log file (if it exists) and return all lines.
///
/// This is a fallback for when the Unix socket is not available or the VM
/// has already stopped. QEMU writes console output to a log file alongside
/// the socket (configured via `logfile=` in the chardev).
pub async fn read_console_log(work_dir: &Path) -> Result<Vec<String>> {
    let log_path = work_dir.join("console.log");
    if !log_path.exists() {
        return Ok(vec![]);
    }

    let content = tokio::fs::read_to_string(&log_path).await?;
    Ok(content.lines().map(|l| l.to_string()).collect())
}
