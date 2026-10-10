#!/usr/bin/env python3
"""Exercise the VM runner's public CPU plan without starting or inspecting a VM."""
import ast
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parent.parent
SCRATCH = ROOT / "target/dev-arm"
# The one profile an emulated aarch64 guest gets: pointer authentication stays
# enabled, with QEMU's implementation-defined algorithm instead of QARMA5.
EMULATED_ARM = "max,pauth=on,pauth-impdef=on,pauth-qarma3=off"
# Every suite and the phase marks the guest prints when it has finished. Guests that
# share the suites must be told which, and held to exactly those.
ALL_SUITES = ("package", "session", "compositors", "greeter")
MARKS = {"package": ["candidate_reinstalled"], "session": ["sway_pam_x11_complete"],
         "compositors": ["labwc_complete", "wayfire_complete"], "greeter": ["greeter_complete"]}

# Runs in the CLI subprocess, intercepting host boundaries rather than runner
# helpers. PATH stubs also keep accidental external commands in the fixture.
HOST_GUARD = textwrap.dedent('''\
    import builtins
    import io
    import os
    from pathlib import Path
    import socket
    import subprocess

    def forbidden(operation):
        with original_open(os.environ["VM_PLAN_SENTINEL"], "a") as stream:
            stream.write(operation + "\\n")
        raise RuntimeError("Plan crossed a host boundary: " + operation)

    original_open = builtins.open
    original_io_open = io.open
    original_access = os.access
    protected = {"/dev/kvm", "/proc/meminfo", "/proc/pressure/memory"}

    def guarded_open(original):
        def call(path, *args, **kwargs):
            if isinstance(path, (str, bytes, os.PathLike)):
                if os.fsdecode(path) in protected:
                    forbidden("read " + os.fsdecode(path))
            return original(path, *args, **kwargs)
        return call

    def guarded_access(path, *args, **kwargs):
        if os.fsdecode(path) == "/dev/kvm":
            forbidden("inspect /dev/kvm")
        return original_access(path, *args, **kwargs)

    def guarded_process(*args, **kwargs):
        forbidden("start subprocess")

    def guarded_socket(*args, **kwargs):
        forbidden("create socket")

    builtins.open = guarded_open(original_open)
    io.open = guarded_open(original_io_open)
    os.access = guarded_access
    subprocess.Popen = guarded_process
    socket.socket = guarded_socket
''')


def pinned_distros():
    source = ast.parse((ROOT / "scripts/test-vm.py").read_text())
    return next(ast.literal_eval(node.value) for node in source.body
                if isinstance(node, ast.Assign)
                and any(isinstance(name, ast.Name) and name.id == "DISTROS"
                        for name in node.targets))


def contents(root):
    return {str(path.relative_to(root)): path.read_bytes() if path.is_file() else None
            for path in root.rglob("*")}


