# **codr**

**Source, issues, pull requests, CI, and releases:** [GitHub](https://github.com/tidley/codr).
Contributors do not need Nostr or ngit. [Contributing](CONTRIBUTING.md)

[ngit](https://gitworkshop.dev/npub19at4nqjjymca2lh3v546vcg5ktun3al4c5xu2pts3ewckv0y70ys0gupzr/relay.ngit.dev/codr)
provides an independent source and release mirror. Release mirrors use unchanged GitHub assets with verified SHA-256 checksums.
[Mirror policy and verification](docs/MIRRORING.md)

**codr** connects coding agent sessions in [**herdr**](https://github.com/herdrdev/herdr) to [White Noise](https://github.com/marmot-protocol/whitenoise-android), with one encrypted chat per session. It forwards prompts to the mapped agent and returns completed replies to the same chat.

Supported agents: OpenCode, Codex, and Claude Code on Linux and macOS.

<img src="docs/screenshots/herdr-phone-progress.png" width="1280" alt="herdr with the Garden planner task checklist beside matching working status and editable tasks in White Noise">

*Real interfaces, encrypted messages, and local speech-to-text. Fictional participants and scripted agent replies. Editable checklists require `edit_responses = true`. [Capture details](docs/REFERENCE.md#screenshot-provenance).*

## Behaviour

- Chat names pair the **herdr** tab and workspace: `[cd] review (Acme)`. Descriptions contain workspace and agent details.
- Status notices, optional editable checklists, and final replies appear as separate messages.
- Supported OpenCode questions accept a number or option label as a reply.
- New groups make your configured account an admin, so you can invite participants and rename the group. Only the owner can prompt the agent. [Group setup](docs/REFERENCE.md#shared-chats-and-owner-only-control)
- Local prompts automatically mute all sessions. A new owner message from your phone enables forwarding again.

## Install

Needs Linux x86_64 or aarch64 or macOS on Apple silicon or Intel, a per-user service manager (systemd or launchd), Python 3.11+, Git, and **herdr** 0.9.1+. No Rust toolchain required.

```sh
herdr plugin install tidley/codr --ref v0.6.0
herdr plugin action invoke local.white-noise.setup
```

Enter **your White Noise public key** (`npub`) during setup. Get White Noise for [iOS](https://github.com/marmot-protocol/whitenoise-ios) or [Android](https://github.com/marmot-protocol/whitenoise-android).

Or give your coding agent this prompt, with your own public key:

> Install https://github.com/tidley/codr. My White Noise public key is npub1…. Follow docs/INSTALL.md and report what you verified.

[Full installation and upgrade guide →](docs/INSTALL.md)

## Usage

1. Check the inviter against the bridge public key printed by setup, then accept the session's chat invitation.
2. Send `/status` to check the connection. Send a prompt to continue that session.
3. Read the reply in the same chat. For a supported decision, reply `1`, `2`, or an exact option label.

Handoff is automatic. Manual controls remain available through **herdr**'s **White Noise forwarding: At desk / Away** action. [Controls and limits →](docs/REFERENCE.md#at-desk--away-forwarding)

## How it works

White Noise ↔ MDK ↔ **codr** ↔ **herdr** ↔ your agent

[MDK](https://github.com/marmot-protocol/mdk) handles identity, encrypted messaging, and relays. **codr** routes prompts through **herdr** and reads completed replies from existing agent transcripts.

Reply collection uses existing transcripts without an additional model call or terminal scraping. [Behaviour and recovery](docs/REFERENCE.md)

OpenCode can use incremental, read-only SQLite reads instead of full exports. [Reader configuration](docs/REFERENCE.md#incremental-opencode-database-reads)

**Limits:** Decision replies support OpenCode single-choice questions with 1–9 options. Other dialogs need the terminal. Avoid simultaneous terminal/phone answers. [Decision compatibility](docs/REFERENCE.md#answer-decisions-from-the-phone) · [Media and audio](docs/REFERENCE.md#images-and-voice-messages)

## Roadmap

[Planned work and completed items](docs/REFERENCE.md#roadmap).

## Screenshots

<details>
<summary>A shared project, images, voice, decisions, and multiple chats</summary>

**Shared chat with owner-only agent control**

Alex invited Sam to the Garden planner chat. Sam can discuss the project, but only Alex's messages reach the agent.

<img src="docs/screenshots/herdr-phone-shared.png" width="1280" alt="Alex, Sam, and codr in a three-member White Noise chat, with only Alex's request shown in herdr">

**Image attachment**

<img src="docs/screenshots/herdr-phone-image.png" width="1280" alt="A watering-reminder design uploaded in White Noise beside its downloaded local image path and scripted review in OpenCode">

**Voice transcription**

<img src="docs/screenshots/herdr-phone-voice.png" width="1280" alt="A White Noise audio message and transcribing status beside the actual Phonon-2 transcript sent to the demo agent">

**Decision response**

<img src="docs/screenshots/herdr-phone-decisions.png" width="1280" alt="A ten-minute snooze decision in OpenCode beside the White Noise question card, phone reply 1, and choice confirmation">

**Review output**

<img src="docs/screenshots/herdr-phone-review.png" width="1280" alt="The herdr review tab beside its White Noise chat, both showing checks for the Garden planner watering reminder">

**Session chats in a project folder**

<img src="docs/screenshots/herdr-phone-conversations.png" width="1280" alt="Six Garden planner session tabs in herdr beside their six matching chats in a White Noise Garden folder">

</details>

---

[Operating reference](docs/REFERENCE.md) · [Changelog](CHANGELOG.md) · [Releases](https://github.com/tidley/codr/releases)

Built on [**herdr**](https://github.com/herdrdev/herdr), [White Noise](https://github.com/marmot-protocol/whitenoise-android), [MDK](https://github.com/marmot-protocol/mdk), and the [Marmot protocol](https://github.com/marmot-protocol/marmot). [Full credits](docs/REFERENCE.md#acknowledgements).
