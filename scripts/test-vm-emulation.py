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
FAST_PAUTH = "max,pauth=on,pauth-impdef=on,pauth-qarma3=off"

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

    def expected(self, target, arch, pauth="default"):
        distro = self.distros[target] | self.distros[target].get(arch, {})
        return {"target": target, "arch": arch, "image": distro["image"],
                "image_sha256": distro["sha256"], "tcg_pauth": pauth,
                "memory_mib": 2048,
                "cpu_models": {"kvm": "host", "tcg": FAST_PAUTH if pauth == "impdef" else "max"},
                "cpus": {"kvm": 2, "tcg": 4}, "slowdown": {"kvm": 1, "tcg": 8}}

    def test_arm_impdef_plan_keeps_pauth_enabled_and_runtime_bounds(self):
        self.assertEqual(self.plan("ubuntu", "aarch64", "--allow-emulation", "--tcg-pauth", "impdef"),
                         self.expected("ubuntu", "aarch64", "impdef"))

    def test_default_arm_profile_preserves_the_original_cpu(self):
        expected = self.expected("ubuntu", "aarch64")
        self.assertEqual(self.plan("ubuntu", "aarch64"), expected)
        self.assertEqual(self.plan("ubuntu", "aarch64", "--tcg-pauth", "default"), expected)

    def test_default_x86_plan_keeps_each_distribution_pin(self):
        for target in ("arch", "fedora", "ubuntu"):
            with self.subTest(target=target):
                self.assertEqual(self.plan(target, "x86_64"), self.expected(target, "x86_64"))

    def test_plan_needs_neither_a_package_nor_permission_to_emulate(self):
        # Execution flags and nonexistent paths must not activate their effects.
        self.assertEqual(self.plan("ubuntu", "aarch64", "--tcg-pauth", "impdef",
                                   "--provision-native", "--package", "absent-candidate.deb",
                                   "--logs", "must-not-create-logs"),
                         self.expected("ubuntu", "aarch64", "impdef"))

    def test_impdef_refuses_x86_before_any_runtime_work(self):
        for print_plan in (True, False):
            with self.subTest(print_plan=print_plan):
                arguments = ["--target", "ubuntu", "--arch", "x86_64",
                             "--tcg-pauth", "impdef", "--package", "absent-candidate.deb"]
                if print_plan:
                    arguments.append("--print-plan")
                result = self.invoke(*arguments)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")
                self.assertIn("aarch64", result.stderr)
                self.assertIn("impdef", result.stderr)

    def test_invalid_or_unpinned_plans_are_rejected(self):
        selections = (
            (("--target", "ubuntu", "--arch", "aarch64", "--tcg-pauth", "qarma5"), "invalid choice"),
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
        for profile in ("default", "impdef"):
            with self.subTest(profile=profile):
                result = self.invoke("--target", "ubuntu", "--arch", "aarch64",
                                     "--allow-emulation", "--tcg-pauth", profile)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertIn("--package", result.stderr)
                self.assertIn("required", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
