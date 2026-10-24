# **codr** reference

[Overview](../README.md) · [Installation guide](INSTALL.md)

Detailed behaviour, operating controls, recovery, and known limits.

The bridge uses one dedicated MDK identity across all mapped groups. Setup keeps its account separate from other connectors.

## Behaviour

- New sessions create groups and invite the configured phone `npub`.
- Group names pair the **herdr** tab label and workspace: `<tab name> (<workspace>)`, for example `review (Acme)`. Fresh sessions launched from an existing chat add the selected agent inside the parentheses, such as `review (Acme - codex)` or `review (Acme - claude)`, without renaming the original chat. Without a workspace label, these fresh chats use `review (codex)`; ordinary chats keep only `<tab name>`. The agent suffix survives restarts, pane moves, and metadata updates, and is kept when long titles are shortened. Workspace and agent details also appear in the group description. Repository names and agent-generated conversation topics do not determine the chat name.
- A phone message prompts only the agent mapped to that group.
- Complete owner text is handed to the agent immediately, even while it is working, without an interrupt. A blocked native dialog requires its supported answer or `/cancel`. Media and previously accepted deliveries remain durable; `/queue` inspects them. `/clear-queue` is retired and does not discard accepted work.
- Final replies come from completed stored agent messages. They exclude reasoning and tool output.
- Working, idle, blocked, and ended notices appear in the group. Blocked notices can include a labelled terminal excerpt. Unsupported OpenCode blockers use a short notice. Separate `done` notices are suppressed; the final reply signals completion.
- `/status` reports the mapped session state.
- `unmute` or `/unmute` resumes forwarding for all chats without sending a prompt or answering an agent question.
- Ended sessions retain their group. A restored native session reuses its group.
- Images and audio use the media route below. Other attachment types and arbitrary terminal key commands are not supported.
- Other detected agent backends get groups, prompts, and state notices. Structured final-reply extraction supports **OpenCode, Codex, and Claude Code**.

**herdr** snapshots refresh every three seconds. Very short intermediate states can occur between snapshots. Native session references preserve routing across pane moves. Group creation waits for **herdr** to report a native session reference. During identity uncertainty, the bridge defers prompts and preserves the existing group.

The worker reads session transcripts in background jobs, with at most four reads in parallel and one outstanding read per session. Each OpenCode snapshot supplies replies, tasks, and decisions together. Completed reads wake the worker without waiting for slower sessions. State changes and terminal submissions remain serial. Reads from an older session state or forwarding mode are discarded. An exiting session gets a fresh final transcript read before its mapping closes.

The four-reader limit bounds background I/O, not chat subscriptions. The bridge listens for phone messages across all mapped chats.

This removes serial transcript exports from the main loop. **herdr** control requests, MDK delivery, and history recovery still await their own I/O. Parallel reads do not guarantee a fixed phone-delivery time.

### Start a workspace from your phone

New bridge accounts use the display name **Pat**. New session chats introduce Pat as the messenger between White Noise and herdr and direct you to `/help`. Verify the account's full `npub`, not its display name. Setup preserves existing profiles and account keys. See [Pat onboarding](INSTALL.md#pat-your-bridge-identity).

