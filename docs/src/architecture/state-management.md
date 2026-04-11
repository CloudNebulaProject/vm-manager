# State Management

## VM Store

vmctl persists VM state in a JSON file at `$XDG_DATA_HOME/vmctl/vms.json` (typically `~/.local/share/vmctl/vms.json`). If `XDG_DATA_HOME` is not set, the standard XDG default of `~/.local/share` is used. Falls back to `/tmp` only if the home directory cannot be determined.

The store is a simple mapping from VM name to `VmHandle`.

## VmHandle Serialization

`VmHandle` is serialized to JSON with all fields. Fields added in later versions have `#[serde(default)]` annotations, so older JSON files are deserialized without errors (missing fields get defaults).

Example stored handle:

```json
{
  "id": "abc123",
  "name": "myvm",
  "backend": "qemu",
  "work_dir": "/home/user/.local/share/vmctl/vms/myvm",
  "overlay_path": "/home/user/.local/share/vmctl/vms/myvm/overlay.qcow2",
  "seed_iso_path": "/home/user/.local/share/vmctl/vms/myvm/seed.iso",
  "pid": 12345,
  "qmp_socket": "/home/user/.local/share/vmctl/vms/myvm/qmp.sock",
  "console_socket": "/home/user/.local/share/vmctl/vms/myvm/console.sock",
  "vnc_addr": "127.0.0.1:5900",
  "vcpus": 2,
  "memory_mb": 2048,
  "disk_gb": 20,
  "network": {"type": "User"},
  "ssh_host_port": 10042,
  "mac_addr": "52:54:00:ab:cd:ef"
}
```

## Write Safety

The store uses an atomic write pattern:
1. Write to a `.tmp` file.
2. Rename (atomic on most filesystems) to the final path.

This prevents corruption if the process is interrupted during a write.

## State vs Process State

The store records the *last known* state but doesn't actively monitor QEMU processes. When vmctl queries a VM's state, it:

1. Checks if the PID file exists.
2. Sends `kill(pid, 0)` to verify the process is alive.
3. If alive, queries QMP for detailed status (`running`, `paused`, etc.).
4. If dead, reports `Stopped`.
