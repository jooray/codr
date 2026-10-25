# Changelog

## [0.7.0] - 2026-10-24

### Added

- Bare `/new` now first offers an existing **herdr** workspace or a new one. Existing destinations receive a new project tab without changing their current sessions, chats, queues, or workspace labels. Workspace menus are paginated and restart-safe; final confirmation includes the destination label and stable ID. Removed or unavailable workspaces stop launch, while renamed workspaces require a new confirmation. No fallback workspace is created automatically.
- Guided `/new` project setup asks for a folder, installed agent, and model before confirmation. Chat-bound menus survive restarts, offer a default or provider categories, and list each provider's models and variants in full. Existing chats gain `/agent` for a separate session and `/model` for confirmed, idle-only OpenCode V2 model changes. Codex/Claude model choices start separate sessions without moving old history or deliveries. Guided OpenCode launches create the selected native session before attaching the terminal.
- Owner-only `/new NAME FOLDER AGENT` previews a workspace launch under `launch_root` (default `~/code`). Both existing and missing folders require a chat-bound confirmation before creating folders or starting OpenCode, Codex, or Claude Code. Requests survive restarts; uncertain startup is never retried automatically.
- Owner image messages retain captions in a persistent per-chat media FIFO. Each image message occupies one of 16 waiting slots. Images are prepared when their item reaches an idle agent, and private prepared paths survive restarts. `/queue` includes these pending deliveries. Voice handling is unchanged.
- Send explicitly embedded local Markdown images in new completed agent replies as encrypted White Noise attachments. Source files must be regular, non-symlink PNG, JPEG, WebP, or GIF files beneath the mapped working directory or `/tmp/opencode`. Private snapshots and stable delivery IDs preserve retry bytes across restarts. Ordinary links stay text; unavailable or blocked images get a visible notice. Limits are four images, 10 MiB per image, and 25 MiB per reply. The connector must allow the private outbound-media spool; fresh setup configures it.
- Owner-only `/cancel` interrupts the current task through herdr without deleting history, stopping workers, or clearing accepted deliveries. Native session identity is checked immediately before submission, and uncertain interrupts are never automatically retried.
- Persistent `/sync auto|on|off` modes, with the current mode displayed in help. Forced modes survive restarts and ignore automatic mute/wake changes; `/unmute` clears the override and restores automatic sync.
- Owner-only OpenCode permission replies with `/allow CODE` and `/reject CODE`. Codes are bound to the pending request and checked against the live session before submission.
- An opt-in, same-user local endpoint for the separate **portr** app to list sessions, read bounded recent history and submit persistent desktop deliveries. Request IDs prevent duplicate enqueueing. SSH can carry requests without exporting the bridge's Nostr identity; codr does not install SSH, a VPN, or the desktop app.

### Changed

