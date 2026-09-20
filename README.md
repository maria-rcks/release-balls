# release balls

Contributor animations for CI, written in Rust. One compiled binary plus FFmpeg and FreeType at runtime; no Python, browser, Node, GPU, or frame files. Produces H.264 MP4, animated GIF, source JSON, rankings, and release-note Markdown.

```sh
cargo build --release --locked
./target/release/release-balls --repo pingdotgg/t3code --releases 5 --match nightly --format both
./target/release/release-balls --repo pingdotgg/t3code --metric merger --users maria-rcks,juliusmarminge
./target/release/release-balls --data demo/data.json --per-release --format both
./target/release/release-balls --help
```

Build with Rust 1.88+ and FreeType development headers (`sudo apt-get install libfreetype6-dev pkg-config ffmpeg` on Ubuntu). Run the resulting binary directly on a compatible system with FFmpeg and FreeType installed. The font is bundled with its license. The original Python implementation remains as a benchmark reference. Authentication uses `GH_TOKEN`, `GITHUB_TOKEN`, or an existing `gh` login. Tokens never go to avatar URLs.

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

Use an Ubuntu GitHub-hosted runner. The action builds with Cargo on the first run and caches the binary by source, dependency lockfile, font, architecture, and Ubuntu image family. Cache hits execute the binary directly. The first build and dependency installation take longer and are excluded from render benchmarks. FFmpeg is installed only if missing. No checkout of the source repository is needed. On other CI, use the CLI. The action exposes `directory`, `manifest`, `data`, and `markdown` paths for subsequent steps. `manifest.json` lists every generated file and ranked count.

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

![five real t3code nightly releases](https://uploads-production-47e4.up.railway.app/files/40719c93-b211-468b-aaeb-0aa019f314bd/five-nightlies.mp4)

![authors across five t3code nightlies](https://uploads-production-47e4.up.railway.app/files/5ba2b74c-8387-4641-81a7-401acd5f76ff/summary-author.gif)

![merge actors across five t3code nightlies](https://uploads-production-47e4.up.railway.app/files/3e75c76a-450f-4784-99f2-37d3269f240f/summary-merger.gif)

## Performance

The default `--strategy yuv` caches the background and avatar planes in the encoder's YUV420 format. Four chroma variants preserve one-pixel movement. Only changed rectangles are copied per frame. GIFs render directly at 480px / up to 15fps, and `--format both` runs the two encoders concurrently. The MP4 default remains 1080px / 60fps / 16 seconds, CRF 23. The GIF rasterization and YUV conversion can differ slightly from the Python reference; layout, counts, and animation speeds stay the same.

`--strategy dirty` and `--strategy cached` retain the RGB paths for comparison. `--threads` controls MP4 encoder threads (default 2); `--preset veryfast` trades speed for smaller files. Python is required only for the benchmark harness:

```sh
uv run benchmarks/rust_bench.py --strategies yuv --formats mp4 gif both --runs 3 --probe
```

Measurements use a dedicated 4-vCPU Blacksmith Ubuntu worker, identical saved t3code data and cached avatars, alternating Python/Rust order, and subprocess wall time including startup and encoding. API requests, avatar downloads, compilation, and installation are excluded. The harness checks rankings, dimensions, and decoded frame counts and records source hashes and raw timings. These are workload-specific measurements, not a universal fastest-renderer claim.

Historical Python tuning results remain in [render-benchmark.json](demo/render-benchmark.json) and [collection-benchmark.json](demo/collection-benchmark.json). The initial Rust RGB comparison is in [rust-benchmark-rgb.json](demo/rust-benchmark-rgb.json).

Three-run medians with the final candidate, matching top-three merger rankings. Python baseline: `4c29752`; candidate source and binary hashes are recorded in [rust-benchmark.json](demo/rust-benchmark.json). MP4 stays at 1080px/60fps; GIF stays at 480px/15fps.

| workload | Python seconds | Rust seconds | speedup |
| --- | ---: | ---: | ---: |
| 16s, mp4 | 2.025 | 0.947 | 2.14× |
| 16s, gif | 1.606 | 0.344 | 4.67× |
| 16s, both | 3.062 | 0.805 | 3.80× |

The optimization loop measured the initial RGB port, YUV transport, native GIF rendering, 1/2/4 encoder threads, and cropped static artwork resampling. Two threads remained the default. [YUV trial](demo/rust-benchmark-yuv.json) and [thread trials](demo/rust-benchmark-threads.json) retain the intermediate raw results. Run-to-run scheduling affects timings.
