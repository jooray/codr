# codr

**Your coding agents, on your phone.**

Leave your agents running in [HERDR](https://github.com/herdrdev/herdr). Send prompts and read replies from White Noise—one encrypted chat per session.

Supports **OpenCode, Codex, and Claude Code** on Linux.

<img src="docs/screenshots/herdr-phone-progress.png" width="1280" alt="HERDR with an OpenCode task checklist beside the matching White Noise conversation and separate final reply">

*Real interfaces, synthetic demo history—not a live agent run. Editable checklists require an edit-capable MDK build. [Capture details](docs/REFERENCE.md#screenshot-provenance).*

## What it does

- **One session, one chat.** Chat names follow HERDR tab labels: `HDR | review`.
- **Prompt from anywhere.** Messages reach the agent mapped to that chat. Completed replies come back to the same place.
- **Follow the work.** Receive status updates, optional editable checklists, and separate final replies.
- **Answer decisions.** Reply with a number or option label to supported OpenCode questions.
- **Choose when to listen.** At desk mutes all sessions. Away forwards them. A new phone message switches all sessions to Away.

## Install

Needs Linux x86_64 or aarch64, systemd user services, Python 3.11+, Git, and HERDR 0.9.1+. No Rust toolchain required.

```sh
herdr plugin install tidley/codr --ref v0.4.0
herdr plugin action invoke local.white-noise.setup
```

Enter **your White Noise public key** (`npub`) during setup. Get White Noise for [iOS](https://github.com/marmot-protocol/whitenoise-ios) or [Android](https://github.com/marmot-protocol/whitenoise-android).

Or give your coding agent this prompt, with your own public key:

> Install https://github.com/tidley/codr. My White Noise public key is npub1…. Follow docs/INSTALL.md and report what you verified.

[Full installation and upgrade guide →](docs/INSTALL.md)

## Use it

1. Check the inviter against the bridge public key printed by setup, then accept the session's chat invitation.
2. Send `/status` to check the connection. Send a prompt to continue that session.
3. Read the reply in the same chat. For a supported decision, reply `1`, `2`, or an exact option label.

Use HERDR's **White Noise forwarding: At desk / Away** action to control forwarding. [Shortcut and controls →](docs/REFERENCE.md#at-desk--away-forwarding)

## How it works

```text
White Noise ↔ MDK ↔ codr ↔ HERDR ↔ your agent
```

[MDK](https://github.com/marmot-protocol/mdk) handles identity, encrypted messaging, and relays. codr routes prompts through HERDR and reads completed replies from existing agent transcripts.

No extra model call to collect replies. No terminal scraping for final answers. [Behaviour and recovery →](docs/REFERENCE.md)

**Limits:** Decisions target OpenCode 1.18.35 single-choice questions with 1–9 options. Other dialogs need the terminal. Avoid simultaneous terminal/phone answers. Attachments are not supported. [Details →](docs/REFERENCE.md#answer-decisions-from-the-phone)

## Screenshots

<details>
<summary>Decisions, a review, and multiple chats</summary>

**Answer a decision**

<img src="docs/screenshots/herdr-phone-decisions.png" width="1280" alt="HERDR showing an example question, the reply 1, and its result beside the matching White Noise decision conversation">

**Follow a review**

<img src="docs/screenshots/herdr-phone-review.png" width="1280" alt="The HERDR review tab beside its White Noise chat, both showing the same example release review">

**Keep conversations separate**

<img src="docs/screenshots/herdr-phone-conversations.png" width="1280" alt="HERDR with demo, decisions, and review tabs beside the three matching White Noise chats">

</details>

---

[Operating reference](docs/REFERENCE.md) · [Roadmap](docs/REFERENCE.md#roadmap) · [Releases](https://github.com/tidley/codr/releases)

Built on [HERDR](https://github.com/herdrdev/herdr), [White Noise](https://github.com/marmot-protocol/whitenoise-android), [MDK](https://github.com/marmot-protocol/mdk), and the [Marmot protocol](https://github.com/marmot-protocol/marmot). [Full credits](docs/REFERENCE.md#acknowledgements).