- Phone text is handed directly to the mapped native agent while it is working, without interrupting or waiting for idle. Existing accepted deliveries remain intact. Multiple follow-ups preserve response tracking and native-input attribution across staggered transcript updates and restarts.
- Rename catch-up `/sync` to `/recent`, with the help description `Get newest 3 of all active <1h`. Its activity window is now the hour before the request, not an hour before the last global confirmed send. Existing source-message deduplication remains intact.
- Retire `/clear-queue` from supported commands and help. Old standalone invocations return a retirement notice without deleting accepted work. `/queue` remains a pending-delivery inspection command.
- Model menus now offer default/provider selection first and show a provider's full available model list without pagination. Variants are also listed in full; long lists use the existing durable message splitting. Help uses bold section headings with each command's description on the next line, matching the updated phone layout.
- Use bracketed four-step setup headings and agent names **OpenCode**, **Codex**, and **Claude Code**. Group large model catalogues by provider/free pricing, then list base models and variants separately instead of expanding every variant into one long list. Confirm a launch by typing the displayed workspace name within ten minutes; zero-cancel and explicit command compatibility remain available.
- Tidy project menus place numbered **Create new workspace** after existing workspaces, accept plain folder names and offer **0. Cancel** without clearing uncertain operations. Special choices render separately from ordered Markdown lists, so WhiteNoise cannot renumber cancellation or paginated workspace creation.
- Project setup menus accept plain numbered replies, with no long code to copy. Active menu choices are routed before agent decisions. Folder commands and explicit launch/model confirmations also work without a code; existing coded commands, expiry, sender checks, and final validation remain supported.
- Fresh setup gives newly created bridge accounts the public display name **Pat**. New session chats introduce Pat and point to `/help`. Existing identities, custom profiles, and chats remain unchanged; unconfirmed profile publication is reported without automatic retries.
- Make the **portr** desktop/SSH endpoint opt-in instead of starting it with every worker. Fresh setup defaults to phone-only access and offers an optional desktop choice; non-interactive setup uses `--portr`. Existing explicit preferences are preserved, and a changed preference is saved for an idle worker restart rather than triggering an extra restart. `run --portr` and `run --no-portr` override the configuration; earlier `--porter` spellings remain aliases. The desktop app stays separate, and codr does not install SSH or VPN services.
- `/help` uses Markdown headings, bold inline-code commands and descriptions on the next line. `/status` explicitly describes unmuting, and help shows the current persistent sync mode. Catch-up excerpts have section breaks between their existing You/Agent labels.
- Fresh sessions launched from an existing chat keep the source tab label when available and distinguish their new chat with an agent suffix inside the workspace parentheses, for example `review (Acme - codex)`. The original chat is unchanged. Suffixes survive restarts and renames and are preserved within the title byte limit.
- Show a fresh `> working...` notice when an agent resumes from a blocked decision or a phone follow-up is delivered while it is working. Pending notices coalesce; polling and restarts do not repeat them. Refreshes are best effort and follow durable answers.
- White Noise chat names now match the **herdr** tab and workspace as `review (Acme)`, without the `[cd]` prefix. Existing managed chats use the same format after a profile update or `rename-groups`.
- Retained media acknowledgements say only `Queued prompt at position N.`, including position 1; immediate text no longer receives a queue acknowledgement.
- `/recent` skips unchanged source messages already confirmed sent or pending in each chat, including previous catch-ups and ordinary replies. Per-source delivery records survive restarts and confirm only after every message chunk succeeds. Excerpts preserve conversation order, can include older context or unanswered user prompts, and exclude tool output and unfinished replies.

### Added

- Add `scripts/transcribe_gemini.py`, an optional voice transcriber that uses the Gemini API. It sends the voice audio to Google. The API key is read from `~/.config/codr/gemini-api-key`.

### Fixed

- Clear expired project menus after one expiry notice so later phone messages return to normal agent routing. Preserve uncertain actions and retry safely after a failed state save.
- Keep control replies after owner inputs across whole-second timestamp ties and small clock differences. Persist the per-chat send deadline across restarts. Do not delay native agent input or replies in other chats.
- Keep stable executable paths across Homebrew upgrades and report missing OpenCode executables without exposing transcript data.
- Preserve optional OpenCode support for Claude-only and Codex-only installations, including repeat setup and check mode. Bare commands retain PATH lookup.
- Validate replacement session readers before saving an OpenCode repair. Restore the old configuration after a failed worker restart so setup can retry the repair.
- Read Claude transcripts larger than 128 MiB and parse only appended bytes, retaining the bounded 32-entry transcript cache and eviction order.
- OpenCode V2 decision replies no longer reject a matching live question when the sidebar shows a repository-path footer instead of the older version footer. The bridge still checks the current request identity, exact question and options, visible selection controls, and unchanged terminal session before sending a choice.

## [0.6.2] - 2026-10-23 (Linux preview)

### Added

- Owner-only `/sync` sends the latest completed assistant reply from conversations active since one hour before the last confirmed message across all linked White Noise chats, each to its own chat. The global reference persists across restarts and is recovered from MDK history on older installations; no-history installs fall back to the past hour. Recently ended mappings remain eligible, and mute settings and normal reply checkpoints stay unchanged.
- Owner-only `/help` lists White Noise commands and decision-reply syntax in the requesting chat. Both commands work while muted, never prompt agents, and retain durable replay-safe replies across bridge restarts.

### Changed

- Build the dedicated connector from MDK 0.14.0 (`3fb4b4759ac676b0c21c4c3527074866c9fa4c89`), retaining the founding-admin and media-recovery extensions. Linux previews rebuild this connector instead of reusing 0.13 assets.
- MDK 0.14.0 rekeys account databases and migrates storage on first open. Keep a device-local backup of the complete connector home and secret-store credentials before upgrading; downgrade to 0.13 is unsupported.

### Fixed

- Muting suppresses chat messages, not group invitations or automatic chat-name and description updates. Metadata updates do not wait for transcript reads.
- Introduction messages use bracketed lowercase agent labels, such as `[oc] Casual greeting`, instead of legacy labels such as `OC | Casual greeting`.

## [0.6.1] - 2026-10-23 (Linux preview)

### Added

