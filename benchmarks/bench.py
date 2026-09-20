"""Benchmark real rendering; `uv run benchmarks/bench.py --help`."""
import argparse
import importlib.util
import json
from pathlib import Path
import statistics
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from PIL import Image, ImageDraw
from release_balls import avatar, ranking, render

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--runs', type=int, default=3)
parser.add_argument('--out', type=Path, default=Path('out/bench'))
parser.add_argument('--reference', action='store_true', help='also measure supplied 1080p renderer at 30 fps for 3 seconds')
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
data = json.loads(Path('demo/data.json').read_text())
rows = ranking(data['releases'], 'author')
# Warm avatars independently; network excluded from render timings.
for login, _ in rows:
    avatar(login, Path('.cache/avatars'))
results = []
for strategy in ('cached', 'dirty'):
    for preset in ('ultrafast', 'superfast', 'veryfast'):
        times = []
        for run in range(args.runs):
            start = time.perf_counter()
            render(data['repo'], 'latest 5 nightlies / 19 listed PRs', rows, 'author', args.out / 'candidate', ['mp4'], preset=preset, strategy=strategy)
            times.append(time.perf_counter() - start)
        item = {'strategy': strategy, 'preset': preset, 'seconds': times, 'median_seconds': statistics.median(times), 'bytes': (args.out / 'candidate.mp4').stat().st_size}
        results.append(item)
        print(json.dumps(item), flush=True)
if args.reference:
    spec = importlib.util.spec_from_file_location('reference', Path(__file__).with_name('reference.py'))
    ref = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ref)
    ref.FPS, ref.TOTAL_FRAMES = 30, 90
    ref.PEOPLE = [dict(name=login, login=login, count=count, y=340 + i*250) for i, (login, count) in enumerate(rows[:3])]
    def prepare(_):
        mask = Image.new('L', (ref.BALL_RADIUS*4, ref.BALL_RADIUS*4))
        ImageDraw.Draw(mask).ellipse((0, 0, mask.width-1, mask.height-1), fill=255)
        from PIL import ImageOps
        for p in ref.PEOPLE:
            p['ball'] = ImageOps.fit(avatar(p['login'], Path('.cache/avatars')), mask.size)
        return mask
    ref.prepare_people = prepare
    times = []
    for run in range(args.runs):
        start = time.perf_counter()
        ref.render(args.out / 'reference.mp4')
        times.append(time.perf_counter()-start)
    results.append({'reference': 'supplied script, 1080p/30fps/3s, real top 3, warm avatars, medium/crf17', 'seconds': times, 'median_seconds': statistics.median(times), 'bytes': (args.out / 'reference.mp4').stat().st_size})
    times = []
    for run in range(args.runs):
        start = time.perf_counter()
        render(data['repo'], 'latest 5 nightlies / 19 listed PRs', rows[:3], 'author', args.out / 'matched', ['mp4'], width=1080, fps=30, duration=3, preset='ultrafast')
        times.append(time.perf_counter()-start)
    results.append({'reference': 'prototype matched 1080p/30fps/3s, same top 3; different layout, ultrafast/crf23', 'seconds': times, 'median_seconds': statistics.median(times), 'bytes': (args.out / 'matched.mp4').stat().st_size})
(args.out / 'results.json').write_text(json.dumps({'workload': '720p/30fps/6s, real latest 5 nightlies, 6 authors; avatar network excluded', 'runs': args.runs, 'results': results}, indent=2)+'\n')
print((args.out / 'results.json').read_text())
