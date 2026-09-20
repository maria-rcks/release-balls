#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=12"]
# ///

"""Create the square PR merge-speed animation.

Run:
    uv run make_pr_merge_video.py

Optional output path:
    uv run make_pr_merge_video.py --out my-video.mp4

Requires ffmpeg to be installed and available on PATH.
"""

import argparse
import shutil
import subprocess
import tempfile
import urllib.request
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageOps

W = H = 1080
FPS = 60
DURATION = 16
TOTAL_FRAMES = FPS * DURATION
BALL_RADIUS = 76
LEFT = 350
RIGHT = 980
FONT_PATH = "/System/Library/Fonts/HelveticaNeue.ttc"

PEOPLE = [
    {"name": "Maria", "login": "maria-rcks", "count": 69, "y": 340},
    {"name": "Theo", "login": "t3dotgg", "count": 41, "y": 590},
    {"name": "Julius", "login": "juliusmarminge", "count": 3, "y": 840},
]


def load_font(size: int) -> ImageFont.FreeTypeFont:
    if Path(FONT_PATH).exists():
        return ImageFont.truetype(FONT_PATH, size=size, index=0)
    # Portable fallback for non-macOS systems with Pillow's bundled font.
    return ImageFont.load_default(size=size)


def download_avatar(login: str, destination: Path) -> None:
    url = f"https://github.com/{login}.png?size=512"
    request = urllib.request.Request(url, headers={"User-Agent": "pr-merge-video/1.0"})
    with urllib.request.urlopen(request, timeout=30) as response:
        destination.write_bytes(response.read())


def triangle_wave(distance: float, span: float) -> float:
    period = span * 2
    position = distance % period
    return position if position <= span else period - position


def horizontal_position(frame: int, count: int) -> float:
    # Exact linear speed ratio from the five-day merge counts.
    distance = count * 7.2 * frame / FPS
    return LEFT + triangle_wave(distance, RIGHT - LEFT)


def prepare_people(temp_dir: Path) -> Image.Image:
    diameter = BALL_RADIUS * 4  # 2x render scale, 2 radii per diameter.
    mask = Image.new("L", (diameter, diameter), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, diameter - 1, diameter - 1), fill=255)

    for person in PEOPLE:
        avatar_path = temp_dir / f'{person["login"]}.png'
        download_avatar(person["login"], avatar_path)
        source = Image.open(avatar_path).convert("RGB")
        person["ball"] = ImageOps.fit(
            source,
            (diameter, diameter),
            method=Image.Resampling.LANCZOS,
        )
    return mask


def draw_frame(frame: int, avatar_mask: Image.Image) -> Image.Image:
    scale = 2
    canvas = Image.new("RGB", (W * scale, H * scale), "white")
    draw = ImageDraw.Draw(canvas)

    title_font = load_font(48 * scale)
    name_font = load_font(44 * scale)
    count_font = load_font(25 * scale)

    title = "Pull requests merged in the past 5 days"
    title_box = draw.textbbox((0, 0), title, font=title_font)
    title_width = title_box[2] - title_box[0]
    draw.text(
        ((W * scale - title_width) / 2, 90 * scale),
        title,
        font=title_font,
        fill="black",
    )

    for person in PEOPLE:
        y = person["y"]
        draw.text(
            (70 * scale, (y - 43) * scale),
            person["name"],
            font=name_font,
            fill="black",
        )
        draw.text(
            (72 * scale, (y + 17) * scale),
            f'{person["count"]} merges',
            font=count_font,
            fill="black",
        )

        x = horizontal_position(frame, person["count"])
        canvas.paste(
            person["ball"],
            (
                round((x - BALL_RADIUS) * scale),
                round((y - BALL_RADIUS) * scale),
            ),
            avatar_mask,
        )

    return canvas.resize((W, H), Image.Resampling.LANCZOS)


def render(output: Path) -> None:
    if not shutil.which("ffmpeg"):
        raise SystemExit("ffmpeg is required but was not found on PATH")

    output = output.expanduser().resolve()
    output.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="pr-merge-video-") as temp:
        avatar_mask = prepare_people(Path(temp))
        command = [
            "ffmpeg",
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            f"{W}x{H}",
            "-r",
            str(FPS),
            "-i",
            "-",
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "17",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            str(output),
        ]

        process = subprocess.Popen(command, stdin=subprocess.PIPE)
        try:
            for frame in range(TOTAL_FRAMES):
                process.stdin.write(draw_frame(frame, avatar_mask).tobytes())
        finally:
            if process.stdin:
                process.stdin.close()

        if process.wait() != 0:
            raise SystemExit("ffmpeg failed while rendering the video")

    print(f"Created {output}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("pr-merge-balls-pfps.mp4"),
        help="Output MP4 path",
    )
    args = parser.parse_args()
    render(args.out)


if __name__ == "__main__":
    main()
