#!/bin/ksh
#
# nebula-vm brand: halt script
#
# Called by zoneadm(8) when the zone is being halted.
# Arguments: %z = zone name, %R = zone root
#

ZONENAME="$1"
ZONEROOT="$2"

if [[ -z "$ZONENAME" || -z "$ZONEROOT" ]]; then
    echo "Usage: halt.ksh <zone-name> <zone-root>" >&2
    exit 1
fi

echo "nebula-vm: halting zone '${ZONENAME}'"

# Stop propolis-server
PIDFILE="${ZONEROOT}/root/var/run/propolis.pid"
if [[ -f "$PIDFILE" ]]; then
    PID=$(cat "$PIDFILE")
    if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
        echo "nebula-vm: stopping propolis-server (pid=${PID})"
        kill -TERM "$PID"
        # Wait up to 10 seconds for graceful shutdown
        WAIT=0
        while kill -0 "$PID" 2>/dev/null && [[ $WAIT -lt 10 ]]; do
            sleep 1
            WAIT=$((WAIT + 1))
        done
        # Force kill if still running
        if kill -0 "$PID" 2>/dev/null; then
            echo "nebula-vm: force-killing propolis-server"
            kill -KILL "$PID"
        fi
    fi
    rm -f "$PIDFILE"
fi

# Clean up VNIC
VNIC_NAME="vnic_${ZONENAME}"
if dladm show-vnic "$VNIC_NAME" >/dev/null 2>&1; then
    echo "nebula-vm: removing VNIC ${VNIC_NAME}"
    dladm delete-vnic "$VNIC_NAME" || true
fi

echo "nebula-vm: zone '${ZONENAME}' halted"
exit 0
