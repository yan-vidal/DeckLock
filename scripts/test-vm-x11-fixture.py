#!/usr/bin/env python3
"""Check the VM's real X server survives an isolated readiness/grab probe."""
import ast
import ctypes
import ctypes.util
import os
from pathlib import Path
import select
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
command = next(ast.literal_eval(node.value) for node in
               ast.parse((ROOT / 'scripts/vm/exercise.py').read_text()).body
               if isinstance(node, ast.Assign) and any(
                   isinstance(target, ast.Name) and target.id == 'XVFB_COMMAND'
                   for target in node.targets))

xlib = ctypes.CDLL(ctypes.util.find_library('X11'))
xlib.XOpenDisplay.argtypes = [ctypes.c_char_p]
xlib.XOpenDisplay.restype = ctypes.c_void_p
xlib.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
xlib.XInternAtom.restype = ctypes.c_ulong
xlib.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
xlib.XCloseDisplay.argtypes = [ctypes.c_void_p]

with tempfile.TemporaryDirectory(prefix='decklock-vm-x11-') as folder:
    directory = Path(folder)
    env = os.environ.copy()
    for key in ['DISPLAY', 'WAYLAND_DISPLAY', 'WAYLAND_SOCKET', 'GREETD_SOCK',
                'DBUS_SESSION_BUS_ADDRESS', 'XAUTHORITY']:
        env.pop(key, None)
    for key in ['HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_RUNTIME_DIR']:
        path = directory / key.lower()
        path.mkdir(mode=0o700)
        env[key] = str(path)
    # Allocate our own display; never borrow :99 or an inherited desktop socket.
    read_fd, write_fd = os.pipe()
    with (directory / 'xvfb.log').open('w') as log:
        server = subprocess.Popen([command[0], '-displayfd', str(write_fd), *command[2:]],
                                  env=env, pass_fds=(write_fd,), stdout=log, stderr=log)
        os.close(write_fd)
        try:
            assert select.select([read_fd], [], [], 10)[0], 'Xvfb readiness timed out'
            number = os.read(read_fd, 64).strip()
            assert number.isdigit(), 'Xvfb exited without allocating a private display'
            display_name = b':' + number
            display = xlib.XOpenDisplay(display_name)
            assert display, 'Could not connect to the private VM fixture X server'
            marker = b'DECKLOCK_VM_FIXTURE_GENERATION'
            try:
                expected = xlib.XInternAtom(display, marker, 0)
                assert expected
                xlib.XSync(display, 0)
            finally:
                xlib.XCloseDisplay(display)
            # Like the VM grab probe, the only X client has disconnected. The
            # next GTK client must see the same server generation, not a reset.
            display = xlib.XOpenDisplay(display_name)
            assert display, 'The VM fixture reset while the next client connected'
            try:
                actual = xlib.XInternAtom(display, marker, 1)
                assert actual == expected, 'The VM X server reset after its last probe disconnected'
            finally:
                xlib.XCloseDisplay(display)
            print('PASS: VM X server preserves its generation between isolated client probes')
        finally:
            os.close(read_fd)
            server.terminate()
            try:
                server.wait(timeout=3)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
