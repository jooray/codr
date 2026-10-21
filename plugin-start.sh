#!/bin/sh
set -eu
XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}
export XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS
# A dedicated user service supervises the plugin worker across HERDR client disconnects.
systemctl --user start herdr-white-noise-bridge.service
if [ "${1:-}" = --show-menu ]; then
    if ! "${HERDR_BIN_PATH:-herdr}" plugin pane open --plugin local.white-noise --entrypoint forwarding --focus; then
        printf '%s\n' 'White Noise forwarding menu could not open. Use the forwarding action or its configured shortcut.' >&2
    fi
fi
