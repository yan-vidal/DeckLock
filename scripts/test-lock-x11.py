#!/usr/bin/env python3
"""Exercise the X11 lock backend against a private Xvfb and xfwm4 only.

Never uses the desktop's display or Wayland socket, and issues no PAM request:
no password is ever typed into the real authentication path. Requires DeckLock
and examples/x11_intruder built with --features x11.

What only X11 can break is checked from the outside, as any other client sees it:
the window is override-redirect and covers the screen, another client cannot take
the keyboard, a client that keeps raising itself does not stay on top, focus comes
back after the window manager moves it, a screen blank does not release anything,
and a locker that cannot grab refuses instead of showing an unguarded password box.

It also records what X11 does not provide, so a future change cannot quietly claim
otherwise: keystrokes are readable by any client, and killing DeckLock unlocks.
"""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
# Built with --features x11 into their own directory: the default target/debug
# binary is what the package contract checks, and it must stay feature-free.
BINARY = ROOT / "target/x11/debug/decklock"
INTRUDER = ROOT / "target/x11/debug/examples/x11_intruder"
WIDTH, HEIGHT = 1280, 800
TITLE = "DeckLock"


class Failure(AssertionError):
    pass


def check(condition, message):
    if not condition:
        raise Failure(message)


def until(condition, seconds=8, alive=None, message="condition"):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = condition()
        if value:
            return value
        if alive is not None and alive.poll() is not None:
            raise Failure(f"DeckLock exited with {alive.returncode} while waiting for {message}")
        time.sleep(0.05)
    raise Failure(f"Timed out waiting for {message}")


