#!/usr/bin/env python3
"""Run only under scripts/check's private Xvfb, never a real desktop display."""
import os
from pathlib import Path
import subprocess

root=Path(__file__).resolve().parent.parent
env=os.environ.copy()
# Require the temporary runtime/home created by the gate and Xvfb's auth file.
assert 'decklock-check-' in env.get('XDG_RUNTIME_DIR',''), 'Use scripts/check --all'
assert env.get('DISPLAY') and env.get('XAUTHORITY'), 'Private Xvfb required'
assert not env.get('WAYLAND_DISPLAY') and not env.get('WAYLAND_SOCKET')
env.update(GDK_BACKEND='x11',GSK_RENDERER='cairo',GTK_A11Y='none')
for name in ['help_check','animation_check','settings_check','settings_live_check','preview_check','media_check','video_live_check']:
    command=[str(root/'target/debug/examples'/name)]
    if name=='video_live_check':command.append(str(root/'assets/media/videos/osaka_dotombori.mp4'))
    subprocess.run(command,env=env,check=True,timeout=90)
subprocess.run(['python3',str(root/'scripts/test-controller-shortcut.py')],env=env,check=True,timeout=30)
print('PASS: isolated GTK contracts and fake controller shortcut')
