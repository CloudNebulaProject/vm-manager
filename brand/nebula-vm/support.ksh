#!/bin/ksh
#
# nebula-vm brand: support script
#
# Called for pre/post state change hooks.
# Arguments: prestate|poststate <zone-name> <zone-root>
#

ACTION="$1"
ZONENAME="$2"
ZONEROOT="$3"

case "$ACTION" in
    prestate)
        # Pre-state-change hook: ensure network resources are available
        VNIC_NAME="vnic_${ZONENAME}"
        PHYSICAL_LINK=$(dladm show-phys -p -o LINK 2>/dev/null | head -1)

        if [[ -n "$PHYSICAL_LINK" ]]; then
            if ! dladm show-vnic "$VNIC_NAME" >/dev/null 2>&1; then
                dladm create-vnic -l "$PHYSICAL_LINK" "$VNIC_NAME" 2>/dev/null || true
            fi
        fi
        ;;

    poststate)
        # Post-state-change hook: cleanup if zone is no longer running
        VNIC_NAME="vnic_${ZONENAME}"
        ZONE_STATE=$(zoneadm -z "$ZONENAME" list -p 2>/dev/null | cut -d: -f3)

        if [[ "$ZONE_STATE" != "running" ]]; then
            if dladm show-vnic "$VNIC_NAME" >/dev/null 2>&1; then
                dladm delete-vnic "$VNIC_NAME" 2>/dev/null || true
            fi
        fi
        ;;

    *)
        echo "nebula-vm support: unknown action '${ACTION}'" >&2
        exit 1
        ;;
esac

exit 0
