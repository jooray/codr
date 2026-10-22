# codr reference

[Overview](../README.md) · [Installation guide](INSTALL.md)

Detailed behaviour, operating controls, recovery, and known limits.

The bridge uses one dedicated MDK identity across all mapped groups. Setup keeps its account separate from other connectors.

## Behaviour

- New sessions create groups and invite the configured phone `npub`.
- Group names use the HERDR tab label: `[cd] <tab name>`, for example `[cd] review`. Workspace and agent details appear in the group description. Repository names and agent-generated conversation topics do not determine the chat name.
- A phone message prompts only the agent mapped to that group.
- Final replies come from completed stored agent messages. They exclude reasoning and tool output.
- Working, idle, blocked, and ended notices appear in the group. Blocked notices can include a labelled terminal excerpt. Unsupported OpenCode blockers use a short notice. Separate `done` notices are suppressed; the final reply signals completion.
- `/status` reports the mapped session state.
- Ended sessions retain their group. A restored native session reuses its group.
- Images and audio use the media route below. Other attachment types and arbitrary terminal key commands are not supported.
- Other detected agent backends get groups, prompts, and state notices. Structured final-reply extraction supports **OpenCode, Codex, and Claude Code**.

HERDR snapshots refresh every three seconds. Very short intermediate states can occur between snapshots. Native session references preserve routing across pane moves. Group creation waits for HERDR to report a native session reference. During identity uncertainty, the bridge defers prompts and preserves the existing group.

The worker reads session transcripts in background jobs, with at most four reads in parallel and one outstanding read per session. Each OpenCode snapshot supplies replies, tasks, and decisions together. Completed reads wake the worker without waiting for slower sessions. State changes and terminal submissions remain serial. Reads from an older session state or forwarding mode are discarded. An exiting session gets a fresh final transcript read before its mapping closes.

The four-reader limit bounds background I/O, not chat subscriptions. The bridge listens for phone messages across all mapped chats.

This removes serial transcript exports from the main loop. HERDR control requests, MDK delivery, and history recovery still await their own I/O. Parallel reads do not guarantee a fixed phone-delivery time.

### Incremental OpenCode database reads

Set `opencode_db` to the absolute path of the database used by the mapped OpenCode sessions. For a default Linux profile, this is usually `~/.local/share/opencode/opencode.db`. Expand `~` before adding the path to the configuration:

```toml
opencode_db = "/home/your-user/.local/share/opencode/opencode.db"
```

**codr** opens that database read-only and uses short SQLite read transactions. It does not change OpenCode tables, indexes, journal settings, or migrations. WAL mode lets OpenCode continue to write during a read.

The first read loads the selected session once. Later reads use the indexed per-session event sequence and fetch only messages affected by new events. Updates to existing text, tasks, and questions are included. Unchanged sessions read no message or part bodies. Deletions remove cached content. A restart, replaced checkpoint, or event-log gap rebuilds the cache once. Durable reply IDs still prevent replay after a restart.

This reader supports the v1 `message`/`part` projections and durable message-event format verified with OpenCode 1.19.3. The configured database must contain the mapped sessions. Missing sessions, unsupported message-event versions, and read errors defer collection. They do not silently switch to full exports. Without `opencode_db`, the CLI export reader remains available for installations with a different storage format.

### At desk / Away forwarding

Use the HERDR action **White Noise forwarding: At desk / Away** to open the control popup. **Toggle White Noise forwarding** switches modes directly and can be bound to a HERDR shortcut.

HERDR 0.9.1 does not list plugin actions in its main click menu. For quick access, add this to your HERDR configuration:

```toml
[[keys.command]]
key = "prefix+f"
type = "plugin_action"
command = "local.white-noise.forwarding"
description = "White Noise: At desk / Away"
```

With the default prefix, press **Ctrl+B**, release it, then press **f** without Shift. The shortcut also appears in **Keybinds** under **White Noise: At desk / Away**. HERDR reloads configuration changes live.

The plugin opens this popup once when a fresh HERDR server starts after session restore. Reattaching to an existing server does not run the startup hook again; use the shortcut. Starting the bridge manually does not open another popup.

- **At desk:** suppress outgoing messages and new group invitations while the bridge continues to listen for your phone.
- **Away:** forward progress and final replies from every mapped session.
- A new message from the configured phone to any mapped group switches **all sessions** to Away. Already processed messages, your bridge's own messages, and other senders do not switch modes.

In the popup, enter **1** for At desk, **2** for Away, **3** to toggle, or **q** to close. Press Enter after your choice. Muting applies to every mapped session; it does not stop the transport or change your groups.

