#!/usr/bin/env python3
"""Public cache metadata and download-only CI payload contracts; no VM is started."""
import ast
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parent.parent
CACHE_ACTION = "actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9"
SCRATCH = ROOT / "target/ci-development"


def pinned_distros():
    tree = ast.parse((ROOT / "scripts/test-vm.py").read_text())
    return next(ast.literal_eval(node.value) for node in tree.body
                if isinstance(node, ast.Assign)
                and any(isinstance(name, ast.Name) and name.id == "DISTROS"
                        for name in node.targets))


def block(source, name):
    """Read an indented YAML block without a CI dependency on PyYAML."""
    match = re.search(rf"^( +){re.escape(name)}: \|\n((?:(?:\1 +[^\n]*|)\n)+)",
                      source, re.MULTILINE)
    if not match:
        raise AssertionError(f"Missing {name} block")
    return textwrap.dedent(match.group(2))


class CacheContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)

    def metadata(self, *arguments, root=ROOT):
        with tempfile.TemporaryDirectory(prefix="cache-cli-", dir=SCRATCH) as directory:
            work = Path(directory)
            environment = dict(os.environ, HOME=str(work), PATH=str(work / "empty-bin"))
            for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR"):
                environment[name] = str(work / name)
            for name in ("WAYLAND_DISPLAY", "DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
                environment.pop(name, None)
            return subprocess.run([sys.executable, str(root / "scripts/vm-cache-key"), *arguments],
                                  cwd=work, env=environment, capture_output=True,
                                  text=True, timeout=10)

    def test_public_metadata_for_all_pinned_images(self):
        selections = (("arch", "x86_64"), ("fedora", "x86_64"),
                      ("ubuntu", "x86_64"), ("ubuntu", "aarch64"))
        distros = pinned_distros()
        for target, arch in selections:
            with self.subTest(target=target, arch=arch):
                distro = distros[target] | distros[target].get(arch, {})
                image = ROOT / ".deps/vm-cache" / distro["image"]
                before = image.stat() if image.exists() else None
                result = self.metadata("--target", target, "--arch", arch)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stderr, "")
                self.assertEqual(dict(line.split("=", 1) for line in result.stdout.splitlines()), {
                    "path": str(image),
                    "key": f'vm-download-v1-{target}-{arch}-{distro["sha256"]}',
                })
                self.assertEqual(image.stat() if image.exists() else None, before,
                                 "Metadata must not download or modify an image")

    def test_public_metadata_refuses_invalid_and_unpinned_selections(self):
        for arguments in (("--target", "arch", "--arch", "aarch64"),
                          ("--target", "fedora", "--arch", "aarch64"),
                          ("--target", "debian", "--arch", "x86_64"),
                          ("--target", "ubuntu", "--arch", "armv7l"),
                          ("--target", "ubuntu"), (),
                          ("--target", "../ubuntu", "--arch", "x86_64")):
            with self.subTest(arguments=arguments):
                result = self.metadata(*arguments)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "", "Invalid metadata must not emit cache outputs")

    def test_public_metadata_tracks_the_vm_runner_pin(self):
        with tempfile.TemporaryDirectory(prefix="cache-pins-", dir=SCRATCH) as directory:
            checkout = Path(directory)
            (checkout / "scripts").mkdir()
            shutil.copy2(ROOT / "scripts/vm-cache-key", checkout / "scripts/vm-cache-key")
            source = (ROOT / "scripts/test-vm.py").read_text()
            source = source.replace(pinned_distros()["ubuntu"]["aarch64"]["sha256"], "a" * 64)
            (checkout / "scripts/test-vm.py").write_text(source)
            result = self.metadata("--target", "ubuntu", "--arch", "aarch64", root=checkout)
            self.assertEqual(result.returncode, 0, result.stderr)
            outputs = dict(line.split("=", 1) for line in result.stdout.splitlines())
            self.assertEqual(outputs["key"], "vm-download-v1-ubuntu-aarch64-" + "a" * 64)
            self.assertFalse((checkout / ".deps").exists(), "Cold metadata must not create a cache")

    def test_cargo_cache_payload_is_downloaded_registry_and_git_only(self):
        source = (ROOT / ".github/actions/cache-cargo/action.yml").read_text()
        self.assertIn(f"uses: {CACHE_ACTION}", source)
        self.assertEqual(block(source, "path").splitlines(), [
            "${{ github.workspace }}/.ci-cargo/registry",
            "${{ github.workspace }}/.ci-cargo/git",
        ])
        self.assertNotIn("restore-keys:", source)

    def test_cargo_cache_keys_separate_builds_and_lockfiles(self):
        source = (ROOT / ".github/actions/cache-cargo/action.yml").read_text()
        key = re.search(r"^ +key: (.+)$", source, re.MULTILINE)
        self.assertIsNotNone(key)
        for dimension in ("inputs.target", "runner.os", "runner.arch", "inputs.identity",
                          "hashFiles('Cargo.lock')"):
            self.assertIn("${{ " + dimension + " }}", key.group(1))

    def test_cargo_home_is_shared_by_cache_and_build(self):
        source = (ROOT / ".github/actions/cache-cargo/action.yml").read_text()
        with tempfile.TemporaryDirectory(prefix="cache-home-", dir=SCRATCH) as directory:
            work = Path(directory)
            environment = dict(os.environ, GITHUB_WORKSPACE=str(work / "checkout"),
                               GITHUB_OUTPUT=str(work / "outputs"), GITHUB_ENV=str(work / "env"),
                               HOME=str(work / "home"))
            result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", block(source, "run")],
                                    env=environment, capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            cargo_home = work / "checkout/.ci-cargo"
            self.assertTrue(cargo_home.is_dir(), "A cold build needs a writable Cargo home")
            self.assertEqual((work / "outputs").read_text(), f"cargo-home={cargo_home}\n")
            self.assertEqual((work / "env").read_text(), f"CARGO_HOME={cargo_home}\n")

    def test_vm_action_caches_only_the_metadata_selected_image(self):
        source = (ROOT / ".github/actions/cache-vm/action.yml").read_text()
        self.assertIn(f"uses: {CACHE_ACTION}", source)
        self.assertRegex(source, r"(?m)^ +path: \$\{\{ steps\.metadata\.outputs\.path \}\}$")
        self.assertRegex(source, r"(?m)^ +key: \$\{\{ steps\.metadata\.outputs\.key \}\}$")
        self.assertNotIn("restore-keys:", source)
        self.assertIn("scripts/vm-cache-key", source)
        self.assertIn("${{ inputs.target }}", source)
        self.assertIn("${{ inputs.arch }}", source)

    def test_package_cache_uses_installed_toolchain_and_container_identity(self):
        source = (ROOT / ".github/workflows/build-package.yml").read_text()
        self.assertIn("uses: ./.github/actions/cache-cargo", source)
        self.assertIn("rustc -vV", source)
        self.assertIn("cargo -V", source)
        self.assertIn("sha256sum", source)
        self.assertIn("${{ inputs.image }}", source)
        self.assertRegex(source, r"runuser -u builder -- env CARGO_HOME=\"\$CARGO_HOME\" scripts/ci-build-package")
        self.assertLess(source.index("uses: ./.github/actions/cache-cargo"),
                        source.index('chown -R builder "$GITHUB_WORKSPACE"'))
        self.assertNotIn("cache-hit", source, "Cache hits must still build and test the checkout")


if __name__ == "__main__":
    unittest.main(verbosity=2)
