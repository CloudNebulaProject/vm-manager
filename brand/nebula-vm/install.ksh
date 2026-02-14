#!/bin/ksh
#
# nebula-vm brand: install script
#
# Called by zoneadm(8) during zone installation.
# Arguments: %z = zone name, %R = zone root
#

ZONENAME="$1"
ZONEROOT="$2"

if [[ -z "$ZONENAME" || -z "$ZONEROOT" ]]; then
    echo "Usage: install.ksh <zone-name> <zone-root>" >&2
    exit 1
fi

echo "nebula-vm: installing zone '${ZONENAME}' at ${ZONEROOT}"

# Create the minimal zone root structure
mkdir -p "${ZONEROOT}/root"
mkdir -p "${ZONEROOT}/root/dev"
mkdir -p "${ZONEROOT}/root/etc"
mkdir -p "${ZONEROOT}/root/var/run"
mkdir -p "${ZONEROOT}/root/var/log"
mkdir -p "${ZONEROOT}/root/opt/propolis"

# Copy propolis-server binary into the zone if available on the host
PROPOLIS_BIN="/opt/oxide/propolis-server/bin/propolis-server"
if [[ -f "$PROPOLIS_BIN" ]]; then
    cp "$PROPOLIS_BIN" "${ZONEROOT}/root/opt/propolis/propolis-server"
    chmod 0755 "${ZONEROOT}/root/opt/propolis/propolis-server"
    echo "nebula-vm: propolis-server copied to zone"
else
    echo "nebula-vm: WARNING - propolis-server not found at ${PROPOLIS_BIN}"
    echo "nebula-vm: you must manually place propolis-server in the zone"
fi

# Write a default propolis configuration
cat > "${ZONEROOT}/root/opt/propolis/config.toml" <<'EOF'
[main]
listen_addr = "0.0.0.0"
listen_port = 12400

[log]
level = "info"
EOF

echo "nebula-vm: zone '${ZONENAME}' installed successfully"
exit 0
