# Network Block

The `network` node configures VM networking.

## Syntax

```kdl
network "mode"
// or
network "mode" {
    // mode-specific attributes
}
```

## Modes

### User (Default)

```kdl
network "user"
```

QEMU's SLIRP user-mode networking. No root required. SSH access is via a forwarded host port.

### TAP

```kdl
network "tap"
// or with explicit bridge:
network "tap" {
    bridge "br0"
}
```

TAP device attached to a Linux bridge. The guest appears on the bridge's network with a real IP.

**Default bridge:** `"br0"`

### VNIC (illumos only)

```kdl
network "vnic" name="vnic0"
```

illumos VNIC for exclusive-IP zone networking. Only available on the Propolis backend.

**Required attribute:** `name` — the VNIC device name.

### None

```kdl
network "none"
```

No networking.

## Default

If no `network` node is specified, user-mode networking is used.
