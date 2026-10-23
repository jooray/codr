# Contributing

Development, issues, and pull requests for **codr** are tracked on
[ngit](https://gitworkshop.dev/npub19at4nqjjymca2lh3v546vcg5ktun3al4c5xu2pts3ewckv0y70ys0gupzr/relay.ngit.dev/codr).
GitHub mirrors the code and continues to provide CI and release downloads.
Please file issues and pull requests on ngit, not GitHub.

## Clone and contribute

Install [ngit](https://ngit.dev/install), then clone the canonical repository:

```sh
git clone nostr://npub19at4nqjjymca2lh3v546vcg5ktun3al4c5xu2pts3ewckv0y70ys0gupzr/relay.ngit.dev/codr
cd codr
git switch -c pr/my-change
```

Run the relevant tests and commit your change. Push the `pr/` branch to open a
pull request, or use `ngit send` for a longer description. See the
[ngit pull-request guide](https://ngit.dev/pull-requests).
For bug reports, use the repository's Issues view or `ngit issue create`.

## Maintainer mirror

Use `origin` for the canonical `nostr://` remote and `github` for
`https://github.com/tidley/codr.git`. Publish reviewed commits to ngit first,
then verify that the same branch and tags are present on GitHub. Do not use
`git push --mirror`: it can remove unrelated remote refs.

Keep `master` as the stable branch and `dev/linux` as the Linux preview branch.
Do not move release tags or replace published assets during mirror updates.

The GitHub PR switch hides existing PRs without deleting them. The outstanding
[GitHub PR #6](https://github.com/tidley/codr/pull/6), by Juraj Bednar (`jooray`),
is “Keep the OpenCode executable across Homebrew upgrades”. It remains unmerged
and is preserved in an attributed [ngit carry-over issue](https://gitworkshop.dev/nevent1qqsp9m5qz37u050larxt7vh7fruzcgjc38rclxxs3a4pp0dex08ud2cpz3mhxue69uhhyetvv9ujumn8d96zuer9wcvy8xg5).
