#!/usr/bin/env python3
"""Ordinary Wayland window recording received key values, never the locker input."""
import os
from pathlib import Path
assert Path("/etc/decklock-test-vm").read_text().strip() == "disposable-qemu-fixture"
# compositor.py gives each compositor its own directory; exercise.py uses the default.
OUTPUT = Path(os.environ.get('DECKLOCK_PROBE_DIR', '/var/tmp/decklock-evidence'))
import gi
gi.require_version('Gtk', '4.0')
from gi.repository import Gtk
app = Gtk.Application(application_id='io.github.yan_vidal.DeckLock.VMProbe')
def activate(app):
    window = Gtk.ApplicationWindow(application=app, title='DeckLock input probe')
    window.set_default_size(640, 480)
    window.set_child(Gtk.Label(label='Input must not reach this window while locked'))
    key = Gtk.EventControllerKey()
    def pressed(controller, keyval, keycode, state):
        with (OUTPUT / 'probe-keys').open('a') as output:
            output.write(str(keyval) + '\n')
        return False
    key.connect('key-pressed', pressed)
    window.add_controller(key)
    # A compositor-neutral readiness signal; Sway's tree is only Sway's.
    window.connect('map', lambda _: (OUTPUT / 'probe-mapped').write_text('mapped\n'))
    window.present()
app.connect('activate', activate)
app.run()
