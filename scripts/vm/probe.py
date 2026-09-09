#!/usr/bin/env python3
"""Ordinary Wayland window recording received key values, never the locker input."""
from pathlib import Path
assert Path("/etc/decklock-test-vm").read_text().strip() == "disposable-qemu-fixture"
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
        with Path('/var/tmp/decklock-evidence/probe-keys').open('a') as output:
            output.write(str(keyval) + '\n')
        return False
    key.connect('key-pressed', pressed)
    window.add_controller(key)
    window.present()
app.connect('activate', activate)
app.run()
