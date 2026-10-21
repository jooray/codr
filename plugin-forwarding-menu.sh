#!/bin/sh
set -eu
bridge="$HOME/.local/bin/herdr-white-noise-bridge"
while :; do
    printf '\033[2J\033[HWhite Noise forwarding\n\n'
    "$bridge" forwarding status
    printf '\n1) At desk: mute all sessions\n2) Away: forward all sessions\n3) Toggle\nq) Close\n\nA new message from your configured phone switches all sessions to Away.\nMuted history is not replayed. Press Enter to refresh.\n\nChoice: '
    IFS= read -r choice || exit 0
    case "$choice" in
        1|d|D) "$bridge" forwarding desk ;;
        2|a|A) "$bridge" forwarding away ;;
        3|t|T) "$bridge" forwarding toggle ;;
        q|Q) exit 0 ;;
    esac
done
