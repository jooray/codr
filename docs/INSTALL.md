# Install codr

codr is a HERDR plugin backed by two systemd user services: the Rust bridge and its MDK transport.

This guide covers Linux. It does not install HERDR or configure an OpenCode model provider.

## One-prompt installation

Give your coding agent this prompt, with your own public key:

> Install https://github.com/tidley/codr on this machine. My White Noise public key is npub1…. Follow the agent installation guide and report what you verified.

The [agent section](#instructions-for-coding-agents) explains how to carry out that request. The remaining commands are the shared reference for agents and humans.

## Requirements

- Linux x86_64 or aarch64 for the MDK binaries below.
- A working systemd user manager under the account that runs HERDR.
- HERDR 0.9.1 or newer, with a running local server.
- A working, authenticated OpenCode installation that supports `--pure export SESSION_ID`.
- Rust and Cargo for the source build, plus Git, Python 3, curl, and tar.
- White Noise on your phone, with your public `npub` available.
- Access to the configured White Noise relays.

Use a current stable Rust toolchain if Cargo reports an unsupported dependency or language feature.

The phone account must publish a compatible Marmot KeyPackage before the bridge can invite it. Open White Noise and let it connect to its relays.

## Manual installation

Run the commands as your normal user, not root. Use Bash, and keep the same shell for the variables below.

### 1. Identify the account and environment

```bash
set -euo pipefail
umask 077
export PATH="$HOME/.local/bin:$HOME/.opencode/bin:$PATH"

# Replace this with YOUR White Noise public key. Never enter an nsec/private key.
export OWNER_NPUB="npub1REPLACE_WITH_YOUR_WHITE_NOISE_PUBLIC_KEY"

export CONFIG_ROOT="${XDG_CONFIG_HOME:-$HOME/.config}"
export CODR_CONFIG="$CONFIG_ROOT/herdr-white-noise-bridge/config.toml"
export HERDR_SOCKET="${HERDR_SOCKET_PATH:-$HOME/.config/herdr/herdr.sock}"
export OPENCODE_BIN="$(command -v opencode)"

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"

command -v herdr opencode cargo git python3 systemctl curl tar
herdr --version
opencode --version
systemctl --user show-environment >/dev/null
```

If you use a named HERDR server, set `HERDR_SOCKET` to that server's actual socket. Then select and verify that same server for all subsequent HERDR commands:

```bash
export HERDR_SOCKET_PATH="$HERDR_SOCKET"
test -S "$HERDR_SOCKET"
herdr api snapshot
```

One codr configuration targets one HERDR server.

Confirm that `OWNER_NPUB` contains your real public key before continuing. The placeholder is deliberately invalid.

If the configuration, either codr service unit, or `$HOME/.marmot-agents/herdr` already exists, use [Existing installations](#existing-installations) first.

### 2. Get and build codr

Choose a permanent checkout location. HERDR will retain a link to that directory.

```bash
git clone https://github.com/tidley/codr.git
cd codr
cargo test --locked --tests
cargo install --locked --path . --root "$HOME/.local"
"$HOME/.local/bin/herdr-white-noise-bridge" --version
```

If you already have a checkout, use it instead of cloning over it. Inspect local changes before updating it.

`cargo install` places the executable in `~/.local/bin` without assuming where Cargo stores intermediate build outputs.

Validate the complete owner key before creating an MDK account or starting its services. This uses codr's own key and configuration validation without connecting to either socket:

```bash
python3 <<'PY'
import json
import os
from pathlib import Path
import subprocess
import tempfile

with tempfile.TemporaryDirectory(prefix="codr-preflight-") as temporary:
    directory = Path(temporary)
    values = {
        "phone_npub": os.environ["OWNER_NPUB"],
        "herdr_socket": str(directory / "herdr.sock"),
        "wn_agent_socket": str(directory / "wn-agent.sock"),
        "account_id_hex": "00" * 32,
        "state_path": str(directory / "state.json"),
    }
    config = directory / "config.toml"
    config.write_text("".join(f"{key} = {json.dumps(value)}\n" for key, value in values.items()))
    subprocess.run([str(Path.home() / ".local/bin/herdr-white-noise-bridge"), "--config", str(config), "status"], check=True)
print("Owner public key validated")
PY
```

### 3. Install the MDK transport

codr uses **`wn-agent` 0.13.0**. It does not need `wn-opencode` or another terminal harness.

If `~/.local/bin/wn-agent --version` already reports `wn-agent 0.13.0`, reuse that binary and skip the download. The new bridge still gets a separate account home and service.

If another version is installed, inspect its existing users before replacing it. Do not overwrite a shared executable without checking compatibility.

For a fresh installation, download the binary and its checksum from the same pinned release:

```bash
(
    set -euo pipefail
    case "$(uname -m)" in
        x86_64) platform=linux-x86_64 ;;
        aarch64|arm64) platform=linux-aarch64 ;;
        *) printf 'Unsupported architecture\n' >&2; exit 1 ;;
    esac
    test ! -e "$HOME/.local/bin/wn-agent"
    work_dir="$(mktemp -d)"
    trap 'rm -rf "$work_dir"' EXIT
    asset="wn-agent-$platform-0.13.0.tar.gz"
    base="https://github.com/marmot-protocol/mdk/releases/download/wn-agent-v0.13.0"
    curl -fsSL --proto '=https' --proto-redir '=https' "$base/$asset" -o "$work_dir/$asset"
    curl -fsSL --proto '=https' --proto-redir '=https' "$base/$asset.sha256" -o "$work_dir/$asset.sha256"

    python3 - "$work_dir/$asset" "$work_dir/$asset.sha256" <<'PY'
import hashlib
import pathlib
import sys

archive, checksum = map(pathlib.Path, sys.argv[1:])
expected = checksum.read_text().split()[0].lower()
digest = hashlib.sha256()
with archive.open("rb") as source:
    for chunk in iter(lambda: source.read(1024 * 1024), b""):
        digest.update(chunk)
if len(expected) != 64 or digest.hexdigest() != expected:
    raise SystemExit("MDK checksum verification failed")
print("MDK checksum verified")
PY

    tar -xzf "$work_dir/$asset" -C "$work_dir" --no-same-owner "wn-agent-$platform/wn-agent"
    install -d -m 0755 "$HOME/.local/bin"
    install -m 0755 "$work_dir/wn-agent-$platform/wn-agent" "$HOME/.local/bin/wn-agent"
)
"$HOME/.local/bin/wn-agent" --version
```

The archive names and layout above match MDK's `wn-agent-v0.13.0` release. Keep the release and checksum together when selecting a different version.

### 4. Install the service units and start MDK

Run this from the codr checkout. These commands are for a fresh installation and refuse to replace existing units.

```bash
install -d -m 0700 "$CONFIG_ROOT/systemd/user"
test ! -e "$CONFIG_ROOT/systemd/user/wn-agent-herdr.service"
test ! -e "$CONFIG_ROOT/systemd/user/herdr-white-noise-bridge.service"
install -m 0644 systemd/wn-agent-herdr.service "$CONFIG_ROOT/systemd/user/"
install -m 0644 systemd/herdr-white-noise-bridge.service "$CONFIG_ROOT/systemd/user/"
install -d -m 0700 "$HOME/.marmot-agents/herdr"
systemctl --user daemon-reload
systemctl --user start wn-agent-herdr.service

"$HOME/.local/bin/wn-agent" bootstrap \
    --home "$HOME/.marmot-agents/herdr" \
    --socket "$HOME/.marmot-agents/herdr/dev/wn-agent.sock" \
    --label herdr-bridge \
    --allow-welcomer "$OWNER_NPUB" \
    --no-quic --json \
    > "$HOME/.marmot-agents/herdr/codr-bootstrap.json"
```

`bootstrap` waits for the socket. If it fails, inspect the transport log before repeating account creation:

```bash
systemctl --user status wn-agent-herdr.service
journalctl --user -u wn-agent-herdr.service --no-pager -n 50
```

### 5. Generate the configuration

The following command uses your public key, actual paths, and the account ID returned by MDK. It refuses to replace an existing configuration.

It also sets an explicit configuration path in the worker unit. This keeps the service correct when `XDG_CONFIG_HOME` differs from the default.

```bash
python3 <<'PY'
import json
import os
from pathlib import Path

home = Path.home()
owner = os.environ["OWNER_NPUB"]
if not owner.startswith("npub1") or "REPLACE" in owner or "…" in owner:
    raise SystemExit("Set OWNER_NPUB to your own complete White Noise public key")
config = Path(os.environ["CODR_CONFIG"]).resolve()
if config.exists():
    raise SystemExit("Configuration already exists; follow Existing installations")
bootstrap = json.loads((home / ".marmot-agents/herdr/codr-bootstrap.json").read_text())
values = {
    "phone_npub": owner,
    "herdr_socket": str(Path(os.environ["HERDR_SOCKET"]).resolve()),
    "wn_agent_socket": str(home / ".marmot-agents/herdr/dev/wn-agent.sock"),
    "account_id_hex": bootstrap["account_id_hex"],
    "state_path": str(Path(os.environ.get("XDG_STATE_HOME", str(home / ".local/state"))) / "herdr-white-noise-bridge/state.json"),
    "opencode_bin": str(Path(os.environ["OPENCODE_BIN"]).resolve()),
}
config.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
with config.open("x") as output:
    output.write("".join(f"{key} = {json.dumps(value, ensure_ascii=False)}\n" for key, value in values.items()))
config.chmod(0o600)
unit = Path(os.environ["CONFIG_ROOT"]) / "systemd/user/herdr-white-noise-bridge.service"
text = unit.read_text()
old = "ExecStart=%h/.local/bin/herdr-white-noise-bridge run"
if old not in text:
    raise SystemExit("Unexpected worker unit; inspect its ExecStart manually")
argument = json.dumps(str(config), ensure_ascii=False).replace("%", "%%")
unit.write_text(text.replace(old, f"ExecStart=%h/.local/bin/herdr-white-noise-bridge --config {argument} run"))
print(f"Configuration: {config}")
print(f"Bridge public npub: {bootstrap['npub']}")
PY

"$HOME/.local/bin/herdr-white-noise-bridge" --config "$CODR_CONFIG" status
systemctl --user daemon-reload
```

The status command validates the `npub` checksum and configuration. An empty routing list is normal before the worker's first start.

### 6. Link the plugin and verify local operation

```bash
herdr plugin link "$PWD"
herdr plugin action invoke local.white-noise.start
systemctl --user is-active wn-agent-herdr.service herdr-white-noise-bridge.service
herdr plugin list
"$HOME/.local/bin/herdr-white-noise-bridge" --config "$CODR_CONFIG" status
```

Group creation can take time. Repeat the status command until detected sessions have group IDs. For a failed or uncertain creation, follow the [recovery guide](../README.md#recovery-and-diagnostics).

The linked checkout must remain at this path. A HERDR server restart invokes the startup hook. A client disconnect does not stop the services.

If you also want the worker to start at user login, enable it explicitly:

```bash
systemctl --user enable herdr-white-noise-bridge.service
```

### 7. Verify on your phone

1. Compare the full bridge `npub` printed during configuration with the inviter in White Noise.
2. Accept a session-group invitation from that identity.
3. Send `/status` and check for a connected session.
4. Send a harmless request containing a fresh word. Ask the agent to echo it without tools or file changes.
5. Check that the correct HERDR session receives the request and its final reply returns to the same group.

A running service, a group ID, or a confirmed send does not prove receipt on the phone. Report these local results separately from the phone round trip.

## Existing installations

Inspect existing configuration, units, and MDK homes before changing them. Keep the selected account ID, phone owner, relay configuration, and session mappings.

For an existing codr installation:

1. Read its actual configuration and check that the phone `npub` matches the user.
2. Inspect the running worker and transport with `systemctl --user status`.
3. Confirm which checkout HERDR has linked with `herdr plugin list`.
4. If the installation is healthy, verify it instead of rebuilding its identity.
5. For an authorised upgrade, stop only the bridge worker before replacing its executable. Start it again afterwards.

Do not replace a shared MDK executable with another version without checking its other services. Do not reset an account or delete persistent state to repair a failed install.

### Resume an interrupted first installation

If the units or dedicated MDK home exist but the codr configuration is missing, inspect them before resuming. Do not repeat the fresh-install unit-copy commands.

After correcting and validating `OWNER_NPUB`, reuse the account already created in the dedicated home. Capture successful bootstrap output in a temporary file before replacing the public bootstrap record:

```bash
systemctl --user start wn-agent-herdr.service
(
    set -euo pipefail
    bootstrap_temp="$(mktemp "$HOME/.marmot-agents/herdr/codr-bootstrap.XXXXXX")"
    trap 'rm -f "$bootstrap_temp"' EXIT
    "$HOME/.local/bin/wn-agent" bootstrap \
        --home "$HOME/.marmot-agents/herdr" \
        --socket "$HOME/.marmot-agents/herdr/dev/wn-agent.sock" \
        --label herdr-bridge \
        --allow-welcomer "$OWNER_NPUB" \
        --no-create --no-quic --json > "$bootstrap_temp"
    mv "$bootstrap_temp" "$HOME/.marmot-agents/herdr/codr-bootstrap.json"
)
```

`--no-create` prevents a second identity. If MDK reports multiple possible accounts, stop and select the verified existing account with `--account-id-hex`. If no account was created, inspect the home before intentionally repeating the original bootstrap without `--no-create`.

Then resume at [Generate the configuration](#5-generate-the-configuration). If that file already exists, follow the existing-install checks instead of replacing it.

## Instructions for coding agents

The one-prompt request authorises a normal user-level installation and its checks. Ask only for missing information that you cannot discover safely.

### Required input

Get the installing user's own complete White Noise **public** `npub`. It is the owner authorised to prompt the bridge. A test fixture is not an installation default. Do not request an `nsec`, seed phrase, private key, or model-provider API key.

### Execution checklist

1. Read this guide before changing the machine.
2. Inspect Linux architecture, the systemd user bus, executable paths, and existing codr configuration and services.
3. Resolve the intended HERDR socket and verify it with `session.snapshot`. Ask which server to use if several are plausible.
4. Check that OpenCode already works under this user. Model authentication belongs to OpenCode and stays in its existing location.
5. For a fresh installation, follow the manual steps with the supplied owner and discovered paths. Respect the target machine's build and storage policies.
6. Reuse a compatible MDK binary. Otherwise verify the pinned release archive before installing it. Do not install an unrelated harness.
7. Keep one dedicated `herdr` MDK home and one transport service owner. Do not share the `wn-opencode` account home.
8. Create owner-only configuration with the returned bridge account ID and absolute target-machine paths. Keep the phone identity separate from the bridge identity.
9. Link the permanent checkout, start the worker, and inspect services, plugin registration, group mappings, and relevant logs.
10. Return the bridge's public `npub`, mapped-session count, and local verification result. The session groups and their connection/status messages provide the normal installation signal in White Noise.
11. Ask the user to complete the phone check. Do not claim phone delivery without evidence.

For a repeated installation request, follow [Existing installations](#existing-installations). Do not overwrite customised units or configuration, broaden sender permissions, or restart unrelated agents to make a check pass.

If a prerequisite is missing, report it precisely. Do not silently change the host platform, install system-wide services, or replace an existing connector identity.

### Optional White Noise completion notice

A separate “done” message is not required. Named session groups with connection/status messages provide the normal visible installation result.

If the user explicitly requests an extra message, follow the [completion-notice instructions](../README.md#completion-notice). Select a verified group ID from the session mapping, not its display name. Include the local result and any phone check still pending. Report whether MDK confirmed the send; do not claim phone receipt without evidence.

### Completion report

Include:

- The codr checkout path and revision.
- HERDR, OpenCode, and MDK versions and selected HERDR socket.
- Configuration path, selected bridge account, and bridge public `npub`.
- Worker and transport service status, plugin registration, and mapped-session count.
- Commands actually run and their results.
- If an extra completion notice was requested, its target group and send result.
- Whether the phone round trip passed or still needs user confirmation.
- Any specific blocker or unsupported platform requirement.

Never include private keys, provider credentials, prompt contents, or exported transcripts in the report.