class Session:
    """A private X server, its window manager and the helpers that poke at them."""

    def __init__(self, directory):
        self.directory = directory
        self.processes = []
        self.display = self.start_server()
        self.env = dict(os.environ)
        self.env.pop("WAYLAND_DISPLAY", None)
        self.env.pop("WAYLAND_SOCKET", None)
        self.env.update(DISPLAY=self.display, GDK_BACKEND="x11", GSK_RENDERER="cairo",
                        GTK_A11Y="none", XAUTHORITY=str(directory / "xauth"))
        self.window_manager()

    def start_server(self):
        for number in range(95, 100):
            display = f":{number}"
            if Path(f"/tmp/.X{number}-lock").exists():
                continue
            server = subprocess.Popen(
                ["Xvfb", display, "-screen", "0", f"{WIDTH}x{HEIGHT}x24", "-nolisten", "tcp"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.processes.append(server)
            for _ in range(100):
                if Path(f"/tmp/.X11-unix/X{number}").exists():
                    return display
                if server.poll() is not None:
                    break
                time.sleep(0.05)
        raise Failure("Could not start a private Xvfb on :95-:99")

    def window_manager(self):
        """A real session has one, and it is what moves X focus away from the lock."""
        self.processes.append(subprocess.Popen(
            ["xfwm4", "--compositor=off"], env=self.env,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(1.5)

    def intruder(self, *arguments, background=False):
        command = [str(INTRUDER), *[str(argument) for argument in arguments]]
        if background:
            process = subprocess.Popen(command, env=self.env, stdout=subprocess.PIPE, text=True)
            self.processes.append(process)
            return process
        return subprocess.run(command, env=self.env, capture_output=True, text=True,
                              timeout=60).stdout.strip()

    def report(self):
        fields = {}
        for pair in self.intruder("report", TITLE).split():
            key, _, value = pair.partition("=")
            fields[key] = value
        return fields

    def lock(self, config):
        process = subprocess.Popen([str(BINARY), "--lock", "--config", str(config)], env=self.env,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.processes.append(process)
        return process

    def close(self):
        for process in reversed(self.processes):
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


def write_config(directory):
    service = next((name for name in ("other", "login", "system-auth")
                    if (Path("/etc/pam.d") / name).is_file()), None)
    check(service, "No /etc/pam.d service available to satisfy the lock preflight")
    config = directory / "config.toml"
    config.write_text(f'pam_service = "{service}"\n')
    return config


def run():
    check(BINARY.is_file() and INTRUDER.is_file(),
          "Build with: cargo build --features x11 --target-dir target/x11 --bin decklock --example x11_intruder")
    with tempfile.TemporaryDirectory(prefix="decklock-x11-") as directory:
        directory = Path(directory)
        session = Session(directory)
        passed = []
        try:
            config = write_config(directory)
            locker = session.lock(config)

            def lock_window():
                fields = session.report()
                return fields if fields.get("found") == "true" else None

            until(lock_window, alive=locker, message="the lock window")

            def mapped():
                fields = session.report()
                return fields if fields.get("map_state") == "2" else None

            fields = until(mapped, alive=locker, message="the lock window to be mapped")
            check(fields["override_redirect"] == "true",
                  "The lock window is managed by the window manager, not override-redirect")
            check(fields["parent_is_root"] == "true", "The window manager reparented the lock window")
            check((fields["width"], fields["height"]) == (str(WIDTH), str(HEIGHT)),
                  f"The lock window does not cover the screen: {fields}")
            check(fields["topmost"] == "true", "The lock window is not the topmost window")
            xid = fields["xid"]
            passed.append("override-redirect window covering the screen")

            # Taking the grabs is a retry loop in the backend, so this waits for
            # them; each attempt here holds them only while the helper runs.
            grabs = until(lambda: session.intruder("grab") if "keyboard=1" in session.intruder("grab")
                          else None, alive=locker, message="DeckLock to hold the grabs")
            check("keyboard=1" in grabs and "pointer=1" in grabs,
                  f"Another client took the keyboard or pointer from the lock: {grabs}")
            passed.append("another client cannot take the keyboard or pointer")

            # What the #17 warning states, measured here so it cannot become false
            # silently: the grabs do not hide keystrokes from other clients.
            keylog = session.intruder("keylog", 3, background=True)
            time.sleep(0.5)
            session.intruder("type", "hunter")
            captured = keylog.communicate(timeout=30)[0]
            check("h u n t e r" in captured,
                  f"XI2 raw key events no longer expose typing; #17's warning needs review: {captured}")
            passed.append("keystrokes remain readable by any client (the reason for the warning)")

            # A window that covers the lock once and stays there: the lock screen
            # must come back over it and keep it there.
            coverer = session.intruder("cover", 4, background=True)
            recovered = until(lambda: session.intruder("stack", xid, 1) == "on_top=1 samples=1 other_top=0",
                              seconds=2, alive=locker, message="the lock screen back on top")
            check(recovered, "The lock screen stayed under a window that covered it")
            # xfwm4 answers one new window with more than one restack of its frame
            # (measured: a second one about 70 ms after the lock is back on top),
            # and each costs the lock one reaction, slower on a loaded machine.
            # Counting from the first recovery made that timing the test's outcome.
            # Instead the lock must hold the top for 0.5 s within 3 s, and from
            # then on keep it in every sample, not 19 of 20 as before.
            until(lambda: session.intruder("stack", xid, 10) == "on_top=10 samples=10 other_top=0",
                  seconds=3, alive=locker, message="the lock screen to hold the top once xfwm4 settles")
            stack = session.intruder("stack", xid, 20)
            check(stack == "on_top=20 samples=20 other_top=0", f"The lock screen did not stay on top: {stack}")
            covered = coverer.communicate(timeout=30)[0]
            check("mapped=" in covered, f"The covering window never appeared: {covered}")

            # A client raising itself every 200 ms wins the odd moment: X11 lets
            # any client raise a window, and the lock can only answer each time.
            # What is required is that it answers, and ends up on top.
            raiser = session.intruder("raise", 4, background=True)
            time.sleep(1)
            stack = session.intruder("stack", xid, 20)
            samples = dict(pair.split("=") for pair in stack.split())
            check(int(samples["on_top"]) >= 12,
                  f"A client raising itself kept the lock screen down: {stack}")
            focus = until(lambda: session.intruder("focus") == f"focus={xid}", seconds=5,
                          alive=locker, message="focus back on the lock window")
            check(focus, "The lock window never got the X input focus back")
            # An intruder that died early would make the checks above vacuous.
            raised = raiser.communicate(timeout=30)[0]
            check(int(dict(pair.split("=") for pair in raised.split())["raises"]) >= 5,
                  f"The intruder stopped raising itself, so it never competed: {raised}")
            back = until(lambda: session.intruder("stack", xid, 1) == "on_top=1 samples=1 other_top=0",
                         seconds=2, alive=locker, message="the lock screen on top after the raising stopped")
            check(back, "The lock screen did not end up on top once the raising stopped")
            passed.append("stacking and focus recovered from an intruding window")

            # Xvfb has no DPMS extension, so this blanks through the core screen
            # saver instead, and checks the server really did blank: a command that
            # silently does nothing would make the rest of this step meaningless.
            # Real monitor power-down on Xorg stays untested here.
            subprocess.run(["xset", "s", "activate"], env=session.env, check=True, timeout=10)
            check(session.intruder("blanked") == "state=1",
                  "The screen saver did not blank, so this step would prove nothing")
            time.sleep(0.5)
            subprocess.run(["xset", "s", "reset"], env=session.env, check=True, timeout=10)
            time.sleep(1.5)
            after = session.report()
            check(after.get("topmost") == "true" and after.get("focused") == "true",
                  f"The lock screen did not come back on top after a blank: {after}")
            grabs = session.intruder("grab")
            check("keyboard=1" in grabs and "pointer=1" in grabs,
                  f"The grabs were lost across a screen blank: {grabs}")
            passed.append("blanking and waking the screen releases nothing")

            locker.send_signal(signal.SIGTERM)
            check(locker.wait(timeout=5) == -signal.SIGTERM, "DeckLock did not die on SIGTERM")
            stderr = locker.stderr.read()
            check("Session lock failed" not in stderr, f"The lock had already failed: {stderr}")
            # X11 cannot keep a session locked without the locker. This asserts the
            # gap the guarantee declares, so code claiming otherwise fails here.
            after_death = session.intruder("grab")
            check("keyboard=0" in after_death,
                  f"Unexpected: the keyboard stayed grabbed after DeckLock died: {after_death}")
            passed.append("killing DeckLock unlocks, as Guarantees declares")

            holder = session.intruder("grab", 15, background=True)
            time.sleep(0.5)
            refused = subprocess.run([str(BINARY), "--lock", "--config", str(config)],
                                     env=session.env, capture_output=True, text=True, timeout=60)
            check(refused.returncode != 0,
                  "DeckLock pretended to lock while another client held the keyboard")
            check("Could not acquire session lock" in refused.stderr,
                  f"Refusal did not name the failure: {refused.stderr[-2000:]}")
            holder.wait(timeout=30)
            passed.append("refuses to lock when it cannot take the keyboard")

            print("PASS: " + "; ".join(passed))
        except BaseException:
            print("PASSED BEFORE FAILURE: " + "; ".join(passed), flush=True)
            raise
        finally:
            session.close()


if __name__ == "__main__":
    try:
        run()
    except Failure as failure:
        print(f"FAIL: {failure}", file=sys.stderr)
        raise SystemExit(1)
