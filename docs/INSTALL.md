# Install codr

**codr** is a Linux HERDR plugin backed by two systemd user services: the Rust bridge and its MDK transport.

## One-prompt installation

Give your coding agent this prompt, with your own complete public key:

> Install https://github.com/tidley/codr on this machine. My White Noise public key is npub1…. Follow the agent installation guide and report what you verified.

The [agent section](#instructions-for-coding-agents) provides the non-interactive setup steps.

## Requirements

- Linux x86_64 or aarch64.
- Python 3.11 or newer and Git.
- A working systemd user manager under the account that runs HERDR.
- HERDR 0.9.1 or newer, with a running local server.
- A working, authenticated OpenCode, Codex, or Claude Code installation.
- HERDR native session IDs and access to the selected agent's local transcripts.
- White Noise on your phone and your own public `npub`.
- Access to GitHub release assets and the configured White Noise relays.

Agent authentication stays in its existing location. Setup does not request a provider API key, `nsec`, seed phrase, or private key.

The phone account must publish a compatible Marmot KeyPackage before the bridge can invite it. Open White Noise and let it connect to its relays.

## Install through HERDR

Run these commands as your normal user, not root:

```bash
herdr plugin install tidley/codr --ref v0.4.0
herdr plugin action invoke local.white-noise.setup
```

HERDR previews and runs the manifest's installation hook, then registers the plugin. The hook downloads the binary for the host architecture and checks its SHA-256 checksum before installation. Users do not need Rust or Cargo.

The Setup action opens a HERDR tab. Enter your own White Noise public `npub` there. Setup:

1. Validates the key through **codr**'s configuration parser before changing accounts or services.
2. Checks the HERDR socket, available supported agents, Git, and systemd user manager. Codex-only and Claude-only installations do not need OpenCode.
3. Reuses a compatible `wn-agent` 0.13.0 binary or downloads a verified upstream binary into **codr**'s own binary directory.
4. Creates or reuses the dedicated `herdr-bridge` identity under `~/.marmot-agents/herdr`.
5. Writes owner-only configuration and dedicated user units.
6. Starts the transport and bridge, then prints the bridge's public `npub` and local result.

The MDK release is pinned independently at `wn-agent-v0.13.0`. Setup never replaces a shared MDK binary with a different version.

The binary installer selects the **codr** version from the installed plugin checkout. A pinned installation uses that exact version's assets and checksums. A missing release asset or checksum stops installation rather than falling back to unverified code.

### Select a HERDR server

The setup tab receives `HERDR_SOCKET_PATH` from its HERDR server. For direct setup outside a tab, pass `--herdr-socket /absolute/path/to/herdr.sock` or set `HERDR_SOCKET_PATH` explicitly.

One **codr** configuration targets one HERDR server. Select the intended server before installation when using named sessions:

```bash
export HERDR_SOCKET_PATH="/absolute/path/to/the/intended/herdr.sock"
herdr api snapshot
```

### Existing installations and upgrades

Setup preserves existing configuration, identities, mappings, and customised units. A supplied owner that differs from the configured phone account is rejected.

For an existing installation, setup validates the configuration and starts its worker if needed. It does not recreate the MDK account or restart a running worker. `--check` performs configuration validation without changing services.

The binary installer replaces **codr**'s executable atomically. A running worker continues with its previous executable until restarted. After an authorised upgrade, wait for work and group creation to settle before restarting only the **codr** worker:

```bash
systemctl --user restart herdr-white-noise-bridge.service
```

HERDR refuses a GitHub install over a locally linked copy of the same plugin. To switch from a local checkout to managed releases, inspect the current installation, then unregister the local plugin before installing the release:

```bash
herdr plugin list
herdr plugin unlink local.white-noise
herdr plugin install tidley/codr --ref v0.4.0
```

Unlinking leaves the local checkout and account state in place. Do not delete the MDK home or reset the bridge to perform an upgrade.

### Interrupted first setup

Re-run setup with the same owner and socket. Matching service files are reused. If the MDK account already exists, setup selects its account ID and bootstraps with `--no-create`.

Ambiguous accounts or service files that differ from the proposed fresh configuration require inspection. Setup reports the conflict instead of overwriting them. Do not remove persistent state to make installation pass.

## Verify on the phone

1. Compare the full bridge `npub` printed by setup with the inviter in White Noise.
2. Accept a session-group invitation from that identity.
3. Send `/status` and check that it reports a connected session.
4. Send a harmless request with a fresh word. Ask the agent to echo it without tools or file changes.
5. Check that the intended HERDR session receives the prompt and its final reply returns to the same group.

Group creation can take time. Group names and initial connection/status messages provide the normal visible installation signal. They do not prove the phone-to-agent round trip.

An extra completion notice is optional. If requested, follow the [completion-notice instructions](REFERENCE.md#completion-notice), select a verified group ID, and distinguish an MDK-confirmed send from phone-confirmed receipt.

## Diagnostics

```bash
herdr plugin list
systemctl --user status wn-agent-herdr.service herdr-white-noise-bridge.service
herdr-white-noise-bridge status
journalctl --user -u wn-agent-herdr.service -u herdr-white-noise-bridge.service --no-pager -n 50
```

Configuration lives under `${XDG_CONFIG_HOME:-~/.config}/herdr-white-noise-bridge/config.toml`. The worker unit specifies that path explicitly. State uses `${XDG_STATE_HOME:-~/.local/state}/herdr-white-noise-bridge/state.json`.

Current source builds support `opencode_db` for incremental, read-only OpenCode database reads. Select the database for the mapped sessions explicitly. See [reader configuration](REFERENCE.md#incremental-opencode-database-reads). The v0.4.0 binary does not support this setting.

For Codex profiles, `codex_home` selects the directory containing `sessions` and `archived_sessions`. It defaults to `CODEX_HOME` or `~/.codex`. For Claude Code, `claude_home` selects the directory containing `projects`; it defaults to `CLAUDE_CONFIG_DIR` or `~/.claude`. Use absolute paths. Setup saves the installing shell's roots for new configurations, and preserves existing configurations during upgrades.

If a supported agent appears without a native session ID, check its HERDR integration. Enable the matching HERDR integration when needed rather than guessing a transcript from its terminal title or repository. **codr** does not copy credentials or reconfigure the model provider.

Native transcript reads are bounded to 128 MiB per file and 16 MiB per record. Missing, ambiguous, or unsupported transcript data stays unavailable for final-reply extraction. Claude Code must record an explicit `end_turn` stop reason; the bridge retries later if the writer has not persisted it yet.

See [Recovery and diagnostics](REFERENCE.md#recovery-and-diagnostics) for uncertain group creation or prompt submissions. Do not blindly repeat those operations.

## Instructions for coding agents

### Required input and discovery

Get the user's own complete White Noise **public** `npub`. Never use a test fixture or another person's public key as the owner. Do not request private keys or provider credentials.

Before changing the host:

1. Verify Linux architecture, Python 3.11+, Git, the systemd user bus, and executable paths.
2. Inspect existing **codr** configuration, services, identities, and plugin registration.
3. Resolve the intended HERDR socket and verify it with `session.snapshot`. Ask which server to use if several are plausible.
4. Check that at least one supported agent already works under this user and reports a native HERDR session reference. Do not change model authentication or execution permissions.

### Non-interactive installation

Set `OWNER_NPUB` to the public key supplied by the user. Export the selected `HERDR_SOCKET_PATH` before running HERDR commands.

```bash
herdr plugin install tidley/codr --ref v0.4.0 --yes
PLUGIN_ROOT="$(herdr plugin list --plugin local.white-noise --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["result"]["plugins"][0]["plugin_root"])')"
python3 "$PLUGIN_ROOT/scripts/setup.py" --npub "$OWNER_NPUB" --herdr-socket "$HERDR_SOCKET_PATH"
```

Use [Existing installations and upgrades](#existing-installations-and-upgrades) instead of overwriting a local plugin registration. For a local source checkout, the same setup script works after the binary has been installed.

For a passive check of an existing configuration:

```bash
python3 "$PLUGIN_ROOT/scripts/setup.py" --check
```

Do not overwrite customised configuration or units, broaden sender permissions, share another harness's account home, or restart unrelated agents to make checks pass.

### Completion report

Report the installed revision/version, configuration path, selected HERDR socket, service status, bridge public `npub`, and mapped-session count. Use the bridge's `status` command to inspect mappings after setup.

State exactly which checks ran. Report the phone round trip as pending unless the user or observed inbound/reply traffic confirms it. Never include private keys, provider credentials, prompts, or exported transcripts.

If a prerequisite or release asset is unavailable, report that blocker precisely rather than claiming installation succeeded.

## Build from source

This path requires Rust and Cargo. Use the target host's build/storage policy. Current source adds `[cd] session` chat names, image downloads, and local Phonon-2 transcription. Audio setup needs FFmpeg and downloads the CPU runtime plus the approximately 164 MB model. Its service is capped at 1 GiB RAM.

```bash
git clone https://github.com/tidley/codr.git
cd codr
git checkout master
cargo test --locked --tests
cargo install --locked --path . --root "$HOME/.local"
herdr plugin link "$PWD"
python3 scripts/setup.py --npub "$OWNER_NPUB"
```

`plugin link` does not run the download hook. Keep the linked checkout at its permanent location. The startup hook starts the configured worker after HERDR restores its session; a client disconnect does not stop it.

Use `python3 scripts/setup.py --npub "$OWNER_NPUB" --no-audio` to skip speech setup. Existing installations can add it with `python3 scripts/setup.py --phonon`. See [media and speech details](REFERENCE.md#images-and-voice-messages).
