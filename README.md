# release balls

Tiny contributor animations for CI. One Python dependency (Pillow), FFmpeg, no browser, Node, GPU, or frame files. Produces H.264 MP4, animated GIF, source JSON, rankings, and release-note Markdown.

```sh
uv run release-balls --repo pingdotgg/t3code --releases 5 --match nightly --format both
uv run release-balls --repo pingdotgg/t3code --metric merger --users maria-rcks,juliusmarminge
uv run release-balls --data demo/data.json --per-release --format both
uv run release-balls --help
```

Install FFmpeg with your package manager. Python 3.11+; `uv sync --frozen` installs the pinned dependency. Any CI that has Python and FFmpeg can run the CLI. Authentication uses `GH_TOKEN`, `GITHUB_TOKEN`, or an existing `gh` login. Tokens never go to avatar URLs.

`author` counts merged PRs by author; `merger` counts the actor GitHub records as merging the PR; `changes` sums additions + deletions by author. `--users` filters logins; bots are excluded unless `--include-bots`. `--top` selects 1–6 rows, defaulting to three. Counts are deduplicated across releases in aggregate mode. Ties sort alphabetically. Missing actors are omitted. The default composition keeps the supplied script's white background, centered title, large avatars, black labels, 1080px, 60fps, and 16 seconds. The 48px title sits near the top, with the contributor names and avatars centered vertically as one group between the title and watermark for any row count. A faded repo URL overlays the bottom-right corner without reserving space. All names share a 44px font; the count sits 12px below the name's font bounds. The avatar region shifts right to fit labels; exceptionally long logins use an ellipsis instead of a smaller font, with full identities preserved in JSON. The leader crosses the available space in 1.1 seconds and other speeds keep the same count ratios. Speeds are normalized within each video, so they are not comparable between separate videos.

The prototype counts **merged PR URLs explicitly linked in release notes**, not every commit between tags. It scans all release pages and sorts by publication time, excluding drafts and tags that don't match. It rejects unsupported notes instead of silently counting zero. Custom providers can supply the same `data.json` schema. Live collection currently supports GitHub.com only.

## GitHub Actions

The private repository's Actions tab has a **release videos** workflow with metric and format choices. It reads the latest five `pingdotgg/t3code` nightlies and uploads the results as an artifact.

For another repository (grant access to this private action first), pin the action to a reviewed commit:

```yaml
permissions:
  contents: read
steps:
  - uses: maria-rcks/release-balls@perf/ci-prototype # replace with a reviewed SHA
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

Use an Ubuntu runner. The action installs FFmpeg only if missing; provisioning time is separate from rendering. No checkout of the source repository is needed. On other CI, use the CLI. The action exposes `directory`, `manifest`, `data`, and `markdown` paths for subsequent steps. `manifest.json` lists every generated file and ranked count.

## Release notes and other actions

GIFs embed in GitHub Markdown; the generated GIF embed links to the source repository. MP4/GIF pixels cannot contain clickable regions, so the visible watermark is a URL and the GIF embed supplies its link. MP4 release assets are downloadable links; GitHub does not universally render arbitrary MP4 links inline. Set `asset-base-url` (CLI: `--asset-base-url`) to the eventual public asset directory, such as `https://github.com/OWNER/REPO/releases/download/TAG`. The generated `embed.md` then contains usable GIF embeds and video links once files have been uploaded. Without it, links are relative filenames.

A later authorized workflow step can upload `directory/*.gif` and `directory/*.mp4` with `gh release upload`, and append `embed.md` to the existing release body. Give only that publishing job `contents: write`. Uploading assets alone does not insert them in release notes. Private assets require authentication and may not work as externally embedded media. The renderer itself never changes releases or sends messages.

## Real t3code example

Snapshot fetched September 20, 2026. Latest five published nightly releases:

| tag suffix | listed merged PRs |
| --- | ---: |
| 20260920.2018 | 3 |
| 20260920.2005 | 2 |
| 20260920.1990 | 7 |
| 20260919.1978 | 3 |
| 20260919.1962 | 4 |

19 unique PRs. Author leaders: cestercian 5, juliusmarminge 4, Bil0000 3. Merge actors: juliusmarminge 11, shivamhwp 5, maria-rcks 3. Full source, PR URLs, dates, and identities are in [demo/data.json](demo/data.json).

![five real t3code nightly releases](https://uploads-production-47e4.up.railway.app/files/1014dbf4-1eef-48ed-8125-968e9a7c4928/five-nightlies.mp4)

![authors across five t3code nightlies](https://uploads-production-47e4.up.railway.app/files/a3be886c-f9ca-4c9e-8cb8-07b3b05da881/summary-author.gif)

![merge actors across five t3code nightlies](https://uploads-production-47e4.up.railway.app/files/9f23d1dd-99eb-4624-b645-d1e00943ec7f/summary-merger.gif)

## Benchmarks

Rerun with the current typography, watermark, and faster motion. Three repetitions per variant on a dedicated Blacksmith 4-vCPU Ubuntu worker, Python 3.12, Pillow 12.3, FFmpeg 6.1. Fixed comparison workload: six real contributors, six-second 720×720 / 30fps MP4, warm avatars. This is smaller than the restored 1080px / 60fps / 16s default. Wall time includes artwork and FFmpeg startup/encoding; excludes installation, API collection, and avatar downloads. [Raw rendering results](demo/render-benchmark.json).

| renderer | x264 preset | median seconds | output bytes |
| --- | --- | ---: | ---: |
| cached | ultrafast | 0.195 | 74,009 |
| cached | superfast | 0.231 | 54,534 |
| cached | veryfast | 0.251 | 42,368 |
| dirty | ultrafast | 0.158 | 74,009 |
| dirty | superfast | 0.231 | 54,534 |
| dirty | veryfast | 0.255 | 42,368 |

Both rendering strategies reuse static artwork; the current default is the fastest measured variant in this run. `--preset veryfast` trades speed for smaller output. GIF uses 480px / 15fps with a generated palette; its cost is additional and excluded from this MP4 table. Both formats share one frame producer. The original source remains in `benchmarks/reference.py`.

Live GitHub collection measurements are unchanged: original REST median 9.968s, batched PR GraphQL 9.261s, plus concurrent release pages 3.659s. Every PR field and selected release matched in all nine runs. Sequential network measurements are affected by GitHub caches and latency. [Raw collection results](demo/collection-benchmark.json).

Reproduce rendering on a dedicated worker with `uv run benchmarks/bench.py --runs 3`. No universal fastest-renderer claim; timings depend on the workload and machine.
