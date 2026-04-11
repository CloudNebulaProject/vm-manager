# OCI Registries for VM Images

vmctl can pull QCOW2 disk images stored as OCI artifacts from any OCI-compatible container registry. This lets you version, distribute, and cache VM base images using the same infrastructure as container images.

## How It Works

When you use an `oci://` prefixed URL in your VMFile:

```kdl
image-url "oci://ghcr.io/myorg/ubuntu-dev:22.04"
```

vmctl uses the OCI distribution protocol to:

1. Resolve the tag to a manifest.
2. Download the first layer (the QCOW2 image).
3. Cache it locally in `~/.local/share/vmctl/images/`.
4. Create a QCOW2 overlay on top for the VM.

Subsequent runs skip the download if the image is already cached.

## Setting Up GitHub Container Registry (ghcr.io)

### 1. Create a Personal Access Token

You need a token with `read:packages` (and `write:packages` if pushing).

1. Go to **GitHub Settings > Developer settings > Personal access tokens > Tokens (classic)**.
2. Create a token with the `read:packages` and `write:packages` scopes.
3. Export it:

```bash
export GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

vmctl automatically uses `GITHUB_TOKEN` when pulling from `ghcr.io`.

### 2. Push a QCOW2 Image

Use [ORAS](https://oras.land/) to push QCOW2 images as OCI artifacts:

```bash
# Install ORAS
# See https://oras.land/docs/installation for your platform

# Log in to ghcr.io
echo $GITHUB_TOKEN | oras login ghcr.io -u USERNAME --password-stdin

# Push with the CloudNebula media type (recommended)
oras push ghcr.io/myorg/ubuntu-dev:22.04 \
  --artifact-type application/vnd.cloudnebula.vm.v1 \
  ubuntu-22.04-cloudimg-amd64.qcow2:application/vnd.cloudnebula.qcow2.layer.v1

# Or push as a generic octet-stream
oras push ghcr.io/myorg/ubuntu-dev:22.04 \
  ubuntu-22.04-cloudimg-amd64.qcow2:application/octet-stream
```

### 3. Use in a VMFile

```kdl
vm "dev" {
    image-url "oci://ghcr.io/myorg/ubuntu-dev:22.04"
    vcpus 2
    memory 2048

    cloud-init {
        hostname "dev"
    }

    ssh {
        user "ubuntu"
    }
}
```

### 4. Package Visibility

By default, GitHub packages are private. To allow anonymous pulls (no `GITHUB_TOKEN` needed), change the package visibility to **public** in the package settings on GitHub.

## Setting Up a Self-Hosted Registry

Any OCI-compatible registry works. Common options:

### Distribution (reference implementation)

```bash
docker run -d -p 5000:5000 --name registry registry:2
```

Push and pull:

```bash
oras push localhost:5000/my-vm-image:latest \
  --artifact-type application/vnd.cloudnebula.vm.v1 \
  my-image.qcow2:application/vnd.cloudnebula.qcow2.layer.v1
```

```kdl
image-url "oci://localhost:5000/my-vm-image:latest"
```

> **Note:** vmctl uses HTTPS by default. For a local registry without TLS, you may need to configure your registry with TLS or use a reverse proxy.

### Harbor, Zot, and others

Any registry implementing the [OCI Distribution Spec](https://github.com/opencontainers/distribution-spec) will work. Push images with ORAS using the same media types.

## OCI Artifact Format

vmctl expects a specific artifact structure:

| Component | Details |
|---|---|
| **Layer content** | A QCOW2 disk image |
| **Layer media type** | `application/vnd.cloudnebula.qcow2.layer.v1` or `application/octet-stream` |
| **Artifact type** | Any (recommended: `application/vnd.cloudnebula.vm.v1`) |

Only the first layer in the manifest is used. Multi-layer artifacts are not supported — only the first layer is extracted.

## Authentication Summary

| Registry | Environment Variable | Auth Method |
|---|---|---|
| `ghcr.io` | `GITHUB_TOKEN` | Basic auth (`_token:<token>`) |
| All others | — | Anonymous |

For registries that require authentication but aren't `ghcr.io`, the current implementation falls back to anonymous access. Support for additional auth methods (Docker config, registry-specific tokens) may be added in the future.

## Caching Behavior

- Cached images are stored in `~/.local/share/vmctl/images/`.
- When used from a VMFile, the cache file is named `<vm-name>.qcow2`.
- When pulled via `vmctl image pull`, the name is derived from the OCI reference (slashes and colons replaced with underscores).
- If the cached file already exists, the pull is skipped entirely — there is no digest-based freshness check.
- To force a re-pull, delete the cached file or use `vmctl reload` after removing it.

## Workflow Example

A complete workflow for building and distributing a custom VM image:

```bash
# 1. Start from a cloud image
wget https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img

# 2. (Optional) Customize it — boot, install packages, shut down, etc.

# 3. Push to your registry
oras push ghcr.io/myorg/ubuntu-noble-dev:latest \
  --artifact-type application/vnd.cloudnebula.vm.v1 \
  noble-server-cloudimg-amd64.img:application/vnd.cloudnebula.qcow2.layer.v1

# 4. Use it in VMFiles across your team
cat > VMFile.kdl <<'EOF'
vm "builder" {
    image-url "oci://ghcr.io/myorg/ubuntu-noble-dev:latest"
    vcpus 4
    memory 4096
    disk 40

    cloud-init {
        hostname "builder"
    }

    ssh {
        user "ubuntu"
    }

    provision "shell" {
        inline "sudo apt-get update && sudo apt-get install -y build-essential"
    }
}
EOF

# 5. Bring it up
export GITHUB_TOKEN=ghp_xxxx
vmctl up
```
