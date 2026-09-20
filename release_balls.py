"""Render real release contributions. Run `uv run release-balls --help`."""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import urllib.parse
import urllib.request

from PIL import Image, ImageDraw, ImageFont, ImageOps

from data import collect


def ranking(releases, metric, users=(), bots=False, top=6):
    counts = Counter()
    seen = set()
    for release in releases:
        for pr in release["prs"]:
            if pr["number"] in seen:
                continue
            seen.add(pr["number"])
            login = pr["merger" if metric == "merger" else "author"]
            if not login or (not bots and login.endswith("[bot]")):
                continue
            if users and login.casefold() not in {u.casefold() for u in users}:
                continue
            counts[login] += (
                pr["additions"] + pr["deletions"] if metric == "changes" else 1
            )
    return sorted(counts.items(), key=lambda item: (-item[1], item[0].casefold()))[:top]


def avatar(login, cache):
    cache.mkdir(parents=True, exist_ok=True)
    path = cache / (re.sub(r"[^\w-]", "_", login) + ".png")
    if not path.exists():
        try:
            request = urllib.request.Request(
                f"https://github.com/{urllib.parse.quote(login)}.png?size=512",
                headers={"User-Agent": "release-balls"},
            )
            with urllib.request.urlopen(request, timeout=15) as response:
                content = response.read(2_000_000)
            from io import BytesIO

            source = Image.open(BytesIO(content)).convert("RGB")
            source.save(path)
        except (OSError, ValueError):
            # Network failure remains visible as initials; never invent an avatar.
            image = Image.new("RGB", (128, 128), "#e4e4e7")
            ImageDraw.Draw(image).text(
                (28, 42),
                login[:2].upper(),
                font=ImageFont.load_default(size=36),
                fill="black",
            )
            return image
    with Image.open(path) as image:
        return image.convert("RGB")


def load_font(size):
    path = Path("/System/Library/Fonts/HelveticaNeue.ttc")
    return (
        ImageFont.truetype(str(path), size=size, index=0)
        if path.exists()
        else ImageFont.load_default(size=size)
    )


def artwork(repo, subtitle, rows, metric, width, cache):
    # Keep the supplied script's composition; only cache its static artwork.
    scale = width * 2 / 1080

    def n(value):
        return round(value * scale)

    canvas = Image.new("RGB", (width * 2, width * 2), "white")
    draw = ImageDraw.Draw(canvas)
    title = (
        ("Lines changed" if metric == "changes" else "Pull requests merged")
        + " in "
        + subtitle
    )
    title_font = load_font(n(48))
    while draw.textlength(title, font=title_font) > n(1000):
        title_font = load_font(title_font.size - 1)
    title_box = draw.textbbox((0, 0), title, font=title_font)
    draw.text(
        ((width * 2 - (title_box[2] - title_box[0])) / 2, n(90)),
        title,
        font=title_font,
        fill="black",
    )
    sprites = []
    row_height = 250 if len(rows) <= 3 else 500 / (len(rows) - 1)
    with ThreadPoolExecutor(max_workers=6) as pool:
        sources = list(pool.map(lambda row: avatar(row[0], cache), rows))
    for index, ((login, count), source) in enumerate(zip(rows, sources)):
        y = 340 + index * row_height
        radius = round(min(76, row_height * 0.4) * width / 1080)
        name_font = load_font(n(44))
        while draw.textlength(login, font=name_font) > n(190):
            name_font = load_font(name_font.size - 1)
        draw.text((n(70), n(y - 43)), login, font=name_font, fill="black")
        draw.text(
            (n(72), n(y + 17)),
            f"{count:,} " + ("lines" if metric == "changes" else "merges"),
            font=load_font(n(25)),
            fill="black",
        )
        diameter = radius * 2
        ball = ImageOps.fit(
            source, (diameter, diameter), method=Image.Resampling.LANCZOS
        )
        mask = Image.new("L", (diameter * 2, diameter * 2))
        ImageDraw.Draw(mask).ellipse(
            (0, 0, diameter * 2 - 1, diameter * 2 - 1), fill=255
        )
        sprites.append(
            (
                ball,
                mask.resize((diameter, diameter), Image.Resampling.LANCZOS),
                round(y * width / 1080),
                radius,
                count,
            )
        )
    if not rows:
        draw.text(
            (n(70), n(340)),
            "No matching contributions",
            font=load_font(n(44)),
            fill="black",
        )
    return canvas.resize((width, width), Image.Resampling.LANCZOS), sprites


