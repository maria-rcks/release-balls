# Release Balls

A GitHub Action that turns a release into a short race between the people who shipped it. Each contributor is a ball with their avatar; the more merged PRs they have, the faster they go. You get a GIF, an MP4, and the rankings as JSON.

![PR authors in a real t3code release](https://uploads-production-47e4.up.railway.app/files/498c7269-9b47-492a-9aa3-bbea5be6a826/summary-author.gif)

*PR authors in a [t3code nightly release](https://github.com/pingdotgg/t3code/releases/tag/v0.0.43-nightly.20260920.2031).*

## Quick start

Copy [examples/release.yml](examples/release.yml) to `.github/workflows/release-balls.yml` and publish a release. The workflow:

1. Ranks the PR authors in the release that triggered it.
2. Attaches the GIF and MP4 to that release.
3. Appends the GIF and a video link to the release notes, leaving your notes intact.

Rerunning it replaces the same assets and the same notes section.

The action is private for now. Before using it from another repository, see [private action access](docs/releases.md#private-action-access).

## Render without publishing

To get the files as a workflow artifact instead, render any repository's releases on demand ([examples/artifacts.yml](examples/artifacts.yml)):

```yaml
- uses: maria-rcks/release-balls@v1
  id: balls
  with:
    repository: pingdotgg/t3code
    releases: '5'
    match: nightly
    metric: merger
    format: both

- uses: actions/upload-artifact@v4
  with:
    name: release-media
    path: ${{ steps.balls.outputs.directory }}
```

## Inputs

| Input | Default | What it does |
| --- | --- | --- |
| `metric` | `author` | `author` counts merged PRs by author, `merger` counts who merged them, `changes` counts added + deleted lines by author. |
| `format` | `mp4` | `gif`, `mp4`, or `both`. |
| `tag` | the triggering release | Exact release tag. Overrides `releases` and `match`. Set `tag: ''` to select by count instead. |
| `releases` | `1` | How many of the latest published releases to combine when `tag` is empty. |
| `match` | `''` | Substring a tag must contain, such as `nightly`. Empty matches every tag. |
| `users` | everyone except bots | Comma-separated logins to include. |
| `repository` | current repository | `owner/repo` to read releases from. |
| `token` | `github.token` | Token that can read the source repository's releases and PRs. |
| `asset-base-url` | unset | URL where the media will be hosted, used to build links in `embed.md`. |
| `output` | `release-balls-output` | Output directory. |

## Outputs

All outputs are paths on the runner.

| Output | Contents |
| --- | --- |
| `directory` | Everything below, plus the GIF and/or MP4. |
| `markdown` | `embed.md`, ready to paste into release notes. |
| `manifest` | `manifest.json` with the rankings and generated files. |
| `data` | `data.json`, a snapshot of the releases and PRs that were read. |

To use the files in another job, upload `directory` as an artifact. See [passing files between jobs](docs/releases.md#pass-files-to-another-job).

## What gets counted

- Only merged PRs linked from release notes count, as full URLs like `https://github.com/owner/repo/pull/123`. GitHub-generated release notes work. PRs are not inferred from commits between tags.
- A PR linked from several selected releases counts once.
- Drafts are skipped. Published prereleases are included.
- Bots are excluded. Ties sort alphabetically.
- Every contributor who qualifies is shown at once. Rows and columns shrink to fit, so labels get small with very large lists.
- The fastest ball crosses in one second, so speeds are relative within a clip. Clips last 16 seconds. MP4 is 1080px at 60fps, GIF is 480px at 15fps.

## Run it locally

Needs Rust 1.88+, FFmpeg, and FreeType headers (`libfreetype6-dev` and `pkg-config` on Ubuntu).

```sh
cargo build --release --locked
export GH_TOKEN=...  # can read the source repository
./target/release/release-balls --repo pingdotgg/t3code --releases 5 --match nightly --metric merger --format both
```

Files go to `out/`. The CLI defaults differ from the action: 5 releases matching `nightly`. Useful flags: `--tag`, `--per-release`, `--top N`, `--include-bots`, `--data data.json` to re-render a saved snapshot without calling the GitHub API. Run `--help` for the rest.

Rendering the example MP4 takes about 0.7 seconds on a 4-vCPU runner, excluding GitHub requests and the build. The action caches the compiled binary, so only the first run on a new ref pays for a Rust build.

## More

- [Release publishing details](docs/releases.md): tag selection, private repositories, and releases created by other workflows.
- [Verified workflow runs](docs/e2e.md)
- [Benchmarks](docs/performance.md)
