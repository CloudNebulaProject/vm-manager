//! QMP (QEMU Machine Protocol) client over Unix domain socket.
//!
//! Implements the QMP wire protocol directly using JSON over a tokio `UnixStream`.
//! QMP is a simple line-delimited JSON protocol:
//! 1. Server sends a greeting `{"QMP": {...}}`
//! 2. Client sends `{"execute": "qmp_capabilities"}`
//! 3. Server responds `{"return": {}}`
//! 4. Client sends commands, server sends responses and events.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::{debug, info, trace};

use crate::error::{Result, VmError};

/// A connected QMP client for a single QEMU instance.
pub struct QmpClient {
    reader: BufReader<tokio::io::ReadHalf<UnixStream>>,
    writer: tokio::io::WriteHalf<UnixStream>,
}

impl QmpClient {
    /// Connect to a QMP Unix socket and negotiate capabilities.
    ///
    /// Retries the connection for up to `timeout` if the socket is not yet available.
    pub async fn connect(socket_path: &Path, timeout: Duration) -> Result<Self> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut backoff = Duration::from_millis(100);

        let stream = loop {
            match UnixStream::connect(socket_path).await {
                Ok(s) => break s,
                Err(e) => {
                    if tokio::time::Instant::now() >= deadline {
                        return Err(VmError::QmpConnectionFailed {
                            path: socket_path.into(),
                            source: e,
                        });
                    }
                    let remaining = deadline.duration_since(tokio::time::Instant::now());
                    tokio::time::sleep(backoff.min(remaining)).await;
                    backoff = backoff.saturating_mul(2).min(Duration::from_secs(1));
                }
            }
        };

        let (read_half, write_half) = tokio::io::split(stream);
        let mut client = Self {
            reader: BufReader::new(read_half),
            writer: write_half,
        };

        // Read the QMP greeting
        let greeting = client.read_response().await?;
        debug!(greeting = %greeting, "QMP greeting received");

        // Negotiate capabilities
        client.send_command("qmp_capabilities", None).await?;
        let resp = client.read_response().await?;
        if resp.get("error").is_some() {
            return Err(VmError::QmpCommandFailed {
                message: format!("qmp_capabilities failed: {resp}"),
            });
        }

        debug!(path = %socket_path.display(), "QMP connected and negotiated");
        Ok(client)
    }

    /// Send a QMP command and return the response.
    async fn send_command(&mut self, execute: &str, arguments: Option<Value>) -> Result<()> {
        let mut cmd = serde_json::json!({ "execute": execute });
        if let Some(args) = arguments {
            cmd.as_object_mut()
                .unwrap()
                .insert("arguments".into(), args);
        }
        let mut line = serde_json::to_string(&cmd).unwrap();
        line.push('\n');
        trace!(cmd = %line.trim(), "QMP send");
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| VmError::QmpCommandFailed {
                message: format!("write failed: {e}"),
            })?;
        self.writer
            .flush()
            .await
            .map_err(|e| VmError::QmpCommandFailed {
                message: format!("flush failed: {e}"),
            })?;
        Ok(())
    }

    /// Read the next JSON response (skipping asynchronous events).
    async fn read_response(&mut self) -> Result<Value> {
        loop {
            let mut line = String::new();
            let n =
                self.reader
                    .read_line(&mut line)
                    .await
                    .map_err(|e| VmError::QmpCommandFailed {
                        message: format!("read failed: {e}"),
                    })?;
            if n == 0 {
                return Err(VmError::QmpCommandFailed {
                    message: "QMP connection closed".into(),
                });
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            trace!(resp = %line, "QMP recv");
            let val: Value = serde_json::from_str(line).map_err(|e| VmError::QmpCommandFailed {
                message: format!("JSON parse failed: {e}: {line}"),
            })?;

            // Skip async events (they have an "event" key)
            if val.get("event").is_some() {
                debug!(event = %val, "QMP async event (skipped)");
                continue;
            }

            return Ok(val);
        }
    }

    /// Execute a QMP command and return the response.
    async fn execute(&mut self, command: &str, arguments: Option<Value>) -> Result<Value> {
        self.send_command(command, arguments).await?;
        self.read_response().await
    }

    /// Send an ACPI system_powerdown event (graceful shutdown).
    pub async fn system_powerdown(&mut self) -> Result<()> {
        let resp = self.execute("system_powerdown", None).await?;
        if resp.get("error").is_some() {
            return Err(VmError::QmpCommandFailed {
                message: format!("system_powerdown: {resp}"),
            });
        }
        info!("QMP: system_powerdown sent");
        Ok(())
    }

    /// Immediately terminate the QEMU process.
    pub async fn quit(&mut self) -> Result<()> {
        // quit disconnects before we can read a response, which is expected
        let _ = self.send_command("quit", None).await;
        info!("QMP: quit sent");
        Ok(())
    }

    /// Pause VM execution (freeze vCPUs).
    pub async fn stop(&mut self) -> Result<()> {
        let resp = self.execute("stop", None).await?;
        if resp.get("error").is_some() {
            return Err(VmError::QmpCommandFailed {
                message: format!("stop: {resp}"),
            });
        }
        info!("QMP: stop (pause) sent");
        Ok(())
    }

    /// Resume VM execution.
    pub async fn cont(&mut self) -> Result<()> {
        let resp = self.execute("cont", None).await?;
        if resp.get("error").is_some() {
            return Err(VmError::QmpCommandFailed {
                message: format!("cont: {resp}"),
            });
        }
        info!("QMP: cont (resume) sent");
        Ok(())
    }

    /// Query the current VM status. Returns the "status" string (e.g. "running", "paused").
    pub async fn query_status(&mut self) -> Result<String> {
        let resp = self.execute("query-status", None).await?;
        if let Some(err) = resp.get("error") {
            return Err(VmError::QmpCommandFailed {
                message: format!("query-status: {err}"),
            });
        }
        let status = resp
            .pointer("/return/status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        Ok(status)
    }
}
