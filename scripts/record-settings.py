#!/usr/bin/env python3
"""Record a synchronized GTK demo on an empty Hyprland workspace 3 at 1920x1080.

The Rust example changes real widgets only after this recorder acknowledges that
its pointer reached their bounds. No synthetic clicks, saving, or lock requests.
Requires a built settings_showcase example, grim, hyprctl, and ffmpeg.
"""
import json
import tomllib
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
CACHE = Path.home() / '.cache/decklock'
CACHE.mkdir(parents=True, exist_ok=True)
frames = Path(tempfile.mkdtemp(prefix='settings-fullhd-', dir=CACHE))
ipc = frames / 'pointer'

def run(args):
    return subprocess.run(args, check=True, stdout=subprocess.DEVNULL)

def query(command):
    return json.loads(subprocess.check_output(['hyprctl', command, '-j']))

def dispatch(command, value=None):
    run(['hyprctl', 'dispatch', command] + ([value] if value else []))

monitor = next(m for m in query('monitors') if m['focused'])
if (monitor['width'], monitor['height'], monitor['scale']) != (1920, 1080, 1):
    raise SystemExit('Recording requires a native 1920x1080 monitor at scale 1.')
if any(c['workspace']['id'] == 3 for c in query('clients')):
    raise SystemExit('Workspace 3 must be empty; existing user windows are never moved.')
previous_workspace = query('activeworkspace')['id']
origin_x, origin_y = monitor['x'], monitor['y']
old_cursor = query('cursorpos')
placed = set()
process = None
log = (frames / 'showcase.log').open('w')
timestamps = []
events = {}
try:
    dispatch('workspace', '3')
    dispatch('movecursor', f'{origin_x + 960} {origin_y + 540}')
    time.sleep(1.3)
    process = subprocess.Popen([
        str(ROOT / 'target/debug/examples/settings_showcase'),
        str(Path.home() / '.config/decklock/config.toml'), str(ipc)
    ], env=dict(os.environ, LD_LIBRARY_PATH=str(ROOT / '.deps/install/lib')), stdout=log, stderr=log)
    start = time.monotonic()
    stable = None
    last_step = None
    print(f'Recording workspace 3, Full HD. Frames and log: {frames}', flush=True)
    while time.monotonic() - start < 120 and not ipc.with_suffix('.done').exists():
        if process.poll() is not None:
            raise RuntimeError('Showcase exited early; inspect ' + str(frames / 'showcase.log'))
        if query('activeworkspace')['id'] != 3:
            raise RuntimeError('Workspace changed; stopping capture to avoid recording other windows.')
        clients = [c for c in query('clients') if c['pid'] == process.pid]
        for client in clients:
            address = 'address:' + client['address']
            if address in placed:
                continue
            title = client['title'].lower()
            if 'settings' in title:
                x, y, w, h = 20, 60, 820, 980
            elif 'preview' in title or 'prévia' in title:
                x, y, w, h = 860, 210, 1040, 650
            else:
                x, y, w, h = 35, 125, 795, 820
            dispatch('focuswindow', address)
            if not client['floating']:
                dispatch('togglefloating', address)
            dispatch('resizewindowpixel', f'exact {w} {h},{address}')
            dispatch('movewindowpixel', f'exact {origin_x+x} {origin_y+y},{address}')
            placed.add(address)
        if ipc.exists():
            message = tomllib.loads(ipc.read_text())
            step = message['step']
            if step != last_step:
                stable = None
                last_step = step
            client = next((c for c in query('clients') if c['pid'] == process.pid and c['title'] == message['title']), None)
            if client:
                target_x = client['at'][0] + message['x']
                target_y = client['at'][1] + message['y']
                current = query('cursorpos')
                dx, dy = target_x-current['x'], target_y-current['y']
                dispatch('movecursor', f'{round(current["x"] + dx*0.55)} {round(current["y"] + dy*0.55)}')
                if abs(dx) < 3 and abs(dy) < 3:
                    stable = stable or time.monotonic()
                    if time.monotonic()-stable > 0.35 and step not in events:
                        events[step] = time.monotonic()-start
                        ipc.with_suffix('.ack').write_text(str(step))
                        print(f'Action {step}: pointer at ({round(target_x)}, {round(target_y)})', flush=True)
                else:
                    stable = None
        frame_start = time.monotonic()
        run(['grim', '-c', '-l', '0', '-s', '1', '-o', monitor['name'], str(frames / f'{len(timestamps):04}.png')])
        timestamps.append(time.monotonic()-start)
        time.sleep(max(0, 0.125-(time.monotonic()-frame_start)))
    if not ipc.with_suffix('.done').exists():
        raise RuntimeError('Recording timed out; inspect showcase.log')
finally:
    if process is not None and process.poll() is None:
        process.terminate()
        process.wait(timeout=5)
    log.close()
    dispatch('workspace', str(previous_workspace))
    dispatch('movecursor', f'{old_cursor["x"]} {old_cursor["y"]}')

# Preserve timestamps: compositor capture time must not speed up or slow down playback.
lines = []
for i, timestamp in enumerate(timestamps):
    lines.append(f"file '{frames / f'{i:04}.png'}'")
    lines.append(f'duration {timestamps[i+1]-timestamp if i+1<len(timestamps) else 0.125:.4f}')
lines.append(f"file '{frames / f'{len(timestamps)-1:04}.png'}'")
(frames / 'frames.txt').write_text('\n'.join(lines)+'\n')
(frames / 'events.json').write_text(json.dumps(events, indent=2))
video = CACHE / 'settings-demo-fullhd.mp4'
run(['ffmpeg', '-v', 'error', '-y', '-f', 'concat', '-safe', '0', '-i', str(frames/'frames.txt'),
     '-vf', 'drawbox=x=0:y=0:w=iw:h=40:color=0x111111:t=fill,drawbox=x=0:y=1045:w=iw:h=35:color=0x111111:t=fill,fps=8', '-c:v', 'libx264', '-preset', 'fast', '-crf', '18', '-pix_fmt', 'yuv420p', '-an', '-movflags', '+faststart', str(video)])
# Full resolution master plus shorter demonstrations for focused README embeds.
segments = [('settings-demo', 0, timestamps[-1]),
            ('settings-themes', events[0], events[5]-events[0]),
            ('settings-rest', events[5], events[10]-events[5]),
            ('settings-editor', events[15], timestamps[-1]-events[15])]
for name, start, duration in segments:
    palette = frames / f'{name}-palette.png'
    source = ['ffmpeg', '-v', 'error', '-y', '-ss', str(start), '-t', str(duration), '-i', str(video)]
    run(source + ['-vf', 'fps=8,palettegen=max_colors=192', '-frames:v', '1', str(palette)])
    output = ROOT / 'docs/assets' / f'{name}.gif'
    run(source + ['-i', str(palette), '-lavfi', 'fps=8[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3', '-loop', '0', str(output)])
    print(f'{output}: {output.stat().st_size / 1024**2:.2f} MiB, 1920x1080', flush=True)
print(f'Video master: {video}\nSource frames retained: {frames}', flush=True)