The selected mode survives restarts. Mode changes discard the outgoing backlog. Before Away becomes active, the bridge checkpoints replies for all active sessions. The popup shows **Away requested** while a transcript checkpoint is pending. Completed muted turns remain suppressed even if their transcript records arrive late. An operation already submitted to MDK can still arrive after you select At desk.

The command-line controls work while the worker runs:

```sh
~/.local/bin/herdr-white-noise-bridge forwarding status
~/.local/bin/herdr-white-noise-bridge forwarding desk
~/.local/bin/herdr-white-noise-bridge forwarding away
~/.local/bin/herdr-white-noise-bridge forwarding toggle
```

Forwarding defaults to Away on an existing installation. The control file is private and separate from routing state, so changing modes does not stop either service.

### Editable progress and separate final replies

Set `edit_responses = true` in the bridge configuration to enable editable progress. This requires an MDK build with the `edit_message` API, such as upstream commit `1d2b40dbd9bea1f7e9beb4ecab04482647a9e5a6`. The published `wn-agent` 0.13.0 binaries do not include that API. With those binaries, leave this mode disabled.

Each response starts as one `> working` progress message. The bridge edits that message with the current task list, when available. It sends the final answer as a separate message through the normal durable send path, with its own retry key. The bridge never edits that final answer. When possible, it marks the progress message `> done`.

OpenCode to-do writes, Codex plan updates, and Claude Code to-do/task tools supply structured task data. The bridge does not prompt a model to obtain progress. Message IDs and pending updates survive bridge restarts. Updates coalesce between snapshots, and edit attempts are at least two timestamp seconds apart.

Progress is best effort. Final replies take priority over progress updates, and progress requests have a three-second local deadline. A failed progress edit does not block a queued final answer. An MDK response or local readback does not prove phone delivery. Very long final replies use continuation messages to preserve all text. Progress messages are ordinary encrypted chat messages, not Nostr ephemeral events.

If HERDR becomes idle before the agent stores its final reply, the bridge keeps that response pending and retries the transcript read. After inspection, `clear-prompt` can release a cancelled response that has no final record.

The installer preserves the pinned MDK transport. Editable progress needs a separately installed MDK build with edit support; ordinary progress, decisions, and final replies work without it.

### Answer decisions from the phone

When OpenCode asks one single-choice question, **codr** sends a clean question card with numbered options. Reply with **1**, **2**, or an exact option label. The answer goes to the active question, even while HERDR reports the agent as blocked; it is not submitted as a new chat prompt.

Decision cards and answers do not require editable progress.

The bridge checks the session, pending tool request, and visible question before sending the native number shortcut. If a question was not answered through **codr** and is replaced, the new card requires its short code, for example `qabc123 1`. Duplicate deliveries and uncertain submissions are not sent again automatically.

Decision routing was verified against OpenCode 1.18.35's single-choice question UI with 1–9 listed options. The gallery uses OpenCode 1.19.3, but does not verify live decision routing on that version. Custom text entry, multi-select/multi-question dialogs, permission approvals, and decisions in other backends require the terminal. Unsupported OpenCode blockers show a short notice instead of forwarding unrelated tool output.

HERDR 0.9.1 cannot make the final identity check and terminal input atomic. Avoid answering the same question simultaneously from the terminal and phone. After an uncertain answer, inspect the terminal before using `clear-prompt` and sending a new reply.

## Shared chats and owner-only control

Group members can discuss a project and read the agent's replies. Only messages from the phone public key in **codr**'s configuration become agent prompts. Other participants' messages are not added to the agent's context. The owner can relay a suggestion in a new prompt.

Invitations require group admin authority in White Noise. Group membership and **codr** prompt permission are separate. Automatic **codr** groups make the bridge identity their admin. The phone owner needs separate admin authority to invite from the app.

The gallery uses an owner-created group, bound to an isolated demo session. Alex creates the group and invites Sam through White Noise. Sam is a scripted second demo identity. Its message appears in the chat but produces no agent prompt. Alex's next message produces one prompt and a separate reply. This does not grant additional people agent control.

## Images and voice messages

Images and audio are available in **codr** 0.5.0. Voice prompts require the local transcription worker described below.

**codr** asks MDK's `download_media` API to fetch and decrypt each attachment. Keys stay in MDK. PNG, JPEG, WebP, and GIF images are copied into a private `media` directory beside bridge state. The prompt includes the local file paths and the original caption. The agent needs its normal local-file access to inspect them. Files remain there for later conversation access until you remove them.

