# Changelog

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

- Pending and prepared media survive restarts and temporary HERDR outages without duplicate prompt submission.
- Deferred media in one session does not block preparation for other chats.
- Transcript reads preserve session identity and forwarding-mode checks. Exiting sessions get a fresh final read.
- Transcription notice delivery retries independently of agent submission, including lost send or edit responses.

### Compatibility

- Same-bubble transcripts and editable progress require `edit_responses = true` and an MDK build with `edit_message` support.
- Offline media recovery needs MDK's opt-in `include_media` download references. These extensions are not in published MDK 0.13.0 binaries.
- Incremental database reads were verified against OpenCode 1.19.3's v1 projections and durable event log. The CLI reader remains available when `opencode_db` is unset.

[0.5.0]: https://github.com/tidley/codr/compare/v0.4.0...v0.5.0
