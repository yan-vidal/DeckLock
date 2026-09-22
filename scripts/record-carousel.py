#!/usr/bin/env python3
"""Record a showcase GIF and screenshots of the DeckLock user carousel in 1280x800 (Steam Deck native).

Can be run directly on the desktop or headless via Xvfb:
    python3 scripts/record-carousel.py
"""
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT_DIR = ROOT / "docs" / "assets"
OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
READY_FILE = Path("/tmp/decklock_carousel_ready")

def run_capture(display_env=None):
    env = os.environ.copy()
    if display_env:
        env.update(display_env)
    env.update(
        GTK_A11Y="none",
        GSK_RENDERER="cairo",
        LD_LIBRARY_PATH=str(ROOT / ".deps/install/lib") + ":" + env.get("LD_LIBRARY_PATH", ""),
    )

    if READY_FILE.exists():
        READY_FILE.unlink()

    with tempfile.TemporaryDirectory(prefix="decklock-carousel-") as work_dir:
        work = Path(work_dir)
        video_raw = work / "carousel_raw.mp4"
        video_clean = work / "carousel_clean.mp4"
        display = env.get("DISPLAY", ":0")

        print(f"Starting ffmpeg screen grab on DISPLAY={display} at 1280x800...")
        ffmpeg_proc = subprocess.Popen(
            [
                "ffmpeg",
                "-v", "error",
                "-y",
                "-video_size", "1280x800",
                "-framerate", "24",
                "-f", "x11grab",
                "-i", display,
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-crf", "18",
                "-pix_fmt", "yuv420p",
                str(video_raw),
            ],
            env=env,
        )

        rec_start_time = time.monotonic()
        time.sleep(0.5)

        print("Running record_carousel example...")
        example_proc = subprocess.Popen(
            [str(ROOT / "target/debug/examples/record_carousel")],
            env=env,
        )

        # Wait for the example to signal that the window is painted
        ready_time = None
        while example_proc.poll() is None:
            if READY_FILE.exists():
                ready_time = time.monotonic() - rec_start_time
                print(f"DeckLock window ready at t={ready_time:.2f}s")
                break
            time.sleep(0.05)

        example_proc.wait(timeout=30)
        time.sleep(0.3)

        print("Stopping ffmpeg recording...")
        ffmpeg_proc.send_signal(signal.SIGINT)
        try:
            ffmpeg_proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            ffmpeg_proc.kill()
            ffmpeg_proc.wait()

        if not video_raw.exists() or video_raw.stat().st_size == 0:
            raise RuntimeError("Recording video is empty or was not created.")

        # Trim out the black startup frames so video begins cleanly with window presented
        start_trim = max(0.0, (ready_time or 1.8) + 0.1)
        print(f"Trimming video from t={start_trim:.2f}s...")
        subprocess.run(
            [
                "ffmpeg", "-v", "error", "-y",
                "-ss", str(start_trim),
                "-t", "5.8",
                "-i", str(video_raw),
                "-c:v", "libx264",
                "-crf", "18",
                "-pix_fmt", "yuv420p",
                str(video_clean),
            ],
            check=True,
        )

        print(f"Generating screenshots and GIFs into {OUTPUT_DIR}...")

        # 1. Fullscreen screenshot: initial user (Yan) at t=0.5s of clean video
        full_yan = OUTPUT_DIR / "user-carousel-yan.png"
        subprocess.run(
            [
                "ffmpeg", "-v", "error", "-y",
                "-ss", "00:00:00.600",
                "-i", str(video_clean),
                "-vframes", "1",
                str(full_yan),
            ],
            check=True,
        )
        print(f"Created: {full_yan}")

        # 2. Fullscreen screenshot: second user (Gravador) at t=2.6s of clean video
        full_gravador = OUTPUT_DIR / "user-carousel-gravador.png"
        subprocess.run(
            [
                "ffmpeg", "-v", "error", "-y",
                "-ss", "00:00:02.600",
                "-i", str(video_clean),
                "-vframes", "1",
                str(full_gravador),
            ],
            check=True,
        )
        print(f"Created: {full_gravador}")

        # Also copy full_yan as default user-carousel.png
        default_png = OUTPUT_DIR / "user-carousel.png"
        subprocess.run(["cp", str(full_yan), str(default_png)], check=True)

        # 3. Cropped zoom screenshots focusing on user carousel & login card
        # Center in 1280x800 is x=640, y=400.
        # Carousel + name + password box: width 680, height 360, x=300, y=290
        crop_filter = "crop=680:360:300:290"
        zoom_png = OUTPUT_DIR / "user-carousel-zoom.png"
        subprocess.run(
            [
                "ffmpeg", "-v", "error", "-y",
                "-ss", "00:00:00.600",
                "-i", str(video_clean),
                "-vf", crop_filter,
                "-vframes", "1",
                str(zoom_png),
            ],
            check=True,
        )
        print(f"Created: {zoom_png}")

        # 4. Full animated GIF (scale 800 width for crisp, compact display)
        full_gif = OUTPUT_DIR / "user-carousel.gif"
        palette = work / "palette.png"
        subprocess.run(
            [
                "ffmpeg", "-v", "error", "-y",
                "-i", str(video_clean),
                "-vf", "fps=14,scale=800:-1:flags=lanczos,palettegen=max_colors=192",
                str(palette),
            ],
            check=True,
        )
        subprocess.run(
            [
                "ffmpeg", "-v", "error", "-y",
                "-i", str(video_clean),
                "-i", str(palette),
                "-lavfi", "fps=14,scale=800:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3",
                "-loop", "0",
                str(full_gif),
            ],
            check=True,
        )
        print(f"Created: {full_gif}")

        # 5. Zoom animated GIF focusing on avatar carousel interaction
        zoom_gif = OUTPUT_DIR / "user-carousel-zoom.gif"
        palette_zoom = work / "palette_zoom.png"
        subprocess.run(
            [
                "ffmpeg", "-v", "error", "-y",
                "-i", str(video_clean),
                "-vf", f"{crop_filter},fps=14,scale=560:-1:flags=lanczos,palettegen=max_colors=192",
                str(palette_zoom),
            ],
            check=True,
        )
        subprocess.run(
            [
                "ffmpeg", "-v", "error", "-y",
                "-i", str(video_clean),
                "-i", str(palette_zoom),
                "-lavfi", f"{crop_filter},fps=14,scale=560:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3",
                "-loop", "0",
                str(zoom_gif),
            ],
            check=True,
        )
        print(f"Created: {zoom_gif}")

def main():
    # Recompile record_carousel example if needed
    subprocess.run(
        [str(ROOT / "scripts/cargo-local"), "build", "--example", "record_carousel"],
        cwd=ROOT,
        check=True,
    )

    if not os.environ.get("DECKLOCK_INSIDE_XVFB"):
        cmd = [
            "xvfb-run",
            "-a",
            "-s", "-screen 0 1280x800x24 -nolisten tcp",
            "dbus-run-session",
            "--",
            sys.executable,
            str(Path(__file__).resolve()),
            "--isolated",
        ]
        env = os.environ.copy()
        env["DECKLOCK_INSIDE_XVFB"] = "1"
        for k in ["WAYLAND_DISPLAY", "WAYLAND_SOCKET"]:
            env.pop(k, None)
        env["GDK_BACKEND"] = "x11"
        subprocess.run(cmd, env=env, check=True)
    else:
        run_capture()

if __name__ == "__main__":
    main()
