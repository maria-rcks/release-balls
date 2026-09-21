# release balls

turn github releases into a contributor race. choose who authored the most merged prs, who merged them, or who changed the most lines. get a gif, an mp4, and the numbers behind them.

![merge actors across five real t3code nightly releases](https://uploads-production-47e4.up.railway.app/files/5df8a3aa-cd24-48d7-b2d7-766845f75a4f/summary-merger.gif)

real [pingdotgg/t3code](https://github.com/pingdotgg/t3code) data: five nightlies published september 19–20, 2026, with 19 unique merged prs. juliusmarminge merged 11, shivamhwp 5, and maria-rcks 3. [saved source data](demo/data.json).

## try it in github actions

this action is currently private and shared with other private `maria-rcks` repositories. the workflow below works there. for another account, copy the action into your own private repository, enable access under **settings → actions → general → access**, and change the `uses:` reference. a token passed to the action cannot grant access to the action itself.

save this as `.github/workflows/release-balls.yml`, then open **actions → release balls → run workflow**:

```yaml
name: release balls
on:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  render:
    runs-on: ubuntu-latest
    steps:
      - uses: maria-rcks/release-balls@efb1c937455874a887b83c0dc6cea9d9c37a28e8
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

download `release-media` from the finished run. it contains the video, gif, `data.json`, `manifest.json` with rankings, and `embed.md` for release notes. no checkout step is needed. the first run compiles rust; later runs reuse the binary when github has a cache for that ref or the default branch.

## make it yours

| input | what it does | default |
| --- | --- | --- |
| `repository` | repository whose releases to read | current repository |
| `releases` | number of latest published releases | `5` |
| `match` | substring in release tags; `''` includes all tags | `nightly` |
| `tag` | exact published tag; overrides `releases` and `match` | unset |
| `metric` | `author`: merged prs by author; `merger`: who merged them; `changes`: additions + deletions by author | `author` |
| `format` | `gif`, `mp4`, or `both` | `mp4` |
| `users` | comma-separated logins to include | everyone except bots |

for your own repository, set `repository: ${{ github.repository }}` and add `pull-requests: read` to `permissions` for private pr details. set `match: ''` to include all release tags instead of only nightlies. for a different private source repository, also set `token: ${{ secrets.SOURCE_READ_TOKEN }}` to a token with access to its releases and pull requests. the default token reads the repository running the workflow.

the animation shows the top three contributors. mp4 defaults to 1080px at 60fps; gif to 480px at 15fps. both last 16 seconds.

## what gets counted

release notes must contain full merged pr links, such as `https://github.com/owner/repo/pull/123`. github-generated release notes work. the tool reads those prs and deduplicates them across the selected releases; it does not infer prs from commits between tags. notes without supported pr links fail with an explanation, unless they explicitly describe an empty changelog.

releases are selected by publication time and tag match. drafts are excluded; published prereleases are included. `merger` uses the actor github records, which can be a bot. bots are excluded by default, and ties sort alphabetically. each video's fastest ball crosses in one second, so speeds are relative within that video.

## use the files elsewhere

pass `${{ steps.balls.outputs.directory }}` to another action to upload or process the files. the other outputs are paths: `data` is the source snapshot, `manifest` is the rankings and file list, and `markdown` is the generated embed text.

for an automatic release workflow, follow the [release publishing example](docs/releases.md). set `asset-base-url` to `https://github.com/OWNER/REPO/releases/download/TAG`, upload the gif/mp4 to that release, and append the contents of `markdown` to its existing notes. the publishing step needs `contents: write`. rendering alone does not modify releases.

gifs embed in github markdown and link back to the source repository. mp4s are download links. private release assets require authentication and may not display as embedded media outside the repository.

## local or other ci

from a checkout of this repository, install rust 1.88+, ffmpeg, and freetype development headers (`libfreetype6-dev` and `pkg-config` on ubuntu), then run:

```sh
cargo build --release --locked
export GH_TOKEN=... # token with read access to the source repository
./target/release/release-balls --repo pingdotgg/t3code --releases 5 --match nightly --metric merger --format both
```

files go to `out/`. use `--per-release` for a separate animation per release, `--top 1` through `--top 6` to change the number of people, or `--include-bots`. run `--help` for all options. live collection supports github.com.

rust + ffmpeg render the example mp4 in about 0.73 seconds on a dedicated 4-vcpu worker, excluding github requests, downloads, and builds. [benchmarks and methodology](docs/performance.md). [real workflow checks](docs/e2e.md).