- Send `unmute` or `/unmute` in any linked White Noise chat to resume forwarding for all chats without sending agent input. Owner checks, history checkpoints, and duplicate-message protection still apply.
- Opt-in Linux-only prerelease builds for x86-64 and ARM64, with verified reuse of the unchanged dedicated connector. Stable macOS releases remain separate.

### Fixed

- Recover pending OpenCode V2 question cards from live session forms. V2 transcript exports omit running questions. Existing session, choice, and visible-terminal checks still apply before an answer is sent.
- Keep the working checklist visible while a final reply is being saved, send completed replies separately, and preserve checklists from both working-label formats.

## [0.6.0] - 2026-10-22

### Added

- Automatically mute all groups on local native prompts and resume on authorized phone input. Preserve manual controls without opening a startup popup.
- macOS support for the Rust bridge, the MDK transport, and the local Phonon-2 worker.
- launchd LaunchAgents on macOS, with the same restart-on-failure behaviour as the systemd units.
- Native macOS aarch64 and x86_64 release binaries, ad-hoc signed, alongside the existing musl builds.
- Setup finds a compatible interpreter for Phonon, including Python 3.10–3.12 on Intel macOS.
- Phonon installs its MLX runtime on Apple silicon and the CPU runtime on Intel Macs.
- New groups grant admin rights to the configured phone owner and bridge in the founding state, including the initial Welcome.
- Release-matched connector binaries, built from a pinned MDK revision and the published founding-admin/media-recovery patch.
- Managed connector upgrades that preserve account state, group mappings, and transport arguments. Custom overrides require inspection.

### Changed

- Service supervision moved behind one interface shared by systemd and launchd.
- The install hook and setup tolerate macOS's Python 3.9 when only stdlib parsing is needed.
- Group names include the workspace, with UTF-8 byte limits.
- OpenCode session exports support both 1.x and 2.x formats.
- Live group titles follow **herdr** label changes, with durable rename intent and protection for manually renamed groups.
- Session readers select CLI export when an OpenCode database history format becomes incompatible, including upgrades while the worker runs. Storage errors still defer reads.
- Setup checks live transcript readers before changing services or identities. `check-readers` repeats that read-only validation without printing transcripts.

### Known limits

- Automatic handoff observes native transcripts after polling and read latency. It cannot recall output already submitted to MDK. Set `auto_forwarding = false` to retain manual local-activity control.
- Phone prompts are not queued behind busy or blocked agents. Wait for idle and resend; supported decision answers remain available while blocked.
- launchd has no memory cap, so the 1 GiB Phonon limit applies only on Linux.
- macOS setup needs a logged-in launchd session; a headless SSH session cannot bootstrap the agents.

### Compatibility

- Upgrade the bridge and dedicated connector together through setup. Shared `wn-agent` installations are not replaced.
- Existing groups retain their current admins. Group names do not affect routing, and additional admins do not receive agent-prompt permission.
- On Linux, the dedicated connector requires glibc 2.35 or newer; its OpenSSL dependency is bundled.

## [0.5.0] - 2026-10-22

### Added

- Image attachments with captions and private local files for the mapped agent.
- Local Phonon-2 voice transcription, submitted directly as an agent prompt.
- A quoted transcript beneath `transcribed`, edited into the original `transcribing` bubble when MDK supports edits.
- Optional incremental, read-only OpenCode SQLite reads through `opencode_db`. Unchanged sessions fetch no message or part bodies.
- Background transcript reads, with up to four readers and one outstanding read per session.

### Changed

- Chat names use `[cd] session`. Descriptions contain workspace and agent details.
- Status notices use blockquotes. Task lists appear below the status without a `Tasks` heading.
- README screenshots cover images, voice, decisions, and shared chats with owner-only agent control.

### Fixed

- Pending and prepared media survive restarts and temporary **herdr** outages without duplicate prompt submission.
- Deferred media in one session does not block preparation for other chats.
- Transcript reads preserve session identity and forwarding-mode checks. Exiting sessions get a fresh final read.
- Transcription notice delivery retries independently of agent submission, including lost send or edit responses.

### Compatibility

- Same-bubble transcripts and editable progress require `edit_responses = true` and an MDK build with `edit_message` support.
- Offline media recovery needs MDK's opt-in `include_media` download references. These extensions are not in published MDK 0.13.0 binaries.
- Incremental database reads were verified against OpenCode 1.19.3's v1 projections and durable event log. The CLI reader remains available when `opencode_db` is unset.

[0.6.0]: https://github.com/tidley/codr/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/tidley/codr/compare/v0.4.0...v0.5.0
