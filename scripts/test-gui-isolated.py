#!/usr/bin/env python3
"""Run only under scripts/check's private Xvfb, never a real desktop display."""
import ctypes
from contextlib import contextmanager
import os
from pathlib import Path
import subprocess
import time

root=Path(__file__).resolve().parent.parent
env=os.environ.copy()
# Require the temporary runtime/home created by the gate and Xvfb's auth file.
assert 'decklock-check-' in env.get('XDG_RUNTIME_DIR',''), 'Use scripts/check --all'
assert env.get('DISPLAY') and env.get('XAUTHORITY'), 'Private Xvfb required'
assert not env.get('WAYLAND_DISPLAY') and not env.get('WAYLAND_SOCKET')
env.update(GDK_BACKEND='x11',GSK_RENDERER='cairo',GTK_A11Y='none')

@contextmanager
def private_window_manager():
    """Require the WM to acknowledge fullscreen, rather than inspect a request flag."""
    xlib = ctypes.CDLL('libX11.so.6')
    xlib.XOpenDisplay.argtypes = [ctypes.c_char_p]
    xlib.XOpenDisplay.restype = ctypes.c_void_p
    xlib.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    xlib.XDefaultRootWindow.restype = ctypes.c_ulong
    xlib.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    xlib.XInternAtom.restype = ctypes.c_ulong
    xlib.XGetWindowProperty.argtypes = [
        ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long,
        ctypes.c_long, ctypes.c_int, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
        ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong),
        ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_void_p)]
    xlib.XFree.argtypes = [ctypes.c_void_p]
    xlib.XCloseDisplay.argtypes = [ctypes.c_void_p]
    display = xlib.XOpenDisplay(env['DISPLAY'].encode())
    assert display, 'Cannot connect to the private Xvfb'
    manager = None
    try:
        root_window = xlib.XDefaultRootWindow(display)
        atom = xlib.XInternAtom(display, b'_NET_SUPPORTING_WM_CHECK', 0)
        manager = subprocess.Popen(['xfwm4', '--compositor=off'], env=env)
        deadline = time.monotonic() + 5
        while True:
            actual_type, count, remaining = ctypes.c_ulong(), ctypes.c_ulong(), ctypes.c_ulong()
            actual_format = ctypes.c_int()
            data = ctypes.c_void_p()
            result = xlib.XGetWindowProperty(
                display, root_window, atom, 0, 1, 0, 0,
                ctypes.byref(actual_type), ctypes.byref(actual_format),
                ctypes.byref(count), ctypes.byref(remaining), ctypes.byref(data))
            ready = result == 0 and actual_format.value == 32 and count.value == 1
            if data.value:
                xlib.XFree(data)
            assert manager.poll() is None, 'Private xfwm4 exited before readiness'
            if ready:
                break
            assert time.monotonic() < deadline, 'Private xfwm4 did not publish its WM property'
            time.sleep(0.01)
        yield
    finally:
        if manager is not None:
            manager.terminate()
            try:
                manager.wait(timeout=3)
            except subprocess.TimeoutExpired:
                manager.kill()
                manager.wait(timeout=3)
        xlib.XCloseDisplay(display)

for name in ['help_check','lockout_check','animation_check','settings_check','settings_live_check','preview_check','media_check','video_live_check','guarantee_check','x11_video_check']:
    command=[str(root/'target/debug/examples'/name)]
    if name=='video_live_check':command.append(str(root/'assets/media/videos/osaka_dotombori.mp4'))
    # Without the x11 feature the sink has no X11 GL support, so this display
    # must reach the software path and still deliver frames.
    if name=='x11_video_check':command+=[str(root/'assets/media/videos/osaka_dotombori.mp4'),'software']
    if name == 'preview_check':
        with private_window_manager():
            subprocess.run(command,env=env,check=True,timeout=90)
    else:
        subprocess.run(command,env=env,check=True,timeout=90)
subprocess.run(['python3',str(root/'scripts/test-controller-shortcut.py')],env=env,check=True,timeout=30)
print('PASS: isolated GTK contracts and fake controller shortcut')
