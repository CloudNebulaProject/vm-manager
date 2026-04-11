# VM Lifecycle

Every VM in vmctl moves through a set of well-defined states.

## States

| State | Description |
|---|---|
| `Preparing` | Backend is allocating resources (overlay, ISO, sockets) |
| `Prepared` | Resources allocated, ready to boot |
| `Running` | VM is booted and executing |
| `Suspended` | VM vCPUs are paused (memory preserved, not executing) |
| `Stopped` | VM has been shut down (gracefully or forcibly) |
| `Failed` | An error occurred during a lifecycle operation |
| `Destroyed` | VM and all its resources have been cleaned up |

## Transitions

```text
          prepare()         start()
[new] ──────────> Prepared ──────────> Running
                                        │  │
                            suspend()   │  │  stop(timeout)
                           ┌────────────┘  └──────────────┐
                           v                               v
                        Suspended ─── resume() ──>     Stopped
                                                        │
                                           start()      │
                                     Running <──────────┘

Any state ── destroy() ──> Destroyed
```

## Commands and Transitions

| Command | From State | To State |
|---|---|---|
| `vmctl create` | (none) | Prepared |
| `vmctl start` | Prepared, Stopped | Running |
| `vmctl stop` | Running | Stopped |
| `vmctl suspend` | Running | Suspended (paused vCPUs) |
| `vmctl resume` | Suspended | Running |
| `vmctl destroy` | Any | Destroyed |
| `vmctl up` | (none), Stopped | Running (auto-creates if needed) |
| `vmctl down` | Running | Stopped |
| `vmctl reload` | Any | Running (destroys + recreates) |

## Graceful Shutdown

`vmctl stop` sends an ACPI power-down signal via QMP. If the guest doesn't shut down within the timeout (default 30 seconds), vmctl sends SIGTERM, and finally SIGKILL as a last resort.
