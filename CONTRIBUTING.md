# Contributing

Use [GitHub](https://github.com/tidley/codr) for **codr** source, issues, pull requests, reviews, CI, and releases.
You do not need a Nostr account or ngit to contribute.

## Clone and contribute

Fork the repository on GitHub, then clone your fork:

```sh
git clone https://github.com/YOUR-USER/codr.git
cd codr
git switch -c my-change
```

Run the relevant tests and commit your change. Push your branch to your fork, then open a GitHub pull request.
Use `master` for stable changes or `dev/linux` for Linux preview changes. State which branch your change targets.
For bug reports, use [GitHub Issues](https://github.com/tidley/codr/issues).

## Maintainer mirror

GitHub is primary. Publish reviewed commits and tags to GitHub first, then mirror the same approved refs to ngit.
Identify remotes by URL. This maintainer checkout keeps `github` for `https://github.com/tidley/codr.git` and `origin` for the existing `nostr://` mirror.
Contributors can keep the normal GitHub `origin` name in their own clones.
Do not use `git push --mirror`: it can remove unrelated remote refs.

Keep `master` as the stable branch and `dev/linux` as the Linux preview branch.
Do not move release tags or replace published assets during mirror updates.

GitHub Actions builds releases. Mirror the exact published archives, checksum files, notes, source commit, release date, and stable or preview channel.
Do not rebuild, repackage, re-sign, or replace published assets to create a mirror.
Download each mirrored asset from Blossom and compare its SHA-256 hash with the GitHub original before reporting success.
See [mirror operations](docs/MIRRORING.md).

## Existing ngit discussions

Keep existing ngit issues, PRs, comments, and attribution. Do not duplicate or close them solely because GitHub is now primary.
Use GitHub for new collaboration. Move an existing discussion only with the author's or maintainer's agreement, and link its original record.

Juraj Bednar's (`jooray`) OpenCode executable contribution was integrated in `c64eaabc3f7e3e2b449a576208a4d9dda0b86b99`.
The [ngit carry-over record](https://gitworkshop.dev/nevent1qqsp9m5qz37u050larxt7vh7fruzcgjc38rclxxs3a4pp0dex08ud2cpz3mhxue69uhhyetvv9ujumn8d96zuer9wcvy8xg5)
preserves the original attribution and discussion.
