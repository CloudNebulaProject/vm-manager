use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use ssh2::Session;
use tracing::info;

use crate::error::{Result, VmError};
use crate::ssh;
use crate::vmfile::{FileProvision, ProvisionDef, ShellProvision, resolve_path};

/// Run all provision steps on an established SSH session.
///
/// If `log_dir` is provided, all stdout/stderr from provision steps is appended to
/// `provision.log` in that directory.
pub fn run_provisions(
    sess: &Session,
    provisions: &[ProvisionDef],
    base_dir: &Path,
    vm_name: &str,
    log_dir: Option<&Path>,
) -> Result<()> {
    for (i, prov) in provisions.iter().enumerate() {
        let step = i + 1;
        match prov {
            ProvisionDef::Shell(shell) => {
                run_shell(sess, shell, base_dir, vm_name, step, log_dir)?;
            }
            ProvisionDef::File(file) => {
                run_file(sess, file, base_dir, vm_name, step, log_dir)?;
            }
        }
    }
    Ok(())
}

/// Log provision output to tracing and optionally to a file.
fn log_output(vm_name: &str, step: usize, label: &str, stdout: &str, stderr: &str) {
    for line in stdout.lines() {
        info!(vm = %vm_name, step, "[{label}:stdout] {line}");
    }
    for line in stderr.lines() {
        info!(vm = %vm_name, step, "[{label}:stderr] {line}");
    }
}

/// Append provision output to a log file in the given directory.
pub fn append_provision_log(log_dir: &Path, step: usize, label: &str, stdout: &str, stderr: &str) {
    let log_path = log_dir.join("provision.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = writeln!(f, "=== Step {step}: {label} ===");
        if !stdout.is_empty() {
            let _ = writeln!(f, "--- stdout ---");
            let _ = write!(f, "{stdout}");
            if !stdout.ends_with('\n') {
                let _ = writeln!(f);
            }
        }
        if !stderr.is_empty() {
            let _ = writeln!(f, "--- stderr ---");
            let _ = write!(f, "{stderr}");
            if !stderr.ends_with('\n') {
                let _ = writeln!(f);
            }
        }
        let _ = writeln!(f);
    }
}

fn run_shell(
    sess: &Session,
    shell: &ShellProvision,
    base_dir: &Path,
    vm_name: &str,
    step: usize,
    log_dir: Option<&Path>,
) -> Result<()> {
    if let Some(ref cmd) = shell.inline {
        info!(vm = %vm_name, step, cmd = %cmd, "running inline shell provision");
        let (stdout, stderr, exit_code) =
            ssh::exec(sess, cmd).map_err(|e| VmError::ProvisionFailed {
                vm: vm_name.into(),
                step,
                detail: format!("shell exec: {e}"),
            })?;

        log_output(vm_name, step, cmd, &stdout, &stderr);
        if let Some(dir) = log_dir {
            append_provision_log(dir, step, cmd, &stdout, &stderr);
        }

        if exit_code != 0 {
            return Err(VmError::ProvisionFailed {
                vm: vm_name.into(),
                step,
                detail: format!(
                    "inline command exited with code {exit_code}\nstdout: {stdout}\nstderr: {stderr}"
                ),
            });
        }
        info!(vm = %vm_name, step, "inline shell provision completed");
    } else if let Some(ref script_raw) = shell.script {
        let local_path = resolve_path(script_raw, base_dir);
        info!(vm = %vm_name, step, script = %local_path.display(), "running script provision");

        let remote_path_str = format!("/tmp/vmctl-provision-{step}.sh");
        let remote_path = Path::new(&remote_path_str);

        // Upload the script
        ssh::upload(sess, &local_path, remote_path).map_err(|e| VmError::ProvisionFailed {
            vm: vm_name.into(),
            step,
            detail: format!("upload script: {e}"),
        })?;

        // Make executable and run
        let run_cmd = format!("chmod +x {remote_path_str} && {remote_path_str}");
        let (stdout, stderr, exit_code) =
            ssh::exec(sess, &run_cmd).map_err(|e| VmError::ProvisionFailed {
                vm: vm_name.into(),
                step,
                detail: format!("script exec: {e}"),
            })?;

        log_output(vm_name, step, script_raw, &stdout, &stderr);
        if let Some(dir) = log_dir {
            append_provision_log(dir, step, script_raw, &stdout, &stderr);
        }

        if exit_code != 0 {
            return Err(VmError::ProvisionFailed {
                vm: vm_name.into(),
                step,
                detail: format!(
                    "script exited with code {exit_code}\nstdout: {stdout}\nstderr: {stderr}"
                ),
            });
        }
        info!(vm = %vm_name, step, "script provision completed");
    }
    Ok(())
}

fn run_file(
    sess: &Session,
    file: &FileProvision,
    base_dir: &Path,
    vm_name: &str,
    step: usize,
    log_dir: Option<&Path>,
) -> Result<()> {
    let local_path = resolve_path(&file.source, base_dir);
    let remote_path = Path::new(&file.destination);

    info!(
        vm = %vm_name,
        step,
        source = %local_path.display(),
        destination = %file.destination,
        "running file provision"
    );

    ssh::upload(sess, &local_path, remote_path).map_err(|e| VmError::ProvisionFailed {
        vm: vm_name.into(),
        step,
        detail: format!("file upload: {e}"),
    })?;

    let msg = format!("{} -> {}", local_path.display(), file.destination);
    if let Some(dir) = log_dir {
        append_provision_log(dir, step, "file-upload", &msg, "");
    }

    info!(vm = %vm_name, step, "file provision completed");
    Ok(())
}
