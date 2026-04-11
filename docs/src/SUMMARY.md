# Summary

[Introduction](./introduction.md)

# Getting Started

- [Installation](./getting-started/installation.md)
- [Prerequisites](./getting-started/prerequisites.md)
- [Quick Start](./getting-started/quick-start.md)

# Concepts

- [How vmctl Works](./concepts/how-it-works.md)
- [Imperative vs Declarative](./concepts/imperative-vs-declarative.md)
- [VM Lifecycle](./concepts/vm-lifecycle.md)
- [Networking Modes](./concepts/networking.md)
- [Image Management](./concepts/image-management.md)
- [Cloud-Init and SSH Keys](./concepts/cloud-init-ssh.md)

# Tutorials

- [Creating a VM Imperatively](./tutorials/imperative-vm.md)
- [Declarative Workflow with VMFile.kdl](./tutorials/declarative-workflow.md)
- [Provisioning](./tutorials/provisioning.md)
- [Real-World: OmniOS Builder VM](./tutorials/omnios-builder.md)

# VMFile.kdl Reference

- [Overview](./vmfile/overview.md)
- [VM Block](./vmfile/vm-block.md)
- [Image Sources](./vmfile/image-sources.md)
- [Resources](./vmfile/resources.md)
- [Network Block](./vmfile/network.md)
- [Cloud-Init Block](./vmfile/cloud-init.md)
- [SSH Block](./vmfile/ssh.md)
- [Provision Blocks](./vmfile/provision.md)
- [Multi-VM Definitions](./vmfile/multi-vm.md)
- [Full Example](./vmfile/full-example.md)

# CLI Reference

- [vmctl](./cli/vmctl.md)
- [vmctl create](./cli/create.md)
- [vmctl start](./cli/start.md)
- [vmctl stop](./cli/stop.md)
- [vmctl destroy](./cli/destroy.md)
- [vmctl list](./cli/list.md)
- [vmctl status](./cli/status.md)
- [vmctl console](./cli/console.md)
- [vmctl ssh](./cli/ssh.md)
- [vmctl suspend](./cli/suspend.md)
- [vmctl resume](./cli/resume.md)
- [vmctl image](./cli/image.md)
- [vmctl up](./cli/up.md)
- [vmctl down](./cli/down.md)
- [vmctl reload](./cli/reload.md)
- [vmctl provision](./cli/provision.md)
- [vmctl log](./cli/log.md)

# Architecture

- [Overview](./architecture/overview.md)
- [Crate Structure](./architecture/crate-structure.md)
- [Hypervisor Backends](./architecture/backends.md)
- [State Management](./architecture/state-management.md)
- [SSH Subsystem](./architecture/ssh.md)
- [Error Handling](./architecture/error-handling.md)

# Library API Guide

- [Using vm-manager as a Crate](./library/using-as-crate.md)
- [Hypervisor Trait](./library/hypervisor-trait.md)
- [Core Types](./library/core-types.md)
- [Image Management API](./library/image-api.md)
- [SSH and Provisioning API](./library/ssh-provisioning-api.md)
- [VMFile Parsing API](./library/vmfile-api.md)

# Advanced Topics

- [Running in Docker/Podman](./advanced/containerization.md)
- [TAP Networking and Bridges](./advanced/tap-networking.md)
- [OCI Registries for VM Images](./advanced/oci-registries.md)
- [illumos / Propolis Backend](./advanced/propolis-illumos.md)
- [Custom Cloud-Init User Data](./advanced/custom-cloud-init.md)
- [Debugging and Logs](./advanced/debugging.md)
