# Release Balls

Generate a GIF and video of the PR authors in the release that triggered your workflow.

![PR authors in a real t3code release](https://uploads-production-47e4.up.railway.app/files/5456f31e-5c8d-453d-a4bd-f2ed21d3fe7f/summary-author.gif)

*Example: PR authors in a [t3code nightly release](https://github.com/pingdotgg/t3code/releases/tag/v0.0.43-nightly.20260920.2031).*

## Quick start

Copy [this workflow](examples/release.yml) to `.github/workflows/release-balls.yml`, then publish a release. It uploads a GIF and MP4 and adds the GIF and video link to the existing release notes.

Want a different ranking or format?

- `metric`: `author`, `merger`, or `changes` (added + deleted lines).
- `format`: `gif`, `mp4`, or `both`.

Every matching contributor is included. Larger releases cycle through pages without shrinking the text. Counts come from merged PR links in release notes; GitHub-generated notes work.

The action is currently private. [Set up access](docs/releases.md#private-action-access) before using it in another repository.

[Release setup](docs/releases.md) · [Manual runs](examples/artifacts.yml) · [All inputs](action.yml) · [Benchmarks](docs/performance.md)