Audio goes through FFmpeg to 16 kHz mono WAV, then to a local Phonon-2 worker. Its transcript enters the same prompt route as typed text, without a confirmation step. Supported question answers also use that route. Download and local transcription add time before the agent starts.

The phone sees `> transcribing` while audio processing runs. With `edit_responses = true` and an edit-capable MDK build, **codr** edits that bubble to `> transcribed` with the transcript quoted underneath. The edit survives restarts and retries independently of agent submission. Without edit support, the transcript arrives as a separate message. Images use `> downloading`. Long transcript displays are shortened with a notice; the agent receives the full prepared prompt.

Downloads and transcription run separately from session updates, one media message at a time. Pending messages survive bridge restarts. Sender checks and session checks still apply. A preparation failure sends no agent prompt. An uncertain terminal submission is not repeated automatically.

Offline media recovery also needs `timeline_message_get` with opt-in `include_media` download references. The local MDK source extension provides this. Older connectors return only attachment summaries. **codr** defers these without consuming the message and can accept a complete live event later. This extension is not in the published MDK 0.13.0 binaries.

Limits are four attachments per message, 25 MiB per file, and five minutes per audio file. Combined captions, transcripts, and image paths must fit within 30,000 UTF-8 bytes. Unsupported files, truncated messages, and empty transcripts do not reach the agent.

### Local speech worker

Setup installs `fermion-research` 0.2.12 with CPU-only PyTorch and downloads Phonon-2, approximately 164 MB plus runtime dependencies. FFmpeg is required. The worker uses an owner-only Unix socket under `$XDG_RUNTIME_DIR/codr-phonon` and processes recordings locally.

`codr-phonon.service` has a 1 GiB memory limit and no swap allowance. The bridge starts it as a dependency. A healthy existing **codr** worker is reused. If the worker cannot start within the limit, setup reports failure rather than raising the limit.

Fresh setup includes audio by default. Use `--no-audio` for a text/image-only installation. To add Phonon to an existing installation:

```sh
python3 scripts/setup.py --phonon
```

This preserves the identity and mappings, adds the transcription command, and restarts only the bridge when its configuration changes. Existing custom transcription commands are not overwritten. A plain setup rerun preserves the existing selection.

```sh
systemctl --user status codr-phonon.service
journalctl --user -u codr-phonon.service
```

Normalised audio files are temporary and removed after each attempt. MDK manages its downloaded temporary files. Image copies stay in **codr**'s private media directory. Do not commit media, transcripts, or model files to Git.

## Supported reply sources

| Agent | Final-reply source | Default transcript location |
| --- | --- | --- |
| OpenCode | Completed text from `opencode --pure export SESSION_ID` | Managed by OpenCode |
| Codex | Successful `task_complete` / `turn_complete` events with a turn ID and final message | `~/.codex/sessions` and `~/.codex/archived_sessions` |
| Claude Code | Completed main-conversation assistant responses with `message.stop_reason = "end_turn"` | `~/.claude/projects/<project>/<session-id>.jsonl` |

The bridge reads the existing session. It does not start another model invocation, change authentication, or use terminal snapshots as final replies. HERDR must report the native session ID.

Codex and Claude Code readers verify session identity, ignore partial records, and reject ambiguous transcript files. Claude's null stop reasons and duration markers alone are not treated as successful replies. Transcript formats are internal to the agent tools; unsupported or incomplete records are not guessed into final answers.

For custom profiles, set absolute `codex_home` and `claude_home` paths in the bridge configuration. Defaults use `CODEX_HOME` / `CLAUDE_CONFIG_DIR` when available, otherwise the locations above. Fresh setup records these paths explicitly.

Existing history is baselined when a group is first attached or reply support is first enabled for an older group. It is not reposted. Later completed replies use persistent message IDs to avoid duplicates after restart.

## Roadmap

- [x] Add structured final replies for existing Codex sessions.
- [x] Add structured final replies for existing Claude Code sessions.
- [ ] Add structured final replies for further HERDR-supported agents.
- [ ] Create new agent conversations from White Noise by messaging the main worker, which starts a HERDR session and creates its mapped group.
  - Select the HERDR workspace, directory, agent, and model.
- [ ] Let the owner authorise additional people to prompt the same agent in a White Noise group.
  - Add owner-managed prompt permissions per group, including revocation.
  - Distinguish agent-directed requests from discussion between participants.
