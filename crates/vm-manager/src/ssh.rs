use std::io::Read;
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use ssh2::Session;
use tracing::warn;

use crate::error::{Result, VmError};
use crate::types::SshConfig;

/// Establish an SSH session to the given IP and port using the provided config.
///
/// Tries in-memory key first, then key file path.
pub fn connect(ip: &str, port: u16, config: &SshConfig) -> Result<Session> {
    let addr = format!("{ip}:{port}");
    let tcp = TcpStream::connect(&addr).map_err(|e| VmError::SshFailed {
        detail: format!("TCP connect to {addr}: {e}"),
    })?;

    let mut sess = Session::new().map_err(|e| VmError::SshFailed {
        detail: format!("session init: {e}"),
    })?;
    sess.set_tcp_stream(tcp);
    sess.handshake().map_err(|e| VmError::SshFailed {
        detail: format!("handshake with {addr}: {e}"),
    })?;

    // Authenticate: in-memory PEM → file path
    if let Some(ref pem) = config.private_key_pem {
        sess.userauth_pubkey_memory(&config.user, None, pem, None)
            .map_err(|e| VmError::SshFailed {
                detail: format!("pubkey auth (memory) as {}: {e}", config.user),
            })?;
    } else if let Some(ref key_path) = config.private_key_path {
        sess.userauth_pubkey_file(&config.user, None, key_path, None)
            .map_err(|e| VmError::SshFailed {
                detail: format!(
                    "pubkey auth (file {}) as {}: {e}",
                    key_path.display(),
                    config.user
                ),
            })?;
    } else {
        return Err(VmError::SshFailed {
            detail: "no SSH private key configured (neither in-memory PEM nor file path)".into(),
        });
    }

    if !sess.authenticated() {
        return Err(VmError::SshFailed {
            detail: "session not authenticated after auth attempt".into(),
        });
    }

    Ok(sess)
}

/// Execute a command over an existing SSH session.
///
/// Returns `(stdout, stderr, exit_code)`.
pub fn exec(sess: &Session, cmd: &str) -> Result<(String, String, i32)> {
    let mut channel = sess.channel_session().map_err(|e| VmError::SshFailed {
        detail: format!("channel session: {e}"),
    })?;

    channel.exec(cmd).map_err(|e| VmError::SshFailed {
        detail: format!("exec '{cmd}': {e}"),
    })?;

    let mut stdout = String::new();
    channel
        .read_to_string(&mut stdout)
        .map_err(|e| VmError::SshFailed {
            detail: format!("read stdout: {e}"),
        })?;

    let mut stderr = String::new();
    channel
        .stderr()
        .read_to_string(&mut stderr)
        .map_err(|e| VmError::SshFailed {
            detail: format!("read stderr: {e}"),
        })?;

    channel.wait_close().map_err(|e| VmError::SshFailed {
        detail: format!("wait close: {e}"),
    })?;
    let exit_code = channel.exit_status().unwrap_or(1);

    Ok((stdout, stderr, exit_code))
}

/// Execute a command and stream stdout/stderr to the provided writers as data arrives.
///
/// Returns `(stdout_collected, stderr_collected, exit_code)`.
pub fn exec_streaming<W1: std::io::Write, W2: std::io::Write>(
    sess: &Session,
    cmd: &str,
    mut out: W1,
    mut err: W2,
) -> Result<(String, String, i32)> {
    let mut channel = sess.channel_session().map_err(|e| VmError::SshFailed {
        detail: format!("channel session: {e}"),
    })?;

    // Non-blocking mode so we can interleave stdout and stderr reads
    sess.set_blocking(false);

    channel.exec(cmd).map_err(|e| {
        sess.set_blocking(true);
        VmError::SshFailed {
            detail: format!("exec '{cmd}': {e}"),
        }
    })?;

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let mut buf = [0u8; 8192];

    loop {
        let mut progress = false;

        // Read stdout
        match channel.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                let _ = out.write_all(&buf[..n]);
                let _ = out.flush();
                stdout_buf.extend_from_slice(&buf[..n]);
                progress = true;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                sess.set_blocking(true);
                return Err(VmError::SshFailed {
                    detail: format!("read stdout: {e}"),
                });
            }
        }

        // Read stderr
        match channel.stderr().read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                let _ = err.write_all(&buf[..n]);
                let _ = err.flush();
                stderr_buf.extend_from_slice(&buf[..n]);
                progress = true;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                sess.set_blocking(true);
                return Err(VmError::SshFailed {
                    detail: format!("read stderr: {e}"),
                });
            }
        }

        if channel.eof() && !progress {
            break;
        }

        if !progress {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    sess.set_blocking(true);

    channel.wait_close().map_err(|e| VmError::SshFailed {
        detail: format!("wait close: {e}"),
    })?;
    let exit_code = channel.exit_status().unwrap_or(1);

    let stdout = String::from_utf8_lossy(&stdout_buf).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();

    Ok((stdout, stderr, exit_code))
}

/// Upload a local file to a remote path via SFTP.
pub fn upload(sess: &Session, local: &Path, remote: &Path) -> Result<()> {
    let sftp = sess.sftp().map_err(|e| VmError::SshFailed {
        detail: format!("SFTP init: {e}"),
    })?;

    let mut local_file = std::fs::File::open(local).map_err(|e| VmError::SshFailed {
        detail: format!("open local file {}: {e}", local.display()),
    })?;

    let mut buf = Vec::new();
    local_file
        .read_to_end(&mut buf)
        .map_err(|e| VmError::SshFailed {
            detail: format!("read local file: {e}"),
        })?;

    let mut remote_file = sftp.create(remote).map_err(|e| VmError::SshFailed {
        detail: format!("SFTP create {}: {e}", remote.display()),
    })?;

    std::io::Write::write_all(&mut remote_file, &buf).map_err(|e| VmError::SshFailed {
        detail: format!("SFTP write: {e}"),
    })?;

    Ok(())
}

/// Connect with exponential backoff retry.
///
/// Retries the connection until `timeout` elapses, with exponential backoff capped at 5 seconds.
pub async fn connect_with_retry(
    ip: &str,
    port: u16,
    config: &SshConfig,
    timeout: Duration,
) -> Result<Session> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut backoff = Duration::from_secs(1);
    let mut attempt: u32 = 0;

    loop {
        attempt += 1;
        let ip_owned = ip.to_string();
        let config_clone = config.clone();

        // Run the blocking SSH connect on a blocking thread
        let result =
            tokio::task::spawn_blocking(move || connect(&ip_owned, port, &config_clone)).await;

        match result {
            Ok(Ok(sess)) => return Ok(sess),
            Ok(Err(e)) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(e);
                }
                warn!(
                    attempt,
                    ip = %ip,
                    error = %e,
                    "SSH connect failed; retrying"
                );
            }
            Err(join_err) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(VmError::SshFailed {
                        detail: format!("spawn_blocking join error: {join_err}"),
                    });
                }
            }
        }

        let remaining = deadline.duration_since(tokio::time::Instant::now());
        let sleep_dur = backoff.min(remaining);
        tokio::time::sleep(sleep_dur).await;
        backoff = backoff.saturating_mul(2).min(Duration::from_secs(5));
    }
}
