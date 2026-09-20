"""Compare Python and Rust rendering; `uv run benchmarks/rust_bench.py --help`."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from PIL import __version__ as pillow_version
from release_balls import avatar, ranking

ROOT = Path(__file__).resolve().parents[1]


def workload(value):
    try:
        width, fps, duration = value.split(":")
        width, fps, duration = int(width), int(fps), float(duration)
        if not (
            320 <= width <= 2160
            and width % 2 == 0
            and 1 <= fps <= 60
            and 0.1 <= duration <= 60
        ):
            raise ValueError
        return width, fps, duration
    except ValueError:
        raise argparse.ArgumentTypeError(
            "use WIDTH:FPS:SECONDS, even width 320–2160, fps 1–60, seconds 0.1–60"
        ) from None


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def version(*command):
    try:
        result = subprocess.run(
            command, cwd=ROOT, capture_output=True, text=True, timeout=15
        )
        return result.stdout.strip().splitlines()[0] if result.returncode == 0 else None
    except (OSError, subprocess.TimeoutExpired, IndexError):
        return None


def probe(path, width, fps, duration):
    result = subprocess.run(
        [
            "ffprobe", "-v", "error", "-select_streams", "v:0", "-count_frames",
            "-show_entries", "stream=width,height,nb_read_frames", "-of", "json",
            str(path),
        ],
        check=True, capture_output=True, text=True, timeout=120,
    )
    stream = json.loads(result.stdout)["streams"][0]
    frames = max(1, round(fps * duration))
    gif = path.suffix == ".gif"
    expected_width = 480 if gif else width
    expected_frames = round(frames / fps * 15) if gif else frames
    if (
        stream["width"] != expected_width
        or stream["height"] != expected_width
        or abs(int(stream["nb_read_frames"]) - expected_frames) > int(gif)
    ):
        raise RuntimeError(f"Unexpected dimensions or frame count: {path}: {stream}")
    return stream


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/release-balls")
    parser.add_argument("--data", type=Path, default=ROOT / "demo/data.json")
    parser.add_argument("--out", type=Path, default=ROOT / "out/rust-bench")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--formats", nargs="+", choices=["mp4", "gif", "both"], default=["mp4"])
    parser.add_argument(
        "--workloads", nargs="+", type=workload,
        default=[(720, 30, 6.0), (1080, 60, 16.0)],
        metavar="WIDTH:FPS:SECONDS",
    )
    parser.add_argument("--strategies", nargs="+", choices=["dirty", "cached", "yuv"], default=["dirty"])
    parser.add_argument(
        "--presets", nargs="+", choices=["ultrafast", "superfast", "veryfast"],
        default=["ultrafast"],
    )
    parser.add_argument("--threads", type=int, default=2, help="Rust encoder threads; Python stays at its original two")
    parser.add_argument("--probe", action="store_true", help="decode/count output frames with ffprobe, outside timing")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    args.binary, args.data, args.out = args.binary.resolve(), args.data.resolve(), args.out.resolve()
    if not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        parser.error("--binary must name an existing executable; build it before benchmarking")
    data = json.loads(args.data.read_text())
    rows = ranking(data["releases"], "merger", top=3)
    expected_ranking = [list(row) for row in rows]
    cache = ROOT / ".cache/avatars-512"
    avatar_paths = []
    for login, _ in rows:
        avatar(login, cache)
        path = cache / (re.sub(r"[^\w-]", "_", login) + ".png")
        if not path.is_file():
            raise RuntimeError(f"Could not warm {login}'s avatar; refusing to time network retries")
        avatar_paths.append(path)
    sources = [ROOT / name for name in ("release_balls.py", "data.py", "Cargo.toml", "Cargo.lock", "benchmarks/rust_bench.py")]
    sources.extend(sorted((ROOT / "src").rglob("*.rs")))
    report = {
        "measured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "method": "alternating Python/Rust order; subprocess startup included; shared warm avatars; no installation, API collection, or probing in timings",
        "environment": {
            "platform": platform.platform(), "machine": platform.machine(),
            "cpu_count": os.cpu_count(),
            "available_cpus": len(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
            "python": sys.version, "pillow": pillow_version,
            "ffmpeg": version("ffmpeg", "-version"),
            "ffprobe": version("ffprobe", "-version"),
            "rustc": version("rustc", "--version"),
            "cargo": version("cargo", "--version"),
        },
        "source": {
            "head": version("git", "rev-parse", "HEAD"),
            "sha256": {str(path.relative_to(ROOT)): digest(path) for path in sources if path.is_file()},
            "binary": str(args.binary), "binary_sha256": digest(args.binary),
            "data": str(args.data), "data_sha256": digest(args.data),
            "avatars_sha256": {path.name: digest(path) for path in avatar_paths},
        },
        "repo": data["repo"], "release_tags": [r["tag"] for r in data["releases"]],
        "metric": "merger", "top": 3, "ranking": expected_ranking,
        "runs": args.runs, "rust_threads": args.threads, "probe": args.probe, "completed": False, "results": [],
    }
    args.out.mkdir(parents=True, exist_ok=True)
    report_path = args.out / "results.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    engines = {
        "python": [sys.executable, str(ROOT / "release_balls.py")],
        "rust": [str(args.binary)],
    }
    case_index = 0
    for width, fps, duration in args.workloads:
        for format_name in args.formats:
            for strategy in args.strategies:
                for preset in args.presets:
                    case = f"{width}-{fps}-{duration:g}-{format_name}-{strategy}-{preset}"
                    result = {
                        "width": width, "fps": fps, "duration": duration,
                        "format": format_name, "strategy": strategy, "python_strategy": "dirty" if strategy == "yuv" else strategy, "preset": preset,
                        "samples": [],
                    }
                    for run in range(args.runs):
                        order = ["python", "rust"] if (run + case_index) % 2 == 0 else ["rust", "python"]
                        for position, engine in enumerate(order):
                            output = args.out / case / engine
                            output.mkdir(parents=True, exist_ok=True)
                            formats = ["mp4", "gif"] if format_name == "both" else [format_name]
                            paths = [output / f"summary-merger.{fmt}" for fmt in formats]
                            for path in [*paths, output / "manifest.json"]:
                                path.unlink(missing_ok=True)
                            command = engines[engine] + [
                                "--data", str(args.data), "--metric", "merger", "--top", "3",
                                "--width", str(width), "--fps", str(fps), "--duration", str(duration),
                                "--format", format_name, "--strategy", ("dirty" if engine == "python" and strategy == "yuv" else strategy), "--preset", preset,
                                "--out", str(output),
                            ]
                            if engine == "rust":
                                command += ["--threads", str(args.threads)]
                            start = time.perf_counter()
                            process = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=600)
                            elapsed = time.perf_counter() - start
                            if process.returncode:
                                raise RuntimeError(f"{engine} failed ({process.returncode}): {process.stderr}")
                            manifest = json.loads((output / "manifest.json").read_text())
                            if manifest["artifacts"][0]["ranking"] != expected_ranking:
                                raise RuntimeError(f"{engine} ranked different contributors: {manifest}")
                            sizes = {path.suffix[1:]: path.stat().st_size for path in paths}
                            if not all(sizes.values()):
                                raise RuntimeError(f"{engine} produced an empty output: {sizes}")
                            sample = {"run": run + 1, "position": position + 1, "engine": engine, "seconds": elapsed, "bytes": sizes}
                            if args.probe:
                                sample["streams"] = {path.suffix[1:]: probe(path, width, fps, duration) for path in paths}
                            result["samples"].append(sample)
                            print(json.dumps({"case": case, **sample}), flush=True)
                    medians = {
                        engine: statistics.median(sample["seconds"] for sample in result["samples"] if sample["engine"] == engine)
                        for engine in engines
                    }
                    result.update({
                        "median_seconds": medians,
                        "rust_minus_python_seconds": medians["rust"] - medians["python"],
                        "rust_minus_python_percent": (medians["rust"] / medians["python"] - 1) * 100,
                        "python_over_rust_speedup": medians["python"] / medians["rust"],
                    })
                    report["results"].append(result)
                    report_path.write_text(json.dumps(report, indent=2) + "\n")
                    case_index += 1
    report["completed"] = True
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(report_path)


if __name__ == "__main__":
    main()
