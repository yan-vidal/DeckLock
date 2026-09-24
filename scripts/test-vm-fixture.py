#!/usr/bin/env python3
"""Contract of the guest provisioning fixture, checked without a VM.

The real boundary is a guest whose cloud-init finished with a recoverable error,
which cannot be produced on demand: one CI run hit it and provisioning aborted
with exit 2 and no recorded reason. What can be pinned here is the decision that
made it abort, and the evidence that would have explained it, with cloud-init
replaced by a stub that exits the way the real one does.
"""
import ast
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
HELPER = ROOT / "scripts/vm/cloud-init-ready.sh"
SETUP = ROOT / "scripts/vm/setup.sh"
GREETER = ROOT / "scripts/vm/greeter.py"
COMPOSITOR = ROOT / "scripts/vm/compositor.py"


def run_with_stub(exit_code, output="status: done"):
    """Run the helper with a cloud-init that prints `output` and exits `exit_code`."""
    with tempfile.TemporaryDirectory(prefix="decklock-fixture-") as directory:
        directory = Path(directory)
        stub = directory / "cloud-init"
        stub.write_text(f'#!/bin/bash\necho "{output}"\nexit {exit_code}\n')
        stub.chmod(0o755)
        detail = directory / "status.txt"
        environment = dict(os.environ, PATH=f"{directory}:{os.environ['PATH']}")
        result = subprocess.run(["bash", str(HELPER), str(detail)], env=environment,
                                capture_output=True, text=True, timeout=30)
        return result, detail.read_text() if detail.is_file() else None


def main():
    failures = []

    # A clean run is usable, and its status is recorded either way.
    result, detail = run_with_stub(0)
    if result.returncode != 0:
        failures.append(f"a finished cloud-init was rejected: {result.returncode} {result.stderr}")
    if not detail or "exited 0" not in detail:
        failures.append(f"the status of a clean run was not recorded: {detail!r}")

    # The regression: "degraded done" is exit 2. The guest boots, installs and
    # runs the test, so provisioning must continue instead of failing the run.
    result, detail = run_with_stub(2)
    if result.returncode != 0:
        failures.append(
            f"a recoverable cloud-init error took the run down: {result.returncode} {result.stderr}")
    if not detail or "exited 2" not in detail:
        failures.append(f"the recoverable error was not recorded as evidence: {detail!r}")

    # A cloud-init that actually failed leaves a guest that proves nothing.
    result, _ = run_with_stub(1, output="status: error")
    if result.returncode == 0:
        failures.append("a failed cloud-init was accepted as a usable guest")

    # The decision has to be the one provisioning uses, not a script nobody calls.
    setup = SETUP.read_text()
    if "cloud-init-ready.sh" not in setup:
        failures.append("setup.sh does not use the fixture readiness check")
    if any(line.strip().startswith("cloud-init status") for line in setup.splitlines()):
        failures.append("setup.sh still waits on cloud-init directly, bypassing the check")
    if 'python3 "$fixture/greeter.py"' not in setup:
        failures.append("setup.sh does not exercise the packaged greetd login")
    try:
        compile(GREETER.read_text(), str(GREETER), 'exec')
    except SyntaxError as error:
        failures.append(f"greeter VM fixture does not parse: {error}")
    # The compositor list is read without running the guest-only script.
    known = set()
    try:
        tree = ast.parse(COMPOSITOR.read_text(), str(COMPOSITOR))
        for node in ast.walk(tree):
            if isinstance(node, ast.Assign) and any(
                    isinstance(t, ast.Name) and t.id == 'COMPOSITORS' for t in node.targets):
                known = set(ast.literal_eval(node.value))
    except SyntaxError as error:
        failures.append(f"compositor VM fixture does not parse: {error}")
    if not known:
        failures.append("compositor.py names no compositor to exercise")
    if 'compositor.py "$compositor"' not in setup or 'for compositor in $EXTRA_COMPOSITORS' not in setup:
        failures.append("setup.sh does not exercise each manifest's further compositors")
    for target in ('arch', 'fedora', 'ubuntu'):
        manifest = (ROOT / f'scripts/vm/distros/{target}.env').read_text()
        if 'greetd cage' not in manifest:
            failures.append(f"{target} VM does not install greetd and Cage")
        fields = dict(re.findall(r"^([A-Z_]+)='([^']*)'", manifest, re.MULTILINE))
        extra = fields.get('EXTRA_COMPOSITORS', '').split()
        if not extra:
            failures.append(f"{target} VM exercises no compositor besides Sway")
        for name in extra:
            if name not in known:
                failures.append(f"{target} VM names {name}, which compositor.py cannot start")
            if name not in fields.get('HARNESS_PACKAGES', '').split():
                failures.append(f"{target} VM exercises {name} without installing it")

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS: cloud-init readiness contract and its recorded evidence")


if __name__ == "__main__":
    main()