def render(
    repo,
    subtitle,
    rows,
    metric,
    output,
    formats,
    width=1080,
    fps=60,
    duration=16,
    preset="ultrafast",
    strategy="dirty",
    cache=Path(".cache/avatars-512"),
):
    background, sprites = artwork(repo, subtitle, rows, metric, width, cache)
    command = [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-s",
        f"{width}x{width}",
        "-r",
        str(fps),
        "-i",
        "-",
        "-an",
    ]
    if "mp4" in formats:
        command += [
            "-c:v",
            "libx264",
            "-threads",
            "2",
            "-preset",
            preset,
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            str(output) + ".mp4",
        ]
    if "gif" in formats:
        command += [
            "-filter_threads",
            "1",
            "-vf",
            "fps=15,scale=480:-1:flags=bilinear,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=none",
            "-loop",
            "0",
            str(output) + ".gif",
        ]
    output.parent.mkdir(parents=True, exist_ok=True)
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
    canvas = background.copy()
    previous = []
    left, span = round(width * 350 / 1080), round(width * 630 / 1080)
    try:
        for frame in range(max(1, round(fps * duration))):
            if strategy == "cached":
                canvas = background.copy()
            else:
                for box in previous:
                    canvas.paste(background.crop(box), box)
            previous = []
            for ball, mask, y, radius, count in sprites:
                distance = count * 7.2 * (width / 1080) * frame / fps
                phase = distance % (2 * span)
                x = left + round(phase if phase <= span else 2 * span - phase)
                canvas.paste(ball, (x - radius, y - radius), mask)
                previous.append((x - radius, y - radius, x + radius, y + radius))
            process.stdin.write(canvas.tobytes())
    except BaseException:
        process.stdin.close()
        error = process.stderr.read().decode()
        process.wait()
        if error:
            print(error, file=__import__("sys").stderr)
        raise
    process.stdin.close()
    error = process.stderr.read().decode()
    if process.wait():
        raise RuntimeError(f"ffmpeg failed: {error}")
    return [str(output) + "." + fmt for fmt in formats]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo", default=os.environ.get("GITHUB_REPOSITORY", "pingdotgg/t3code")
    )
    parser.add_argument("--releases", type=int, default=5)
    parser.add_argument("--match", default="nightly", help="substring in release tags")
    parser.add_argument(
        "--metric", choices=["author", "merger", "changes"], default="author"
    )
    parser.add_argument("--users", default="", help="comma-separated GitHub logins")
    parser.add_argument("--include-bots", action="store_true")
    parser.add_argument("--top", type=int, default=3)
    parser.add_argument("--format", default="mp4", choices=["mp4", "gif", "both"])
    parser.add_argument("--per-release", action="store_true")
    parser.add_argument(
        "--data", type=Path, help="replay a saved data.json without GitHub API calls"
    )
    parser.add_argument("--out", type=Path, default=Path("out"))
    parser.add_argument(
        "--asset-base-url",
        default="",
        help="public URL directory for release-note embeds",
    )
    parser.add_argument("--width", type=int, default=1080)
    parser.add_argument("--fps", type=int, default=60)
    parser.add_argument("--duration", type=float, default=16)
    parser.add_argument(
        "--preset", choices=["ultrafast", "superfast", "veryfast"], default="ultrafast"
    )
    parser.add_argument("--strategy", choices=["cached", "dirty"], default="dirty")
    args = parser.parse_args()
    if not (
        1 <= args.releases <= 100
        and 1 <= args.top <= 6
        and 320 <= args.width <= 2160
        and args.width % 2 == 0
        and 1 <= args.fps <= 60
        and 0.1 <= args.duration <= 60
    ):
        parser.error(
            "use 1–100 releases, 1–6 rows, even width 320–2160, fps 1–60, duration 0.1–60"
        )
    if not shutil.which("ffmpeg"):
        parser.error("ffmpeg is required; install it using your system package manager")
    start = time.perf_counter()
    data = (
        json.loads(args.data.read_text())
        if args.data
        else collect(args.repo, args.releases, args.match)
    )
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "data.json").write_text(json.dumps(data, indent=2) + "\n")
    formats = ["mp4", "gif"] if args.format == "both" else [args.format]
    groups = (
        [[release] for release in data["releases"]]
        if args.per_release
        else [data["releases"]]
    )
    artifacts = []
    for releases in groups:
        subtitle = (
            releases[0]["tag"]
            if len(releases) == 1
            else f"the last {len(releases)} releases"
        )
        stem = (
            re.sub(r"[^\w.-]", "-", releases[0]["tag"])
            if args.per_release
            else "summary"
        )
        rows = ranking(
            releases,
            args.metric,
            [u.strip() for u in args.users.split(",") if u.strip()],
            args.include_bots,
            args.top,
        )
        paths = render(
            data["repo"],
            subtitle,
            rows,
            args.metric,
            args.out / f"{stem}-{args.metric}",
            formats,
            args.width,
            args.fps,
            args.duration,
            args.preset,
            args.strategy,
        )
        artifacts.append(
            {"releases": [r["tag"] for r in releases], "ranking": rows, "files": paths}
        )
    manifest = {
        "metric": args.metric,
        "elapsed_seconds": round(time.perf_counter() - start, 3),
        "artifacts": artifacts,
    }
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    lines = []
    for artifact in artifacts:
        for path in artifact["files"]:
            name = Path(path).name
            url = (
                args.asset_base_url.rstrip("/") + "/" + urllib.parse.quote(name)
                if args.asset_base_url
                else name
            )
            label = f"{args.metric} contributions ({', '.join(artifact['releases'])})"
            lines.append(
                f"![{label}]({url})"
                if name.endswith(".gif")
                else f"[{label} video]({url})"
            )
    (args.out / "embed.md").write_text("\n\n".join(lines) + "\n")
    print(json.dumps(manifest, indent=2))
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as target:
            target.write(
                f"markdown={(args.out / 'embed.md').resolve()}\ndirectory={args.out.resolve()}\nmanifest={(args.out / 'manifest.json').resolve()}\ndata={(args.out / 'data.json').resolve()}\n"
            )


if __name__ == "__main__":
    main()
