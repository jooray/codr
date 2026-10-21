#!/bin/sh
set -eu
exec "${HERDR_BIN_PATH:-herdr}" plugin pane open --plugin local.white-noise --entrypoint setup --focus
