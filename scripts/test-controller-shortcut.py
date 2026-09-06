#!/usr/bin/env python3
"""Preview-only regression: existing Python shortcut routes to Rust, fake daemon only."""
import os
from pathlib import Path
import queue
import socket
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parent.parent


def wait_for(predicate, message):
    end = time.monotonic() + 10
    while time.monotonic() < end:
        if predicate():
            return
        time.sleep(0.05)
    raise AssertionError(message)


with tempfile.TemporaryDirectory(prefix="decklock-shortcut-") as tmp:
    home = Path(tmp)
    config = home / ".config"
    scc = config / "scc"
    scc.mkdir(parents=True)
    path = scc / "daemon.socket"
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(str(path))
    listener.listen(1)
    listener.settimeout(10)
    messages = queue.Queue()
    errors = queue.Queue()

    def daemon():
        try:
            with listener.accept()[0] as peer:
                peer.settimeout(10)
                peer.sendall(b"Controller: test deck 0 None\nReady.\n")
                for line in peer.makefile("rb"):
                    text = line.decode().strip()
                    messages.put(text)
                    peer.sendall(b"OK.\n")
        except Exception as error:
            errors.put(error)

    worker = threading.Thread(target=daemon, daemon=True)
    worker.start()
    env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(config), GTK_A11Y="none")
    env["LD_LIBRARY_PATH"] = str(ROOT / ".deps/install/lib") + ":" + env.get("LD_LIBRARY_PATH", "")
    proc = subprocess.Popen([str(ROOT / "target/debug/decklock"), "--preview", "--controller"], env=env)
    try:
        pidfile = scc / "deck-lock.pid"
        wait_for(pidfile.exists, "Rust did not register the shortcut")
        assert pidfile.read_text() == str(proc.pid)
        # Execute the original dispatcher with isolated HOME and a fake daemon.
        subprocess.run(["python3", str(ROOT / "deck_osk.py"), "--toggle"], env=env, check=True, timeout=10)
        seen = []

        def captured():
            while not messages.empty():
                seen.append(messages.get_nowait())
            return any(line.startswith("Lock: ") for line in seen)

        wait_for(captured, "Shortcut did not show/capture the embedded keyboard")
        assert not (scc / "ghost-osk.pid").exists(), "Shortcut spawned a separate OSK"
        subprocess.run(["python3", str(ROOT / "deck_osk.py"), "--toggle"], env=env, check=True, timeout=10)

        def released():
            while not messages.empty():
                seen.append(messages.get_nowait())
            return "Unlock." in seen

        wait_for(released, "Shortcut did not hide/release the embedded keyboard")
        assert proc.poll() is None
        assert errors.empty(), "Fake daemon failed"
        assert not (scc / "ghost-osk.pid").exists()
        print("PASS: Python shortcut toggles Rust embedded keyboard; fake capture/release; no separate OSK")
    finally:
        proc.terminate()
        proc.wait(timeout=5)
        listener.close()
        worker.join(timeout=2)