class VmEmulationCLI(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.distros = pinned_distros()

    def invoke(self, *arguments):
        with tempfile.TemporaryDirectory(prefix="plan-", dir=SCRATCH) as directory:
            work = Path(directory)
            checkout = work / "checkout"
            (checkout / "scripts").mkdir(parents=True)
            for name in ("test-vm.py", "vm_native.py"):
                shutil.copy2(ROOT / "scripts" / name, checkout / "scripts" / name)
            shutil.copytree(ROOT / "scripts/vm/distros", checkout / "scripts/vm/distros")
            guard = work / "guard"
            guard.mkdir()
            (guard / "sitecustomize.py").write_text(HOST_GUARD)
            tools = work / "bin"
            tools.mkdir()
            for name in ("qemu-system-aarch64", "qemu-system-x86_64", "qemu-img",
                         "genisoimage", "curl", "ssh", "scp", "ssh-keygen", "git",
                         "sudo", "unshare", "chroot", "mount", "losetup", "growpart",
                         "e2fsck", "resize2fs", "timeout"):
                stub = tools / name
                stub.write_text('#!/bin/sh\nprintf "%s\\n" "$0" >> "$VM_PLAN_SENTINEL"\nexit 91\n')
                stub.chmod(0o755)
            sentinel = work / "external-calls"
            environment = dict(os.environ, HOME=str(work / "home"),
                               PATH=str(tools), PYTHONPATH=str(guard),
                               PYTHONDONTWRITEBYTECODE="1", VM_PLAN_SENTINEL=str(sentinel))
            for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
                         "XDG_STATE_HOME", "XDG_RUNTIME_DIR"):
                environment[name] = str(work / name)
            for name in ("WAYLAND_DISPLAY", "DISPLAY", "DBUS_SESSION_BUS_ADDRESS",
                         "SSH_AUTH_SOCK", "GREETD_SOCK"):
                environment.pop(name, None)
            before = contents(work)
            result = subprocess.run([sys.executable, "-B", str(checkout / "scripts/test-vm.py"),
                                     *arguments], cwd=work, env=environment,
                                    capture_output=True, text=True, timeout=10)
            self.assertFalse(sentinel.exists(),
                             "Planning and validation must not inspect KVM, start processes or connect")
            self.assertEqual(contents(work), before,
                             "Planning and validation must not create logs, caches or runtime files")
            return result

    def plan(self, target, arch, *arguments):
        result = self.invoke("--target", target, "--arch", arch, "--print-plan", *arguments)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        return json.loads(result.stdout)

    def expected(self, target, arch, suites=ALL_SUITES):
        distro = self.distros[target] | self.distros[target].get(arch, {})
        return {"target": target, "arch": arch, "image": distro["image"],
                "image_sha256": distro["sha256"], "memory_mib": 2048,
                "suites": list(suites),
                "completion_marks": [mark for suite in suites for mark in MARKS[suite]],
                "cpu_models": {"kvm": "host", "tcg": EMULATED_ARM if arch == "aarch64" else "max"},
                "cpus": {"kvm": 2, "tcg": 4}, "slowdown": {"kvm": 1, "tcg": 8}}

    def test_emulated_arm_keeps_pauth_with_the_implementation_defined_algorithm(self):
        expected = self.expected("ubuntu", "aarch64")
        self.assertEqual(expected["cpu_models"], {"kvm": "host", "tcg": EMULATED_ARM})
        self.assertEqual(self.plan("ubuntu", "aarch64"), expected)
        self.assertEqual(self.plan("ubuntu", "aarch64", "--allow-emulation"), expected)

    def test_x86_plan_keeps_each_distribution_pin_and_cpu(self):
        for target in ("arch", "fedora", "ubuntu"):
            with self.subTest(target=target):
                plan = self.plan(target, "x86_64")
                self.assertEqual(plan, self.expected(target, "x86_64"))
                self.assertEqual(plan["cpu_models"], {"kvm": "host", "tcg": "max"})

    def test_plan_needs_neither_a_package_nor_permission_to_emulate(self):
        # Execution flags and nonexistent paths must not activate their effects.
        self.assertEqual(self.plan("ubuntu", "aarch64", "--provision-native",
                                   "--package", "absent-candidate.deb",
                                   "--package-fetch", "touch fetched-by-plan; exit 7",
                                   "--logs", "must-not-create-logs"),
                         self.expected("ubuntu", "aarch64"))

    def test_a_guest_runs_every_suite_unless_some_are_selected(self):
        for target in ("arch", "fedora", "ubuntu"):
            with self.subTest(target=target):
                self.assertEqual(self.plan(target, "x86_64")["suites"], list(ALL_SUITES))
        selections = {"session": ("session",), "greeter": ("greeter",),
                      "package+compositors": ("package", "compositors"),
                      "compositors+package": ("package", "compositors"),
                      "package+session+compositors+greeter": ALL_SUITES}
        for selection, suites in selections.items():
            with self.subTest(selection=selection):
                self.assertEqual(self.plan("ubuntu", "aarch64", "--suites", selection),
                                 self.expected("ubuntu", "aarch64", suites))

    def test_an_invalid_suite_selection_is_refused(self):
        for selection in ("", "sessions", "session+", "session+session", "session greeter",
                          "session,greeter", "all"):
            for print_plan in (True, False):
                with self.subTest(selection=selection, print_plan=print_plan):
                    arguments = ["--target", "ubuntu", "--arch", "aarch64", "--allow-emulation",
                                 "--package", "absent-candidate.deb", "--suites", selection]
                    if print_plan:
                        arguments.append("--print-plan")
                    result = self.invoke(*arguments)
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertEqual(result.stdout, "")
                    self.assertIn("--suites", result.stderr)

    def test_the_comparison_selector_is_gone(self):
        # Run 38012670359 settled the comparison: no option selects another algorithm.
        for arch in ("aarch64", "x86_64"):
            for profile in ("default", "impdef"):
                for print_plan in (True, False):
                    with self.subTest(arch=arch, profile=profile, print_plan=print_plan):
                        arguments = ["--target", "ubuntu", "--arch", arch, "--allow-emulation",
                                     "--tcg-pauth", profile, "--package", "absent-candidate.deb"]
                        if print_plan:
                            arguments.append("--print-plan")
                        result = self.invoke(*arguments)
                        self.assertEqual(result.returncode, 2, result.stderr)
                        self.assertEqual(result.stdout, "")
                        self.assertIn("unrecognized arguments", result.stderr)
                        self.assertIn("--tcg-pauth", result.stderr)

    def test_invalid_or_unpinned_plans_are_rejected(self):
        selections = (
            (("--target", "ubuntu", "--arch", "armv7l"), "invalid choice"),
            (("--target", "debian", "--arch", "x86_64"), "invalid choice"),
            (("--target", "arch", "--arch", "aarch64"), "No pinned aarch64 image"),
            (("--target", "fedora", "--arch", "aarch64"), "No pinned aarch64 image"),
        )
        for arguments, diagnostic in selections:
            with self.subTest(arguments=arguments):
                result = self.invoke(*arguments, "--print-plan")
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")
                self.assertIn(diagnostic, result.stderr)

    def test_execution_still_requires_a_candidate_package(self):
        # A fetch command fills --package later; it does not stand in for it.
        for arch in ("aarch64", "x86_64"):
            for fetch in ((), ("--package-fetch", "touch fetched-without-package")):
                with self.subTest(arch=arch, fetch=bool(fetch)):
                    result = self.invoke("--target", "ubuntu", "--arch", arch,
                                         "--allow-emulation", *fetch)
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertEqual(result.stdout, "")
                    self.assertIn("--package", result.stderr)
                    self.assertIn("required", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
