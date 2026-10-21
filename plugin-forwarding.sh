#!/bin/sh
set -eu
if [ "${1:-}" = toggle ]; then
    exec "$HOME/.local/bin/herdr-white-noise-bridge" forwarding toggle
fi
exec "${HERDR_BIN_PATH:-herdr}" plugin pane open --plugin local.white-noise --entrypoint forwarding --focus
