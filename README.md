<img src="assets/logo.svg" alt="" width="96" align="right">

# Release Balls

A GitHub Action that turns a release into a short race between the people who shipped it. Each contributor is a ball with their avatar; the more merged PRs they have, the faster they go. You get a GIF, an MP4, and the rankings as JSON.

![PR authors across five t3code nightly releases](https://uploads-production-47e4.up.railway.app/files/3168ea46-9f0c-4393-b669-2ea04c278f5e/summary-author.gif)

*PR authors across five [t3code nightly releases](https://github.com/pingdotgg/t3code/releases), rendered with `releases: 5` and `match: nightly`.*

## Quick start

Copy [examples/release.yml](examples/release.yml) to `.github/workflows/release-balls.yml` and publish a release. The workflow:

1. Ranks the PR authors in the release that triggered it.
2. Attaches the GIF and MP4 to that release.
3. Appends the GIF and a video link to the release notes, leaving your notes intact.

Rerunning it replaces the same assets and the same notes section.

The first run on a new version builds the Rust binary, which takes about 3 minutes. Later runs reuse the cached binary.

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

## Rank a time window

Set `since` to rank every PR merged in a window, with or without releases: `24h`, `7d`, `2w`, or a UTC date like `2026-09-01`. The title reads "Pull requests merged in the past week". [examples/weekly.yml](examples/weekly.yml) renders last week's PRs every Monday.

```yaml
- uses: maria-rcks/release-balls@v1
  with:
    since: 7d
```

Windows come from GitHub search, which returns at most 1,000 PRs; use a shorter window for busier repositories.

## Inputs

| Input | Default | What it does |
| --- | --- | --- |
| `metric` | `author` | `author` counts merged PRs by author, `merger` counts who merged them, `changes` counts added + deleted lines by author. |
| `format` | `mp4` | `gif`, `mp4`, or `both`. |
| `since` | unset | Rank PRs merged in a window (`24h`, `7d`, `2w`, `2026-09-01`) instead of releases. Overrides `tag`, `releases`, and `match`. |
| `tag` | the triggering release | Exact release tag. Overrides `releases` and `match`. Set `tag: ''` to select by count instead. |
| `releases` | `1` | How many of the latest published releases to combine when `tag` is empty. |
| `match` | `''` | Substring a tag must contain, such as `nightly`. Empty matches every tag. |
| `users` | everyone except bots | Comma-separated logins to include. |
| `top` | `10` | Show the top N contributors. `0` shows everyone. |
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

- In release mode, only merged PRs linked from release notes count, as full URLs like `https://github.com/owner/repo/pull/123`. GitHub-generated release notes work. PRs are not inferred from commits between tags.
- A PR linked from several selected releases counts once.
- Drafts are skipped. Published prereleases are included.
- Bots are excluded. Ties sort alphabetically.
- The top 10 contributors are shown. With `top: 0`, everyone is shown and large lists split into columns with smaller labels.
- The fastest ball crosses in one second, so speeds are relative within a clip. Clips last 16 seconds. MP4 is 1080px at 60fps, GIF is 480px at 15fps.

## Run it locally

Needs Rust 1.88+, FFmpeg, and FreeType headers (`libfreetype6-dev` and `pkg-config` on Ubuntu).

```sh
cargo build --release --locked
export GH_TOKEN=...  # can read the source repository
./target/release/release-balls --repo pingdotgg/t3code --releases 5 --match nightly --metric merger --format both
```

Files go to `out/`. The CLI defaults differ from the action: 5 releases matching `nightly`. Useful flags: `--since 7d`, `--tag`, `--per-release`, `--top N` (default 10, `0` for everyone), `--include-bots`, `--data data.json` to re-render a saved snapshot without calling the GitHub API. Run `--help` for the rest.

Rendering the example MP4 takes about 0.7 seconds on a 4-vCPU runner, excluding GitHub requests and the build.

## More

[Release publishing details](docs/releases.md) covers tag selection, private repositories, and releases created by other workflows.

## License

[MIT](LICENSE). The bundled font is CC0; see [assets](assets/README.md).
