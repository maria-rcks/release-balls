# release balls

Tiny contributor animations for CI. One Python dependency (Pillow), FFmpeg, no browser, Node, GPU, or frame files. Produces H.264 MP4, animated GIF, source JSON, rankings, and release-note Markdown.

```sh
uv run release-balls --repo pingdotgg/t3code --releases 5 --match nightly --format both
uv run release-balls --repo pingdotgg/t3code --metric merger --users maria-rcks,juliusmarminge
uv run release-balls --data demo/data.json --per-release --format both
uv run release-balls --help
```

Install FFmpeg with your package manager. Python 3.11+; `uv sync --frozen` installs the pinned dependency. Any CI that has Python and FFmpeg can run the CLI. Authentication uses `GH_TOKEN`, `GITHUB_TOKEN`, or an existing `gh` login. Tokens never go to avatar URLs.

`author` counts merged PRs by author; `merger` counts the actor GitHub records as merging the PR; `changes` sums additions + deletions by author. `--users` filters logins; bots are excluded unless `--include-bots`. `--top` selects 1–6 rows. Counts are deduplicated across releases in aggregate mode. Ties sort alphabetically. Missing actors are omitted. Avatar speed is proportional to count within each video, normalized so its leader is readable; speeds are not comparable across separate videos.

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

GIFs embed in GitHub Markdown. MP4 release assets are downloadable links; GitHub does not universally render arbitrary MP4 links inline. Set `asset-base-url` (CLI: `--asset-base-url`) to the eventual public asset directory, such as `https://github.com/OWNER/REPO/releases/download/TAG`. The generated `embed.md` then contains usable GIF embeds and video links once files have been uploaded. Without it, links are relative filenames.

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

![five real t3code nightly releases](https://uploads-production-47e4.up.railway.app/files/45ef1b9b-36ea-4121-9b9a-d81455978cd0/five-nightlies.mp4)

![authors across five t3code nightlies](https://uploads-production-47e4.up.railway.app/files/91d15eb0-6f49-4abe-8a69-d8ffb48dbce7/summary-author.gif)

![merge actors across five t3code nightlies](https://uploads-production-47e4.up.railway.app/files/ff9cc65a-55e7-4003-ae9a-eeb794534b83/summary-merger.gif)

## Benchmarks

Three repetitions per variant. Rendering on a dedicated Blacksmith 4-vCPU Ubuntu worker, Python 3.12, Pillow 12.3, FFmpeg 6.1. Warm avatars, real snapshot, six contributors, six-second 720×720 / 30fps MP4. Wall time includes artwork and FFmpeg startup/encoding; excludes installation, API collection, and avatar downloads. [Raw rendering results](demo/render-benchmark.json).

| renderer | x264 preset | median seconds | output bytes |
| --- | --- | ---: | ---: |
| cached background | ultrafast | 0.166 | 61,490 |
| cached background | superfast | 0.215 | 46,711 |
| cached background | veryfast | 0.226 | 38,069 |
| restore moved regions | ultrafast | **0.146** | 61,490 |
| restore moved regions | superfast | 0.202 | 46,711 |
| restore moved regions | veryfast | 0.224 | 38,069 |

The fastest measured variant is the default. `--preset veryfast` trades speed for smaller output. GIF uses 480px / 15fps with a generated palette; its cost is additional and is not included in this MP4 table. Both formats share the same frame producer. No claim of a universal fastest renderer: timings vary by CPU, workload, network, and encoder.

The supplied script took 3.455s versus 0.177s for the prototype at matching 1080×1080 / 30fps / three seconds with the same real top-three contributors. This is an end-product comparison, **not an isolated algorithm speedup**: artwork and encoding settings differ (original medium/CRF17; prototype ultrafast/CRF23). Original source is preserved in `benchmarks/reference.py`; the harness changes duration, fps, and people, and warms avatars.

Live GitHub collection on the development host: original REST median **9.968s**, batched PR GraphQL **9.261s**, plus concurrent release pages **3.659s**. Every PR field and selected release matched in all nine runs. These sequential network measurements are affected by GitHub caches and latency. [Raw collection results](demo/collection-benchmark.json).

Reproduce rendering on a dedicated worker with `uv run benchmarks/bench.py --reference --runs 3`. Avoid mixing installation/network time into render comparisons. The initial committed prototype is `fff547a`; the render matrix compares its cached method to region restoration on the same worker and workload.
