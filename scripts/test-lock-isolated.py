#!/usr/bin/env python3
"""Exercise real lock protocol against gtk4-layer-shell's mock compositor only.

Never uses the desktop's Wayland socket. No PAM request or password input.
Requires the upstream 1.3 mock-server built under .deps/layer-build-1.3.
"""
import os
from pathlib import Path
import select
import signal
import socket as unix
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parent.parent
MOCK = ROOT / ".deps/layer-build-1.3/test/mock-server/mock-server"
BINARY = ROOT / "target/debug/decklock"


def until(check, seconds=8, alive=None):
    """Wait for a condition; a dead client is a failure, never a timeout."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if check():
            return
        if alive is not None and alive.poll() is not None:
            raise AssertionError(f"Lock client exited with {alive.returncode} while waiting")
        time.sleep(0.05)
    raise AssertionError("Timed out waiting for isolated compositor/client")


def read_response(fd, seconds=3):
    """Read one whole response line from the mock's FIFO.

    The mock writes the text and its newline in separate write() calls, so a
    partial read leaves the newline pending and shifts every later response by
    one. Skip stale newlines and only accept a line the mock finished writing.
    """
    buffer = b""
    deadline = time.monotonic() + seconds
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([fd], [], [], remaining)[0]:
            return buffer.decode(errors="replace").strip()
        buffer += os.read(fd, 1024)
        line = buffer.lstrip(b"\n")
        if b"\n" in line:
            return line.split(b"\n", 1)[0].decode()


def run():
    assert MOCK.is_file() and BINARY.is_file(), "Build DeckLock and upstream mock-server first"
    with tempfile.TemporaryDirectory(prefix="decklock-protocol-") as directory:
        root = Path(directory)
        socket = root / "gtkls-test-display"
        env = os.environ.copy()
        env.update(XDG_CONFIG_HOME=str(root / "config"), GTKLS_TEST_DIR=directory, WAYLAND_DISPLAY=str(socket), GDK_BACKEND="wayland",
                   GSK_RENDERER="cairo", GTK_A11Y="none", WAYLAND_DEBUG="1",
                   LD_LIBRARY_PATH=str(ROOT / ".deps/install/lib"))
        env.pop("WAYLAND_SOCKET", None)
        env.pop("DISPLAY", None)
        # Explicit empty config: never load the user's controller/media settings.
        config = root / "config.toml"
        config.write_text("")
        server_log = root / "server.log"
        client_log = root / "client.log"
        processes = []
        logs = []
        response_fd = None
        keepalive = None
        try:
            logs.append(server_log.open("w"))
            server = subprocess.Popen([str(MOCK)], env=env, stdout=logs[-1], stderr=logs[-1])
            processes.append(server)
            until(lambda: socket.exists() or server.poll() is not None)
            assert server.poll() is None, server_log.read_text()
            # Upstream mock exits when its last client disconnects. Keep one
            # inert connection so the post-SIGTERM refusal checks the same server.
            keepalive = unix.socket(unix.AF_UNIX, unix.SOCK_STREAM)
            keepalive.connect(str(socket))
            response = root / "gtkls-test-response"
            os.mkfifo(response, 0o600)
            response_fd = os.open(response, os.O_RDWR | os.O_NONBLOCK)

            def command(message, expected):
                fd = os.open(root / "gtkls-test-command", os.O_WRONLY | os.O_NONBLOCK)
                try:
                    os.write(fd, (message + "\n").encode())
                finally:
                    os.close(fd)
                reply = read_response(response_fd)
                assert reply == expected, (message, reply, server_log.read_text()[-2000:])

            logs.append(client_log.open("w"))
            debugger = ["gdb", "-batch", "-ex", "run", "-ex", "bt", "--args"] if os.getenv("DECKLOCK_TEST_GDB") else []
            client = subprocess.Popen([*debugger, str(BINARY), "--lock", "--config", str(config)], env=env,
                                      stdout=logs[-1], stderr=logs[-1])
            processes.append(client)
            until(lambda: ".locked(" in client_log.read_text() or client.poll() is not None)
            assert client.poll() is None, client_log.read_text()[-4000:]
            until(lambda: ".get_lock_surface(" in client_log.read_text(), alive=client)
            initial = client_log.read_text().count(".get_lock_surface(")
            command("create_output 800 600", "output_created")
            until(lambda: client_log.read_text().count(".get_lock_surface(") > initial, alive=client)
            command("destroy_output 1", "output_destroyed")
            time.sleep(.15)
            assert client.poll() is None, client_log.read_text()[-4000:]
            command("create_output 1024 768", "output_created")
            until(lambda: client_log.read_text().count(".get_lock_surface(") > initial + 1, alive=client)
            client.send_signal(signal.SIGTERM)
            assert client.wait(timeout=5) == -signal.SIGTERM
            assert ".unlock_and_destroy(" not in client_log.read_text(), "SIGTERM requested unlock"
            assert server.poll() is None, server_log.read_text()[-2500:]
            # The mock still owns the lock and must refuse another locker.
            denied = subprocess.run([str(BINARY), "--lock", "--config", str(config)], env=env,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=8)
            assert denied.returncode != 0, "Second client unexpectedly acquired the held lock"
            assert b"Session lock failed" in denied.stderr, (denied.stderr[-2000:], server_log.read_text()[-2500:])
            assert server.poll() is None, server_log.read_text()[-2000:]
            print("PASS: acquire, hotplug/remove/re-add, SIGTERM without unlock, second lock refused")
        except BaseException:
            print("MOCK COMPOSITOR LOG:\n" + server_log.read_text()[-16000:], flush=True)
            print("LOCK CLIENT LOG:\n" + client_log.read_text()[-24000:], flush=True)
            raise
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            if response_fd is not None:
                os.close(response_fd)
            if keepalive is not None:
                keepalive.close()
            for log in logs:
                log.close()


if __name__ == "__main__":
    run()
