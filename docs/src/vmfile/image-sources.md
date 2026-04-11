# Image Sources

Every VM must specify exactly one image source. The two options are mutually exclusive.

## Local Image

```kdl
image "path/to/image.qcow2"
```

Points to a disk image on the host filesystem. The path is resolved relative to the VMFile directory, with tilde expansion.

The file must exist at parse time. Supported formats are auto-detected by `qemu-img` (qcow2, raw, etc.).

## Remote Image

```kdl
image-url "https://example.com/image.qcow2"
```

Downloads the image and caches it in `~/.local/share/vmctl/images/`. If the image is already cached, it won't be re-downloaded.

URLs ending in `.zst` or `.zstd` are automatically decompressed after download.

## OCI Registry Image

```kdl
image-url "oci://ghcr.io/myorg/my-vm-image:latest"
```

Pulls a QCOW2 disk image stored as an OCI artifact from a container registry. The `oci://` prefix tells vmctl to use the OCI distribution protocol instead of HTTP.

The OCI reference follows the standard format: `registry/repository:tag`.

### Authentication

| Registry | Method | Details |
|---|---|---|
| `ghcr.io` | `GITHUB_TOKEN` env var | Automatically used when set; token sent as basic auth |
| Other registries | Anonymous | No authentication by default |

To pull from a private GitHub Container Registry:

```bash
export GITHUB_TOKEN=ghp_xxxxxxxxxxxx
vmctl up
```

### OCI Artifact Format

vmctl expects the OCI artifact to contain a QCOW2 layer with one of these media types:

- `application/vnd.cloudnebula.qcow2.layer.v1`
- `application/octet-stream`

The first layer in the manifest is used as the disk image.

### Pushing Images to a Registry

You can push QCOW2 images using [ORAS](https://oras.land/) or any OCI-compatible tool:

```bash
# Push with the CloudNebula media type
oras push ghcr.io/myorg/my-vm-image:latest \
  --artifact-type application/vnd.cloudnebula.vm.v1 \
  my-image.qcow2:application/vnd.cloudnebula.qcow2.layer.v1

# Push as a generic OCI artifact
oras push ghcr.io/myorg/my-vm-image:latest \
  my-image.qcow2:application/octet-stream
```

### Caching

OCI images are cached alongside HTTP-downloaded images in `~/.local/share/vmctl/images/`. The cache file is named `<vm-name>.qcow2` (or a sanitized form of the reference if no VM name context). If the cached file exists, the pull is skipped.

## Validation

- Exactly one of `image` or `image-url` must be specified.
- `image-url` accepts both `https://` URLs and `oci://` references.
- Specifying both `image` and `image-url` is an error.
- Specifying neither is an error.
