#!/bin/sh
set -eu
XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}
export XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS
# A dedicated user service supervises the plugin worker across HERDR client disconnects.
systemctl --user start herdr-white-noise-bridge.service
