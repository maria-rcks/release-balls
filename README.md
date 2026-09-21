# Release Balls

Turn GitHub releases into animated contributor leaderboards. Generate a GIF or MP4 showing who authored the most merged PRs, merged the most PRs, or changed the most lines.

![Contributors across five real t3code nightly releases](https://uploads-production-47e4.up.railway.app/files/5df8a3aa-cd24-48d7-b2d7-766845f75a4f/summary-merger.gif)

*Example: merge counts across five [t3code nightlies](demo/data.json).*

## Quick start

Copy [this workflow](examples/artifacts.yml) to `.github/workflows/release-balls.yml`. Run **Actions → Release Balls → Run workflow**, then download **release-media** from the finished run.

Change `repository`, `releases`, and `match` to choose your releases. Pick your output:

- `metric`: `author`, `merger`, or `changes` (added + deleted lines).
- `format`: `gif`, `mp4`, or `both`.

Counts come from merged PR links in release notes. GitHub-generated release notes work.

The action is currently private. [Set up access](docs/releases.md#private-action-access) before using it in another repository.

[Publish to release notes](docs/releases.md) · [All inputs](action.yml) · [Benchmarks](docs/performance.md)
