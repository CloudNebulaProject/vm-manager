#!/bin/ksh
#
# nebula-vm brand: uninstall script
#
# Called by zoneadm(8) during zone uninstallation.
# Arguments: %z = zone name, %R = zone root
#

ZONENAME="$1"
ZONEROOT="$2"

if [[ -z "$ZONENAME" || -z "$ZONEROOT" ]]; then
    echo "Usage: uninstall.ksh <zone-name> <zone-root>" >&2
    exit 1
fi

echo "nebula-vm: uninstalling zone '${ZONENAME}'"

# Remove the zone root contents
if [[ -d "${ZONEROOT}/root" ]]; then
    rm -rf "${ZONEROOT}/root"
    echo "nebula-vm: zone root removed"
fi

# Remove the zone path itself if empty
rmdir "${ZONEROOT}" 2>/dev/null || true

echo "nebula-vm: zone '${ZONENAME}' uninstalled"
exit 0
