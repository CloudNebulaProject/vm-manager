# VMFile.kdl Overview

`VMFile.kdl` is the declarative configuration format for vmctl. It uses [KDL](https://kdl.dev) (KDL Document Language), a human-friendly configuration language.

## Discovery

vmctl looks for `VMFile.kdl` in the current directory by default. You can override this with `--file`:

```bash
vmctl up --file path/to/MyVMFile.kdl
```

## Basic Structure

A VMFile contains one or more `vm` blocks, each defining a virtual machine:

```kdl
vm "name" {
    // image source (required)
    // resources
    // networking
    // cloud-init
    // ssh config
    // provisioners
}
```

## Path Resolution

All paths in a VMFile are resolved relative to the directory containing the VMFile. Tilde (`~`) is expanded to the user's home directory.

```kdl
// Relative to VMFile directory
image "images/ubuntu.qcow2"

// Absolute path
image "/opt/images/ubuntu.qcow2"

// Home directory expansion
cloud-init {
    ssh-key "~/.ssh/id_ed25519.pub"
}
```

## Validation

vmctl validates the VMFile on parse and provides detailed error messages with hints:

- VM names must be unique.
- Each VM must have exactly one image source (`image` or `image-url`, not both).
- Shell provisioners must have exactly one of `inline` or `script`.
- File provisioners must have both `source` and `destination`.
- Network type must be `"user"`, `"tap"`, `"vnic"`, or `"none"`.
