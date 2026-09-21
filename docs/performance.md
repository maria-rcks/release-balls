# performance

The default `--strategy yuv` caches the background and avatar planes in the encoder's YUV420 format. Four chroma variants preserve one-pixel movement. Only changed rectangles are copied per frame. GIFs render directly at 480px / up to 15fps, and `--format both` runs the two encoders concurrently. The MP4 default remains 1080px / 60fps / 16 seconds, CRF 23. The GIF rasterization and YUV conversion can differ slightly from the Python reference; layout and counts stay the same. The lead avatar now traverses in 1.0 second (10% faster motion than the original Rust port).

`--strategy dirty` and `--strategy cached` retain the RGB paths for comparison. `--threads` controls MP4 encoder threads (default 2); `--preset veryfast` trades speed for smaller files. Python is required only for the benchmark harness:

```sh
uv run benchmarks/rust_bench.py --strategies yuv --formats mp4 gif both --runs 3 --probe
```

Measurements use a dedicated 4-vCPU Blacksmith Ubuntu worker, identical saved t3code data and cached avatars, alternating Python/Rust order, and subprocess wall time including startup and encoding. API requests, avatar downloads, compilation, and installation are excluded. The harness checks rankings, dimensions, and decoded frame counts and records source hashes and raw timings. These are workload-specific measurements, not a universal fastest-renderer claim.

Historical Python tuning results remain in [render-benchmark.json](../demo/render-benchmark.json) and [collection-benchmark.json](../demo/collection-benchmark.json). The initial Rust RGB comparison is in [rust-benchmark-rgb.json](../demo/rust-benchmark-rgb.json).

Three-run medians with the final candidate, matching top-three merger rankings. Python baseline: `4c29752`; candidate source and binary hashes are recorded in [rust-benchmark.json](../demo/rust-benchmark.json). MP4 stays at 1080px/60fps; GIF stays at 480px/15fps.

| workload | Python seconds | Rust seconds | speedup |
| --- | ---: | ---: | ---: |
| 16s, mp4 | 2.025 | 0.947 | 2.14× |
| 16s, gif | 1.606 | 0.344 | 4.67× |
| 16s, both | 3.062 | 0.805 | 3.80× |

The optimization loop measured the initial RGB port, YUV transport, native GIF rendering, 1/2/4 encoder threads, and cropped static artwork resampling. Two threads remained the default. [YUV trial](../demo/rust-benchmark-yuv.json) and [thread trials](../demo/rust-benchmark-threads.json) retain the intermediate raw results. Run-to-run scheduling affects timings.

Further tuning on September 21 enlarges the Linux MP4 pipe to 1 MiB (best effort, with the normal pipe as fallback) and uses GIF palette mapping only inside changed rectangles. Five alternating runs against the previous Rust renderer, both using the new 1.0-second motion, measured MP4 **0.901s → 0.725s** and MP4 + GIF **0.933s → 0.776s**. GIF-only changed by about 2%, within normal timing variation. Every resulting file was byte-identical between the matched-motion baseline and candidate, including six-row, empty, and 1fps checks. [Raw comparison](../demo/rust-buffer-benchmark.json), [individual optimization trials](../demo/rust-buffer-trials.json). The earlier tables retain the original 1.1-second motion measurements.
