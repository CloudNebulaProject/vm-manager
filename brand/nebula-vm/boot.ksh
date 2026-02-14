#!/bin/ksh
#
# nebula-vm brand: boot script
#
# Called by zoneadm(8) when the zone is booted.
# Arguments: %z = zone name, %R = zone root
#

ZONENAME="$1"
ZONEROOT="$2"

if [[ -z "$ZONENAME" || -z "$ZONEROOT" ]]; then
    echo "Usage: boot.ksh <zone-name> <zone-root>" >&2
    exit 1
fi

echo "nebula-vm: booting zone '${ZONENAME}'"

# Read zone network configuration and create VNIC if needed
VNIC_NAME="vnic_${ZONENAME}"
PHYSICAL_LINK=$(dladm show-phys -p -o LINK 2>/dev/null | head -1)

if [[ -n "$PHYSICAL_LINK" ]]; then
    # Check if VNIC already exists
    if ! dladm show-vnic "$VNIC_NAME" >/dev/null 2>&1; then
        echo "nebula-vm: creating VNIC ${VNIC_NAME} over ${PHYSICAL_LINK}"
        dladm create-vnic -l "$PHYSICAL_LINK" "$VNIC_NAME" || {
            echo "nebula-vm: WARNING - failed to create VNIC" >&2
        }
    fi
fi

# Start propolis-server inside the zone
PROPOLIS="${ZONEROOT}/root/opt/propolis/propolis-server"
PROPOLIS_CONFIG="${ZONEROOT}/root/opt/propolis/config.toml"
PIDFILE="${ZONEROOT}/root/var/run/propolis.pid"
LOGFILE="${ZONEROOT}/root/var/log/propolis.log"

if [[ -x "$PROPOLIS" ]]; then
    echo "nebula-vm: starting propolis-server"
    nohup zlogin "$ZONENAME" /opt/propolis/propolis-server \
        run /opt/propolis/config.toml \
        > "$LOGFILE" 2>&1 &
    echo $! > "$PIDFILE"
    echo "nebula-vm: propolis-server started (pid=$(cat $PIDFILE))"
else
    echo "nebula-vm: ERROR - propolis-server not found at ${PROPOLIS}" >&2
    exit 1
fi

echo "nebula-vm: zone '${ZONENAME}' booted"
exit 0