The phone controls below do not require the optional **portr** desktop/SSH endpoint. That endpoint is disabled unless `portr_enabled = true` is configured or the worker is explicitly started with `run --portr`. See [optional installation](INSTALL.md#optional-portr-desktopssh-access).

Send `/new` in any linked chat. The plugin guides you through **existing or new herdr workspace → folder name → installed agent → model → confirmation**. The four selection headings use bracketed progress: `[1/4] Choose a herdr workspace:`, `[2/4] Folder name:`, `[3/4] Choose an installed agent:`, and `[4/4] Choose a model for OpenCode:` (or the selected agent). The title is outside the brackets. Each prompt explains how to reply:

1. Existing workspaces are listed first. The final numbered choice is **Create new workspace**, followed by **0. Cancel**. With two workspaces, reply `3` to create a new one. Labels and stable IDs distinguish workspaces with the same name. Use `next` or `previous` for ten-item pages; numbers are global across pages, and the new-workspace number is always one higher than the total workspace count. Page counters appear only when there is more than one page. `/new workspace 1` and `new` also work.
2. Reply with your chosen folder name, such as `my-idea`, or `0` to cancel. `/new folder my-idea` remains supported. A new workspace uses the folder's final component as its name. An existing workspace receives a new project tab with an explicit cwd; its existing tabs and workspace name stay unchanged.
3. Reply with a listed number to choose an installed agent: OpenCode, Codex, or Claude Code. `/agent 1` also works.
4. Model selection starts with **Backend default**, when supported, then provider groups such as **View OpenAI / Codex models**, **View Claude models**, and **View Free models**. Select a provider/category to receive its **full available model list**, with no pagination. This also applies when only one provider is available. Variants appear only after selecting a model that has them, as a separate full list. Use `back` to return to the preceding model menu. Long lists use the existing encrypted message-size limit and are delivered in consecutive messages without dropping options. `/model 2` also works. A default-only catalogue shows just its default choice. Older saved flat menus keep their original numbering and pagination until you start a fresh selection.
5. The complete preview shows the existing workspace's label and ID or the new-workspace choice, full folder path, agent, model, and whether the folder exists or will be created. It says **Confirm within ten minutes** and **Confirm by typing:** followed by the destination workspace name (or the new workspace name). Reply with that exact displayed name to confirm, or `0` to cancel. A wrong name stays within the confirmation flow, never becomes an agent prompt, and creates nothing. After expiry, the next plain reply closes the expired confirmation and reports expiry; later messages follow normal chat routing. `/new confirm` remains supported. If a workspace name is `0` or begins with `/`, the preview uses `/new confirm` to avoid conflicting with controls.

Workspace selection does not create anything. If the existing-workspace list is unavailable, the menu says so: retry with `/new`, or explicitly choose a new workspace. Before launch, the plugin checks the selected existing workspace again. If it was renamed, a new confirmation is required. If it is gone or unavailable, launch stops without creating a folder or using another workspace. There is no automatic fallback to a new workspace.

No long code is needed. Short answers apply to the current menu in this chat, which survives restarts and expires after ten minutes. All editable setup menus and pending confirmations offer **0. Cancel**. While a selection menu is open, numbers belong to that menu, not to an agent's pending decision. While the folder prompt is open, plain text is treated as a folder name; while a launch confirmation is waiting, it is treated as a confirmation name. Slash commands such as `/help` still work. To answer the agent instead, cancel setup first with `0` or `/new cancel`. Ordinary text and numbers without a setup menu keep their existing behavior. Explicit coded commands remain supported for compatibility and reject stale codes. `/new cancel CODE` cancels a particular launch, and `/new status` inspects the last launch. Starting a new configuration invalidates the previous unconfirmed launch. Uncertain or in-progress operations still require local inspection and cannot be cleared by `0`.

The one-line `/new my-idea my-idea opencode` shortcut remains available, creates a new workspace, and uses the backend's configured default model. Guided selection detects executable installations in the worker's environment; installation does not prove backend authentication. The configured OpenCode executable must use the same service/profile as the OpenCode executable launched by **herdr**.

Folders are restricted to `launch_root`, which defaults to `~/code`. The root must already exist. To override it, set an absolute path such as `launch_root = "/home/your-user/projects"` in the bridge configuration. Relative paths, absolute paths, and `~/` paths must all resolve below that root. Folder names cannot contain spaces, hidden components, traversal, or symlinks. Agent arguments and arbitrary commands are not supported.

Existing folder contents are kept. Missing folders are created only after confirmation. If the folder's identity or existence changes after the preview, a new confirmation is required. Launch controls do not change forwarding mode or existing prompt queues. Captions, voice transcripts, and **portr** queue messages cannot confirm a launch.

The workspace opens without stealing focus, and the agent starts in its project directory. No initial prompt is sent. Guided OpenCode V2 launches first create a native session with the selected model and location, then attach the terminal to that session. Codex and Claude receive only the selected `--model` argument; arbitrary arguments are not accepted. The normal White Noise invitation follows after **herdr** reports the agent's native session. Backends that do not publish a native session until their first interaction may require that interaction locally.

Requests survive worker restarts. If a workspace or startup acknowledgement is lost, no automatic retry runs. `/new status` shows the recorded phase and any confirmed workspace ID. Inspect **herdr** locally before recovery. Keep partial folders and workspaces; this command never deletes them. An interrupted or uncertain request blocks a replacement in that chat. Local recovery requires inspecting the recorded request; there is no phone-side force-retry command.

### Agent and model selection in existing chats

- `/agent` opens the installed-agent menu for the mapped project. Selecting an agent and model produces a fresh-session confirmation. On confirmation, **codr** opens a new tab in the same workspace and eventually creates a separate White Noise chat. It does not stop the old agent, transfer its history, move its prompts, or remap its chat. The existing project must be below `launch_root`.
- `/model` in an OpenCode V2 chat lists models for that project's location. After selection, `/model confirm` changes the model for subsequent turns in that same native session. It keeps the chat and history. Confirmation requires a live idle session with no waiting or in-flight bridge prompts, decision, or permission request. A busy session is not interrupted; confirm again with a new message once it is idle.
- `/model` in a Codex or Claude chat opens model selection for a **fresh session in a separate chat**, clearly stated before confirmation. In-place switching for these backends is not supported. The old session and queue remain unchanged.

OpenCode models come from its authenticated V2 `/api/model` catalogue, filtered to enabled, non-deprecated choices. Provider IDs and available variants distinguish otherwise similar names. **OpenAI / Codex** is the OpenAI provider group inside OpenCode, not a switch to the Codex agent. The **Free models** group requires explicit zero input, output, and cache prices in the API; absent prices are not evidence that a subscription model is free. The catalogue uses the target folder's configuration, or the nearest existing parent for a new folder. Codex choices come from its local `models_cache.json` and are labelled **cached**; an absent cache leaves only the backend default. Claude choices are its documented `sonnet`, `opus`, and `haiku` aliases, not a verified account-entitlement list. Backend authentication and account access can still prevent startup.

Selected models and installed agents are checked again before a launch. The plugin never changes global backend settings. It saves intent before a model switch and reads back the selected model. An unconfirmed switch is not repeated, even after a restart, and requires local inspection. Selection commands do not change forwarding mode. Captions, voice transcripts, truncated messages, other senders, and **portr** queue input cannot operate these controls.

### Incremental OpenCode database reads

Set `opencode_db` to the absolute path of the database used by the mapped OpenCode sessions. For a default Linux profile, this is usually `~/.local/share/opencode/opencode.db`. Expand `~` before adding the path to the configuration:

```toml
opencode_db = "/home/your-user/.local/share/opencode/opencode.db"
```

**codr** opens that database read-only and uses short SQLite read transactions. It does not change OpenCode tables, indexes, journal settings, or migrations. WAL mode lets OpenCode continue to write during a read.

The first read loads the selected session once. Later reads use the indexed per-session event sequence and fetch only messages affected by new events. Updates to existing text, tasks, and questions are included. Unchanged sessions read no message or part bodies. Deletions remove cached content. A restart, replaced checkpoint, or event-log gap rebuilds the cache once. Durable reply IDs still prevent replay after a restart.

This reader supports the v1 `message`/`part` projections and durable message-event format verified with OpenCode 1.19.3. If required projection columns, the session's durable event log, or a supported message-event version are unavailable, **codr** logs the compatibility change and uses CLI export for that session. This also applies when OpenCode upgrades while the bridge is running. The configured `opencode_bin` must export the same mapped sessions. CLI exports support OpenCode 1.x and 2.x, but full exports cost more than incremental database reads.

Missing sessions in a compatible database, unreadable or corrupt databases, locks, and malformed content remain errors. They do not select another reader or consume pending phone input. A failed export also leaves the history checkpoint pending. Forwarding resumes only after a successful checkpoint; muted history is not replayed.

Setup validates live supported session histories before it changes accounts or services. If no supported session is live, it reports that validation is deferred. To repeat the read-only check manually, run:

```sh
~/.local/bin/herdr-white-noise-bridge check-readers
```

The result reports the number of checked session readers. It does not print transcripts or change bridge state.

### At desk / Away forwarding

By default, a new native user prompt in OpenCode, Codex, or Claude Code mutes **all groups**. A new authorized owner message from your phone enables forwarding for all groups after the history checkpoint. Phone prompts are attributed through durable message IDs and prompt fingerprints, including across bridge restarts. Ambiguous or unconfirmed input is treated as local input and mutes forwarding. Historical input on first observation is only a baseline.

Detection uses transcript polling, not keystrokes. Allow the three-second polling interval plus transcript read time. Output already submitted to MDK cannot be recalled. Do not use automatic handoff as an instantaneous privacy boundary. Set `auto_forwarding = false` in the bridge configuration to disable local-prompt detection and retain the previous manual controls and phone wake behavior.

If a native message appears before its text parts, outgoing delivery pauses until attribution is possible. If that session ends with incomplete input, **codr** mutes globally instead; a new authorized phone message can resume the other sessions.

Use the **herdr** action **White Noise forwarding: At desk / Away** to open the control popup. **Toggle White Noise forwarding** switches modes directly and can be bound to a **herdr** shortcut.

**herdr** 0.9.1 does not list plugin actions in its main click menu. For quick access, add this to your **herdr** configuration:

```toml
[[keys.command]]
key = "prefix+f"
type = "plugin_action"
command = "local.white-noise.forwarding"
description = "White Noise: At desk / Away"
```

With the default prefix, press **Ctrl+B**, release it, then press **f** without Shift. The shortcut also appears in **Keybinds** under **White Noise: At desk / Away**. **herdr** reloads configuration changes live.

Remote **herdr** clients use their local key bindings by default. Add the binding on the client machine, or reconnect with `herdr --remote <host> --remote-keybindings server`. This does not require a server restart. You can also run `herdr plugin pane open --plugin local.white-noise --entrypoint forwarding --focus` on the remote host.

Startup no longer opens the popup automatically. Use the shortcut or action when you need manual control.

- **At desk:** suppress outgoing chat messages while the bridge continues to listen for your phone. New group invitations and chat-name/description updates remain active.
- **Away:** forward progress and final replies from every mapped session.
- In automatic mode, a new message from the configured phone to any mapped group switches **all sessions** to Away, except `/help`, `/recent`, sync/queue controls, `/cancel`, and `/new` launch controls. Already processed messages, your bridge's own messages, and other senders do not switch modes.

To resume forwarding without asking an agent to do work, send `unmute` or `/unmute` by itself in any linked White Noise chat. **codr** consumes the command and confirms in that chat after the normal history checkpoint. It works in busy, blocked, and ended conversations. It does not start a turn or answer a pending decision. Recognition ignores capitalization and surrounding whitespace; longer messages, attachment captions, voice transcripts, and truncated input are not commands. Only the configured phone owner can use it. Muted history is not replayed, and duplicate deliveries cannot unmute again after a later mute.

In the popup, enter **1** for At desk, **2** for Away, **3** to toggle, or **q** to close. Press Enter after your choice. Muting applies to every mapped session; it does not stop the transport or change your groups.

The selected mode survives restarts. Mode changes discard the automatic outgoing backlog, but preserve explicitly requested `/help` and `/recent` replies. Before Away becomes active, the bridge checkpoints replies for all active sessions. The popup shows **Away requested** while a transcript checkpoint is pending. Completed muted turns remain suppressed even if their transcript records arrive late. An operation already submitted to MDK can still arrive after you select At desk.

Phone sync overrides are persistent: `/sync on` means **Always on**, `/sync off` means **Always off**, and `/sync auto` restores normal automatic behavior. Local activity, phone wake-up, and the At desk/Away controls do not change a forced mode. `/unmute` clears it and requests automatic sync with all chats unmuted. `/help` and bare `/sync` show the current mode. Always off suppresses automatic outgoing messages but does not hold new phone text from reaching the agent; explicit control replies and `/recent` still work. Returning to an enabled state retains the history checkpoint before outgoing sync resumes.

The command-line controls work while the worker runs:

```sh
~/.local/bin/herdr-white-noise-bridge forwarding status
~/.local/bin/herdr-white-noise-bridge forwarding desk
~/.local/bin/herdr-white-noise-bridge forwarding away
~/.local/bin/herdr-white-noise-bridge forwarding toggle
```

Forwarding defaults to Away on an existing installation. The control file is private and separate from routing state, so changing modes does not stop either service.

### White Noise chat commands

Send these as standalone text from the configured owner's account in a linked chat:

| Command | Result |
| --- | --- |
| `/help` | List commands and the current sync mode in this chat only. |
| `/recent` | Get newest 3 of all active <1h: send up to three conversation text messages, labelled You or Agent with section breaks, to each chat active in the past hour. Does not resume sync. |
| `/sync auto\|on\|off` | Set a persistent sync mode. Bare `/sync` shows the current mode and choices. |
| `/status` | Show connection, agent state and pending delivery count. Resumes sync in automatic mode; a forced-off override remains off until `/unmute` or a new sync-mode choice. |
| `/queue` | Show this chat's waiting prompt count and in-flight or uncertain submission state without changing mute settings. |
| `/cancel` | Interrupt the current task in this chat through native herdr control, keeping history and accepted deliveries. Does not change sync mode. |
| `unmute` or `/unmute` | Clear a forced mode and resume automatic sync for all chats without prompting an agent. |

`/help` and `/recent` do not prompt agents, answer questions, create groups, or change sync mode. They work while muted or **herdr** is disconnected. Recently ended mapped conversations can participate in catch-up. The command chat receives a count of queued conversations and unavailable readers; a queued send does not prove phone receipt. Retrying the same inbound event does not resend the catch-up. A new `/recent` request skips unchanged source messages already confirmed sent or queued by the bridge, including previous catch-ups and ordinary assistant forwarding.

The cutoff is one hour before the `/recent` request and is fixed for the whole batch. For example, `/recent` at 16:00 selects chats active since 15:00. The last confirmed outgoing-message clock is retained for delivery accounting, but no longer extends this activity window.

Delivery accounting survives restarts and mode changes. Older installations without a confirmation clock recover the newest available own chat message from retained MDK history, paging past incoming messages when needed. Unavailable readers are reported without blocking other chats' excerpts. MDK confirmation does not prove receipt on the phone.

Conversation activity comes from source clocks, not polling: OpenCode message creation, streaming/update, and completion clocks, or the verified Codex/Claude session log's modification time. Histories with no known activity time, future activity times, no conversation text, or activity before the shared cutoff are skipped. A failed reader does not prevent successful readers from contributing.

The time window selects conversations, not individual excerpt messages. Each eligible conversation contributes its last three available user or completed assistant text messages in order, even if some context predates the cutoff. This is often Agent → You → Agent, but it follows the actual conversation and can end with an unanswered user prompt. Empty messages, attachment-only inputs, tool output, internal summaries, private subagent branches, and unfinished assistant replies are excluded. Each excerpt is grouped into one labelled catch-up message; very long excerpts use continuation messages. No model call is made. Sync does not change normal reply checkpoints or replay all missed messages. Catch-ups use durable, idempotent delivery across restarts.

Within that three-message excerpt, already delivered or pending messages are removed without backfilling older history. Delivery records are per chat and survive restarts. A source message is confirmed only after all of its batch's chunks receive valid MDK acknowledgements; failed or partial sends retain their queued retry keys. An identical text with a different source ID, or edited content under the same ID, is a new message version. Incoming phone messages are not themselves outgoing confirmations. Delivery tracking begins with this update: older sends without source records cannot be marked confirmed reliably and may appear once in the first catch-up after upgrade.

Recognition ignores capitalization and surrounding whitespace for standalone controls. Longer prose, attachment captions, voice transcripts, and truncated input cannot operate these controls. Use a listed number, an exact option label, or `/answer 2` for a supported pending decision; include the question code when its card requests one.

### Immediate text and persistent pending deliveries

An ordinary complete owner text prompt goes directly to the freshly verified mapped agent, including while it is working. It does not wait in codr for idle or interrupt the current task. The agent may still use its own native inbox. A blocked native dialog is reported without silently making a future task; answer its supported decision or permission request, or use `/cancel`. Duplicate inbound events do not submit again. Phone text may reach the agent before an older media item finishes preparation.

Media and the optional desktop endpoint retain bounded pending delivery: 16 waiting items per chat and 30000 UTF-8 bytes per queued text/caption or prepared image prompt. A full queue or oversized waiting prompt gets a rejection notice, not silent eviction. An image message occupies one slot, keeps its caption, and accepts up to four PNG, JPEG, WebP, or GIF attachments, with the existing 25 MiB-per-file download limit. Voice handling is unchanged. New phone text has no 16-task waiting queue.

Images are prepared when their retained FIFO item reaches an idle agent. References and private prepared paths survive worker restarts. A processing failure reports that no image prompt was sent and releases the failed item. Missing offline download references remain pending for recovery. The retired `/clear-queue` command returns a retirement notice and does not delete accepted images or text.

Previously accepted text, media and desktop deliveries run independently per chat using the existing guarded dispatch path. They still require the same native session, an idle readiness check, enabled sync and a supported transcript reader. An early idle event alone is not completion. Muting pauses these retained deliveries without discarding them; ending a session never reroutes them to another session. This retained path does not hold new phone text.

The bridge saves consumption and submission intent before native input. A proven pre-submission socket failure retains the message for retry without clearing a running task. A lost acknowledgement, uncertain result, or restart during submission is never automatically repeated. Inspect the terminal, then use local `clear-prompt` to release a retained-delivery hold; it does not resubmit the uncertain task. `/queue` inspects pending state while muted or disconnected. `/cancel` interrupts only a freshly verified active task, persists its receipt before submission, and never repeats an uncertain interrupt automatically.

### Editable progress and separate final replies

Set `edit_responses = true` in the bridge configuration to enable editable progress. The release-matched connector includes the required `edit_message` API.

Each response starts as one `> working...` progress message. The bridge edits that message with the current task list, when available. It sends the final answer as a separate message through the normal durable send path, with its own retry key. The bridge never edits that final answer. When possible, it marks the progress message `> done`.

When an agent resumes from a blocked decision, or a phone follow-up is delivered while it is working, the bridge also posts a fresh `> working...` notice below newer chat messages. Unsent notices coalesce. Direct text delivery does not claim a queue position; retained media still acknowledges `Queued prompt at position N.`. This is not a heartbeat: polling and restarts do not repeat the notice. Refreshes are best effort, follow durable answers, and are discarded when the agent stops working.

OpenCode to-do writes, Codex plan updates, and Claude Code to-do/task tools supply structured task data. The bridge does not prompt a model to obtain progress. Message IDs and pending updates survive bridge restarts. Updates coalesce between snapshots, and edit attempts are at least two timestamp seconds apart.

Progress is best effort. Final replies take priority over progress updates, and progress requests have a three-second local deadline. A failed progress edit does not block a queued final answer. An MDK response or local readback does not prove phone delivery. Very long final replies use continuation messages to preserve all text. Progress messages are ordinary encrypted chat messages, not Nostr ephemeral events.

If **herdr** becomes idle before the agent stores its final reply, the bridge keeps the `working...` progress bubble and its checklist, continues reading task updates, and retries the final transcript read. It marks progress done only when the completed reply is collected. After inspection, `clear-prompt` can release a cancelled response that has no final record.

OpenCode 2 disables todos by default. To restore native task updates, add `"experimental": { "todo": true }` to your global `~/.config/opencode/opencode.json(c)`. This makes the `todowrite` tool available; the checklist appears when the agent uses it. **codr** does not invent a task list when the agent supplies none.

Edit support is included in the dedicated connector; the flag controls whether **codr** uses it.

### Answer decisions from the phone

When OpenCode asks one single-choice question, **codr** sends a clean question card with numbered options. Reply with **1**, **2**, or an exact option label. The answer goes to the active question, even while **herdr** reports the agent as blocked; it is not submitted as a new chat prompt.

Decision cards and answers do not require editable progress.

OpenCode V2 exports omit running questions. The bridge reads V2's live session forms through `opencode api` instead, using the CLI's existing service discovery and authentication. It still checks the exact visible question before submitting a choice. Completed forms disappear from this read and cannot be replayed as pending questions.

The bridge checks the session, pending tool request, and visible question before sending the native number shortcut. If a question was not answered through **codr** and is replaced, the new card requires its short code, for example `qabc123 1`. Duplicate deliveries and uncertain submissions are not sent again automatically.

Decision routing was verified against OpenCode 1.18.35's single-choice question UI with 1–9 listed options. The gallery uses OpenCode 1.19.3, but does not verify live decision routing on that version. Custom text entry, multi-select/multi-question dialogs, and decisions in other backends require the terminal. Unsupported OpenCode blockers show a short notice instead of forwarding unrelated tool output.

OpenCode V2 tool permissions can show a separate permission card. The configured phone owner can reply with `/allow CODE` for **Allow once**, or `/reject CODE`. Use the code from that card in the same chat. **Always allow** remains terminal-only. The bridge checks the session and the exact pending request before sending an API reply; it never sends terminal keys or changes persistent permission rules. Replies bypass the task queue and do not resume muted forwarding. An uncertain reply is not retried, including after restart; check the terminal instead. Multiple pending permissions, oversized requests, unsafe display text, and unavailable permission APIs require the terminal.

**herdr** 0.9.1 cannot make the final identity check and terminal input atomic. Avoid answering the same question simultaneously from the terminal and phone. After an uncertain answer, inspect the terminal before using `clear-prompt` and sending a new reply.

## Shared chats and owner-only control

Group members can discuss a project and read the agent's replies. Only messages from the phone public key in **codr**'s configuration become agent prompts. Other participants' messages are not added to the agent's context. The owner can relay a suggestion in a new prompt.

New groups make both the configured phone owner and the bridge identity admins in the founding group state. The owner can invite participants and edit the group profile as soon as the invitation is accepted. Admin rights do not grant other participants permission to prompt the agent.

This requires the release-matched connector included with **codr**. The bridge checks support before recording creation intent, then uses an explicit owner-admin creation operation. An older connector cannot silently create a group without the requested rights. Existing groups keep their current admin policy.

Renaming a White Noise group does not affect routing: **codr** uses its group ID, not its display name. A title changed outside **codr** is left alone; running `rename-groups` explicitly resets titles to the **herdr** tab labels.

The gallery uses an owner-created group, bound to an isolated demo session. Alex creates the group and invites Sam through White Noise. Sam is a scripted second demo identity. Its message appears in the chat but produces no agent prompt. Alex's next message produces one prompt and a separate reply. This does not grant additional people agent control.

## Images and voice messages

### Images from the agent

In local development builds, **codr** attaches explicit Markdown images from new completed agent replies, such as `![Screenshot](screenshots/current.png)`. Relative paths use the mapped agent's working directory. Absolute paths must remain beneath that directory or `/tmp/opencode`. Symlinks, parent traversal, remote URLs, SVG, and non-image files are not uploaded. Code examples and ordinary download links remain text. To send both dark and light screenshots, embed both with `![label](path)` rather than linking the second with `[label](path)`.

Image formats are identified from their file signatures. Limits are four images, 10 MiB per image, and 25 MiB in total per reply. Missing, blocked, unsupported, or oversized images are replaced with an `Image not attached` notice; the rest of the reply still forwards. Existing mute and history-baseline rules apply. Old replies and `/recent` excerpts do not trigger image uploads.

Before delivery, **codr** snapshots each image into a private `state.outbound-media` directory beside `state.json`. MDK encrypts the snapshot before upload. The durable outbox retains the snapshot path and delivery ID across retries and restarts. Confirmed text is not resent if its image is still pending. Snapshots are removed after a successful delivery receipt is saved, or after forwarding cancellation is saved and no retained outbox item references the image. Interrupted preparation can still leave private snapshots for manual cleanup while the worker is stopped.

The connector must start with `--media-allowed-root /absolute/path/to/state.outbound-media`. Allow **only this private spool**, not the repository, home directory, or `/tmp`. Fresh setup creates and configures it. For an existing installation, inspect its effective service command and add this flag while preserving its binary, account home, socket, relays, and other arguments. Custom service overrides still require manual inspection. The directory must exist with mode `0700` before the connector starts. No identity or storage migration is needed.

### Images and voice from the phone

Images and audio are available in **codr** 0.5.0. Voice prompts require the local transcription worker described below.

**codr** asks MDK's `download_media` API to fetch and decrypt each attachment. Keys stay in MDK. PNG, JPEG, WebP, and GIF images are copied into a private `media` directory beside bridge state. The prompt includes the local file paths and the original caption. The agent needs its normal local-file access to inspect them. Files remain there for later conversation access until you remove them.

Audio goes through FFmpeg to 16 kHz mono WAV, then to a local Phonon-2 worker. Its transcript enters the same prompt route as typed text, without a confirmation step. Supported question answers also use that route. Download and local transcription add time before the agent starts.

The phone sees `> transcribing` while audio processing runs. With `edit_responses = true` and an edit-capable MDK build, **codr** edits that bubble to `> transcribed` with the transcript quoted underneath. The edit survives restarts and retries independently of agent submission. Without edit support, the transcript arrives as a separate message. Images use `> downloading`. Long transcript displays are shortened with a notice; the agent receives the full prepared prompt.

Downloads and transcription run separately from session updates, one media message at a time. Pending messages survive bridge restarts. Sender checks and session checks still apply. A preparation failure sends no agent prompt. An uncertain terminal submission is not repeated automatically.

Offline media recovery uses `timeline_message_get` with opt-in `include_media` download references. The release-matched connector includes this extension. Older connectors return only attachment summaries. **codr** defers those without consuming the message and can accept a complete live event later.

Limits are four attachments per message, 25 MiB per file, and five minutes per audio file. Combined captions, transcripts, and image paths must fit within 30,000 UTF-8 bytes. Unsupported files, truncated messages, and empty transcripts do not reach the agent.

### Local speech worker

Setup installs `fermion-research` 0.2.12 with CPU-only PyTorch and downloads Phonon-2, approximately 164 MB plus runtime dependencies. FFmpeg is required. The worker uses an owner-only Unix socket under `$XDG_RUNTIME_DIR/codr-phonon` and processes recordings locally.

On Linux, `codr-phonon.service` has a 1 GiB memory limit and no swap allowance, and the bridge starts it as a dependency. launchd has no equivalent cap, so the macOS agent runs without a hard memory limit. A healthy existing **codr** worker is reused. If the worker cannot start within the Linux limit, setup reports failure rather than raising the limit.

Fresh setup includes audio by default. Use `--no-audio` for a text/image-only installation. To add Phonon to an existing installation:

```sh
python3 scripts/setup.py --phonon
```

This preserves the identity and mappings, adds the transcription command, and restarts only the bridge when its configuration changes. Existing custom transcription commands are not overwritten. A plain setup rerun preserves the existing selection.

```sh
# Linux
systemctl --user status codr-phonon.service
journalctl --user -u codr-phonon.service
# macOS
launchctl print "gui/$(id -u)/io.github.tidley.codr.codr-phonon"
tail -n 50 ~/Library/Logs/codr/codr-phonon.err.log
```

Normalised audio files are temporary and removed after each attempt. MDK manages its downloaded temporary files. Image copies stay in **codr**'s private media directory. Do not commit media, transcripts, or model files to Git.

## Supported reply sources

| Agent | Final-reply source | Default transcript location |
| --- | --- | --- |
| OpenCode | Completed text from `opencode --pure export SESSION_ID` | Managed by OpenCode |
| Codex | Successful `task_complete` / `turn_complete` events with a turn ID and final message | `~/.codex/sessions` and `~/.codex/archived_sessions` |
| Claude Code | Completed main-conversation assistant responses with `message.stop_reason = "end_turn"` | `~/.claude/projects/<project>/<session-id>.jsonl` |

The bridge reads the existing session. It does not start another model invocation, change authentication, or use terminal snapshots as final replies. **herdr** must report the native session ID.

Codex and Claude Code readers verify session identity, ignore partial records, and reject ambiguous transcript files. Claude's null stop reasons and duration markers alone are not treated as successful replies. Transcript formats are internal to the agent tools; unsupported or incomplete records are not guessed into final answers.

For custom profiles, set absolute `codex_home` and `claude_home` paths in the bridge configuration. Defaults use `CODEX_HOME` / `CLAUDE_CONFIG_DIR` when available, otherwise the locations above. Fresh setup records these paths explicitly.

Existing history is baselined when a group is first attached or reply support is first enabled for an older group. It is not reposted. Later completed replies use persistent message IDs to avoid duplicates after restart.

## Roadmap

- [x] Add structured final replies for existing Codex sessions.
- [x] Add structured final replies for existing Claude Code sessions.
- [ ] Add structured final replies for further **herdr**-supported agents.
- [ ] Create new agent conversations from White Noise by messaging the main worker, which starts a **herdr** session and creates its mapped group.
  - Select the **herdr** workspace, directory, agent, and model.
- [ ] Let the owner authorise additional people to prompt the same agent in a White Noise group.
  - Add owner-managed prompt permissions per group, including revocation.
  - Distinguish agent-directed requests from discussion between participants.
- [x] Make the configured phone owner an admin automatically when creating new groups.
- [x] Download inbound images through MDK and pass their local paths and captions to the mapped agent.
- [x] Convert White Noise voice messages and audio attachments directly into text prompts with a local Phonon-2 worker.
  - [ ] Add guided selection of other existing transcription services or local whisper.cpp.
  - Review the existing Rust audio preprocessing and configurable transcription pipeline in [Nostr Codex Phone](https://github.com/tidley/nostr-codex-phone/blob/main/rust/src/transcribe.rs), which already supports whisper.cpp.
  - Evaluate [whisper.cpp](https://github.com/ggml-org/whisper.cpp) and Phonon-2, as used in [Phonon Wayland Dictation](https://github.com/tidley/phonon-wayland-dictation).

## Requirements

- Linux x86_64 or aarch64 with a systemd user manager, or macOS on Apple silicon or Intel with a logged-in launchd session.
- Python 3.11 or newer for setup, Git, and `launchctl` or `systemctl`. Setup finds a newer Homebrew interpreter when `/usr/bin/python3` is too old. Rust is only needed when building from source.
- **herdr** 0.9.1 or newer, with a running local server.
- The release-matched dedicated connector, including `group_create_with_admins` and `timeline_list` in `marmot.agent-control.v2`.
- A supported agent: OpenCode, Codex, or Claude Code. OpenCode needs `--pure export SESSION_ID`; Codex and Claude Code need readable native transcripts.
- **herdr** native session references for the agents you want to control.
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

Use the [installation guide](INSTALL.md) for complete commands and checksum verification.

Install a pinned release through **herdr**, then open its guided setup tab:

```sh
herdr plugin install tidley/codr --ref v0.6.0
herdr plugin action invoke local.white-noise.setup
```

The installation hook downloads and verifies the matching **codr** bridge and connector from GitHub Releases. Setup asks for your public `npub` and configures dedicated user services. No Rust toolchain is required for this path.

For a non-interactive installation by a coding agent, see the [agent instructions](INSTALL.md#instructions-for-coding-agents).

**herdr** starts the plugin worker through its startup hook after restore. The worker's user service keeps it alive across client disconnects. On Linux, use `systemctl --user enable herdr-white-noise-bridge.service` if it must start independently at login. On macOS the LaunchAgents in `~/Library/LaunchAgents` run at login by default.

Do not share the existing `wn-opencode` connector home. That adapter can also answer messages sent to its identity.

## GitHub builds

GitHub is primary for source, issues, pull requests, reviews, CI, and releases. Contributors do not need Nostr or ngit.
The ngit repository mirrors approved source refs and published release files. Blossom hosts the exact, unchanged release assets.
See [source and release mirror verification](MIRRORING.md). Existing ngit discussions remain available without automatic duplication or closure.

GitHub Actions runs Rust formatting, tests, Clippy, and isolated installer/setup tests on Linux, plus the Rust and Python test suites on macOS, for pushes and pull requests. Version tags build separate bridge and connector archives for Linux and macOS on x86_64 and aarch64. Linux bridges use musl; Linux connectors use native glibc. macOS binaries are ad-hoc signed. Archives and SHA-256 checksums are published to [GitHub Releases](https://github.com/tidley/codr/releases). See the [connector requirements](INSTALL.md#requirements).

The installer selects the bridge and connector release from the plugin checkout. It never silently substitutes a newer release. The dedicated connector is built from the pinned upstream MDK revision and published patch in [connector source](../patches/README.md). Each connector archive includes source provenance and the upstream license.

## Verify on the phone

1. Accept an invitation from the verified bridge identity.
2. Send `/status` and check that it identifies a connected session.
3. Send a harmless request with a fresh word. Ask for that word without tools or file changes.
4. Check that the matching **herdr** session receives the prompt.
5. Check that its completed reply returns to the same White Noise group.

A relay acknowledgement proves publication, not that the phone displayed or accepted a message. Phone verification requires the round trip above.

## Recovery and diagnostics

```sh
~/.local/bin/herdr-white-noise-bridge status
# Linux
journalctl --user -u herdr-white-noise-bridge.service
# macOS
tail -n 50 ~/Library/Logs/codr/herdr-white-noise-bridge.err.log
```

State and the pending outbox use owner-only files. Keep them on persistent storage, with the MDK account home. Do not put these files in Git.

The bridge saves consumed phone-message IDs before it submits terminal input. It never retries an uncertain prompt automatically. This favours avoiding duplicate actions over automatic replay after a crash.

If a prompt result remains uncertain, inspect the live session first. Stop the worker and release the pending flag without resubmitting:

```sh
# Linux
systemctl --user stop herdr-white-noise-bridge.service
~/.local/bin/herdr-white-noise-bridge clear-prompt 'SESSION_KEY_FROM_STATUS'
systemctl --user start herdr-white-noise-bridge.service
# macOS
launchctl bootout "gui/$(id -u)/io.github.tidley.codr.herdr-white-noise-bridge"
~/.local/bin/herdr-white-noise-bridge clear-prompt 'SESSION_KEY_FROM_STATUS'
launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/io.github.tidley.codr.herdr-white-noise-bridge.plist"
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

### **herdr** input limitation

**herdr** 0.9.1 does not accept an expected native-session ID in `agent.prompt`. The bridge verifies the current terminal and session immediately before submission. A concurrent session replacement between verification and submission remains a server-API race. Avoid replacing an agent while sending a phone prompt. Atomic protection requires an upstream expected-session guard.

## Group names and completion notices

To update existing group names without changing their session mappings, stop the worker and run:

```sh
~/.local/bin/herdr-white-noise-bridge rename-groups
```

Then start the worker again. This refreshes names and descriptions without changing group IDs, clears a manual-rename lock, and enables automatic renames for groups created by older versions. Ended sessions without a stored tab label use their existing title, replacing earlier **codr** prefixes or suffixes. Unavailable historical workspace details appear as `unknown`.

Live sessions automatically update their group title and description when the **herdr** tab or workspace changes. A title changed in White Noise is never overwritten automatically. If a profile update is not confirmed, automatic renames for that group stop; run `rename-groups` to re-apply the current titles and clear that state. Sessions in tabs with identical labels and workspaces can share a display name, while their routing remains separate.

### Completion notice

Stop the worker before using its state-changing CLI. To send a notice to one mapped group:

```sh
~/.local/bin/herdr-white-noise-bridge notify --group GROUP_HEX 'The bridge is ready.'
```

Without `--group`, the command sends the notice to every mapped group. The command returns success only after MDK confirms publication.

## Screenshot provenance

### 0.7.0 controls

The three `codr-v070-*-chat.png` images in the [README gallery](../README.md#screenshots) are crops of the unmodified native White Noise Linux interface. Fresh fictional accounts ran in network-disabled containers, using offline Notes to self chats and supported fixture-injection hooks. They show help, workspace selection, and model-provider selection, including literal **0. Cancel** choices.

The images preserve native pixels; no text or overlays were added. Crops retain the chat title and complete messages. PNGs contain only image chunks, with no text or EXIF metadata. No existing accounts, private conversations, physical-device captures, or credentials were used. These are visual examples, not evidence of live encrypted routing, cancellation, model selection, or model execution. The displayed provider list is a fixture, not a list of bundled accounts or credentials.

Published SHA-256 checksums:

```text
3c04e8821cb27d8ed664610bdb097b0b811fab8f5901e2454194aa873b5cead1  codr-v070-help-chat.png
da336f940a28b856440a7fed5ce3629efb07b232504780b375906c039568b33e  codr-v070-model-providers-chat.png
5499d5c4bfd0316cb5a57f6b4c46cd88312d3327897020b21df7c615369dfc8d  codr-v070-workspaces-chat.png
```

### Earlier end-to-end examples

The earlier gallery contains side-by-side composites of real **herdr** 0.9.1, OpenCode 1.19.3, and White Noise Android interfaces.

All seven images use the same fictional Garden planner project. The desktop replays matching history imported into an isolated OpenCode installation. The phone uses a dedicated Alex demo profile. A synthetic **herdr**/OpenCode fixture supplies agent state and scripted replies through the real **codr** bridge and an isolated MDK transport.

The captures show the previous `[cd]` name prefix, blockquote status labels, editable tasks, decisions, separate replies, images, local voice transcription, a three-member chat, and a White Noise project folder. The folder is configured manually, not created by **codr**.

The reminder design is a generated demo image. The voice clip uses synthetic speech and passes through the real Phonon-2 worker. Its actual transcript reaches the fixture as the prompt, without confirmation. Sam uses a separate MDK identity. Alex's invitation and both participants' chat messages use real encrypted group messaging. Verification checks that Sam's message causes zero agent prompts and Alex's message causes exactly one.

The phone's `1` reply continues the question fixture, not a live OpenCode dialog. The captures do not verify a live coding-model run, a real project change, or a live decision dialog end to end. Editable task captures use an MDK build with edit support. Android status and navigation bars are cropped. The images contain no private conversations or text/EXIF metadata.

## Acknowledgements

**codr** builds on the work of these projects and their maintainers and contributors:

- White Noise: the mobile messaging application used as **codr**'s phone interface, maintained in the [iOS](https://github.com/marmot-protocol/whitenoise-ios) and [Android](https://github.com/marmot-protocol/whitenoise-android) repositories.
- [Marmot Development Kit (MDK)](https://github.com/marmot-protocol/mdk): the `wn-agent` connector that provides identity management, encrypted group state, relay transport, invitations, and message delivery.
- [Marmot protocol](https://github.com/marmot-protocol/marmot): the end-to-end encrypted group messaging protocol built on MLS with Nostr identity.
- [**herdr**](https://github.com/herdrdev/herdr): the agent runtime, plugin system, and local API that **codr** uses to discover and control sessions.
- [OpenCode](https://github.com/anomalyco/opencode): the coding-agent runtime whose stored final replies **codr** currently reads.
- [Codex](https://github.com/openai/codex): the coding-agent runtime and native rollout records used by **codr**'s Codex reply reader.
- [Claude Code](https://github.com/anthropics/claude-code): the coding-agent runtime whose native session transcripts **codr** reads for completed main-conversation replies.
- [Phonon-2](https://github.com/fermionresearch/phonon): the local speech model and Fermion runtime used for audio transcription. Its model weights and runtime remain upstream software.

**codr** provides the routing bridge between these systems. The messaging protocol, encryption, transport, mobile app, and agent runtimes come from the upstream projects above.