- [x] Download inbound images through MDK and pass their local paths and captions to the mapped agent.
- [x] Convert White Noise voice messages and audio attachments directly into text prompts with a local Phonon-2 worker.
  - [ ] Add guided selection of other existing transcription services or local whisper.cpp.
  - Review the existing Rust audio preprocessing and configurable transcription pipeline in [Nostr Codex Phone](https://github.com/tidley/nostr-codex-phone/blob/main/rust/src/transcribe.rs), which already supports whisper.cpp.
  - Evaluate [whisper.cpp](https://github.com/ggml-org/whisper.cpp) and Phonon-2, as used in [Phonon Wayland Dictation](https://github.com/tidley/phonon-wayland-dictation).

## Requirements

- Linux x86_64 or aarch64 with a systemd user manager.
- Python 3.11 or newer and Git. Rust is only needed when building from source.
- HERDR 0.9.1 or newer, with a running local server.
- MDK `wn-agent` 0.13.0, including `group_create` and `timeline_list` in `marmot.agent-control.v2`.
- A supported agent: OpenCode, Codex, or Claude Code. OpenCode needs `--pure export SESSION_ID`; Codex and Claude Code need readable native transcripts.
- HERDR native session references for the agents you want to control.
- Your phone must publish a compatible Marmot KeyPackage. The phone and bridge need reachable relays.

The repository does not implement MLS or Nostr cryptography. MDK owns keys, encrypted group state, relay connections, invitations, and delivery.

## Build

Build with Cargo:

```sh
cargo test --locked --tests
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

`Cargo.lock` is committed because **codr** is an application. It pins the dependency versions used by the build and installation commands.

## Install

Use the [installation guide](INSTALL.md) for complete commands, including the pinned MDK download and checksum verification.

Install a pinned release through HERDR, then open its guided setup tab:

```sh
herdr plugin install tidley/codr --ref v0.5.0
herdr plugin action invoke local.white-noise.setup
```

The installation hook downloads and verifies the matching **codr** binary from GitHub Releases. Setup asks for your public `npub`, installs or reuses the pinned MDK transport, and configures dedicated user services. No Rust toolchain is required for this path.

For a non-interactive installation by a coding agent, see the [agent instructions](INSTALL.md#instructions-for-coding-agents).

HERDR starts the plugin worker through its startup hook after restore. The worker's user service keeps it alive across client disconnects. Use `systemctl --user enable herdr-white-noise-bridge.service` if it must start independently at login.

Do not share the existing `wn-opencode` connector home. That adapter can also answer messages sent to its identity.

## GitHub builds

GitHub Actions runs Rust formatting, tests, Clippy, and isolated installer/setup tests on pushes and pull requests. A version tag builds native Linux x86_64 and aarch64 binaries with musl, then publishes archives and SHA-256 checksums to [GitHub Releases](https://github.com/tidley/codr/releases).

The installer selects the release version from the plugin checkout. It never silently substitutes a newer release. MDK uses its own pinned upstream release rather than a rebuilt or copied transport implementation.

## Verify on the phone

1. Accept an invitation from the verified bridge identity.
2. Send `/status` and check that it identifies a connected session.
3. Send a harmless request with a fresh word. Ask for that word without tools or file changes.
4. Check that the matching HERDR session receives the prompt.
5. Check that its completed reply returns to the same White Noise group.

A relay acknowledgement proves publication, not that the phone displayed or accepted a message. Phone verification requires the round trip above.

## Recovery and diagnostics

```sh
~/.local/bin/herdr-white-noise-bridge status
journalctl --user -u herdr-white-noise-bridge.service
```

State and the pending outbox use owner-only files. Keep them on persistent storage, with the MDK account home. Do not put these files in Git.

The bridge saves consumed phone-message IDs before it submits terminal input. It never retries an uncertain prompt automatically. This favours avoiding duplicate actions over automatic replay after a crash.

If a prompt result remains uncertain, inspect the live session first. Stop the worker and release the pending flag without resubmitting:

```sh
systemctl --user stop herdr-white-noise-bridge.service
~/.local/bin/herdr-white-noise-bridge clear-prompt 'SESSION_KEY_FROM_STATUS'
systemctl --user start herdr-white-noise-bridge.service
```

Group creation has no idempotency key in MDK 0.13.0. Connection failures before submission retry automatically. A timeout or ambiguous connector error requires inspection, including a missing KeyPackage error that MDK reports only as `app_error`.

After inspection, stop the worker. Bind the existing group or explicitly permit another creation attempt:

```sh
~/.local/bin/herdr-white-noise-bridge resolve-create 'SESSION_KEY_FROM_STATUS' --group GROUP_HEX
# Use --retry only after confirming that the previous attempt created no group.
~/.local/bin/herdr-white-noise-bridge resolve-create 'SESSION_KEY_FROM_STATUS' --retry
```

Then start the worker again. Failed sends retain their private outbox entries. Other groups continue after a group-specific send failure.

Offline recovery scans retained group history every minute and after reconnect. It uses existing rows as pagination cursors and durable message IDs for deduplication. Sender timestamps do not control eligibility. This also recovers late relay deliveries and messages from a phone with a different clock.

MDK truncates long timeline text to 8,192 characters. The bridge rejects truncated recovered prompts and asks for a shorter message.

### HERDR input limitation

HERDR 0.9.1 does not accept an expected native-session ID in `agent.prompt`. The bridge verifies the current terminal and session immediately before submission. A concurrent session replacement between verification and submission remains a server-API race. Avoid replacing an agent while sending a phone prompt. Atomic protection requires an upstream expected-session guard.

## Group names and completion notices

To update existing group names without changing their session mappings, stop the worker and run:

```sh
~/.local/bin/herdr-white-noise-bridge rename-groups
```

Then start the worker again. This refreshes names and descriptions without changing group IDs. Ended sessions without a stored tab label use their existing title, replacing earlier **codr** prefixes or suffixes. Unavailable historical workspace details appear as `unknown`.

Tab renames do not automatically rewrite White Noise titles; run `rename-groups` to apply them. Sessions in tabs with identical labels can share a display name, while their routing remains separate.

### Completion notice

Stop the worker before using its state-changing CLI. To send a notice to one mapped group:

```sh
~/.local/bin/herdr-white-noise-bridge notify --group GROUP_HEX 'The bridge is ready.'
```

Without `--group`, the command sends the notice to every mapped group. The command returns success only after MDK confirms publication.

## Screenshot provenance

The [README gallery](../README.md#screenshots) contains side-by-side composites of real HERDR 0.9.1, OpenCode 1.19.3, and White Noise Android interfaces.

All seven images use the same fictional Garden planner project. The desktop replays matching history imported into an isolated OpenCode installation. The phone uses a dedicated Alex demo profile. A synthetic HERDR/OpenCode fixture supplies agent state and scripted replies through the real **codr** bridge and an isolated MDK transport.

The captures show current `[cd]` names, blockquote status labels, editable tasks, decisions, separate replies, images, local voice transcription, a three-member chat, and a White Noise project folder. The folder is configured manually, not created by **codr**.

The reminder design is a generated demo image. The voice clip uses synthetic speech and passes through the real Phonon-2 worker. Its actual transcript reaches the fixture as the prompt, without confirmation. Sam uses a separate MDK identity. Alex's invitation and both participants' chat messages use real encrypted group messaging. Verification checks that Sam's message causes zero agent prompts and Alex's message causes exactly one.

The phone's `1` reply continues the question fixture, not a live OpenCode dialog. The captures do not verify a live coding-model run, a real project change, or a live decision dialog end to end. Editable task captures use an MDK build with edit support. Android status and navigation bars are cropped. The images contain no private conversations or text/EXIF metadata.

## Acknowledgements

**codr** builds on the work of these projects and their maintainers and contributors:

- White Noise: the mobile messaging application used as **codr**'s phone interface, maintained in the [iOS](https://github.com/marmot-protocol/whitenoise-ios) and [Android](https://github.com/marmot-protocol/whitenoise-android) repositories.
- [Marmot Development Kit (MDK)](https://github.com/marmot-protocol/mdk): the `wn-agent` connector that provides identity management, encrypted group state, relay transport, invitations, and message delivery.
- [Marmot protocol](https://github.com/marmot-protocol/marmot): the end-to-end encrypted group messaging protocol built on MLS with Nostr identity.
- [HERDR](https://github.com/herdrdev/herdr): the agent runtime, plugin system, and local API that **codr** uses to discover and control sessions.
- [OpenCode](https://github.com/anomalyco/opencode): the coding-agent runtime whose stored final replies **codr** currently reads.
- [Codex](https://github.com/openai/codex): the coding-agent runtime and native rollout records used by **codr**'s Codex reply reader.
- [Claude Code](https://github.com/anthropics/claude-code): the coding-agent runtime whose native session transcripts **codr** reads for completed main-conversation replies.
- [Phonon-2](https://github.com/fermionresearch/phonon): the local speech model and Fermion runtime used for audio transcription. Its model weights and runtime remain upstream software.

**codr** provides the routing bridge between these systems. The messaging protocol, encryption, transport, mobile app, and agent runtimes come from the upstream projects above.
