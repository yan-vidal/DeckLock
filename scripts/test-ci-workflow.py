#!/usr/bin/env python3
"""Exercise required CI scheduling and the actual final gate's failure boundary."""
import json
import os
from pathlib import Path
import re
import subprocess
import textwrap

ROOT = Path(__file__).resolve().parent.parent
workflow = (ROOT / '.github/workflows/checks.yml').read_text()


def job(name):
    found = re.search(rf'^  {re.escape(name)}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)',
                      workflow, re.MULTILINE | re.DOTALL)
    assert found, f'missing required job {name}'
    return found.group(1)


packages = ['package', 'package-fedora', 'package-ubuntu', 'package-fedora-aarch64',
            'package-ubuntu-aarch64']
vms = {'vm': 'package', 'vm-fedora': 'package-fedora', 'vm-ubuntu': 'package-ubuntu',
       'vm-ubuntu-aarch64': 'package-ubuntu-aarch64'}
for package in packages:
    assert not re.search(r'^    needs:', job(package), re.MULTILINE), (
        f'{package} must start alongside isolated tests')
# Preparing and booting an emulated guest takes about as long as building its
# candidate, so that job starts with the build and awaits the build's artifact.
awaited = {'vm-ubuntu-aarch64'}
package_workflow = (ROOT / '.github/workflows/build-package.yml').read_text()
assert '    name: ${{ inputs.label }} package contract\n' in package_workflow
assert 'name: ${{ inputs.target }}-package${{ inputs.artifact_suffix }}' in package_workflow
for vm, package in vms.items():
    block, build = job(vm), job(package)
    if vm not in awaited:
        assert re.search(rf'^    needs: {package}$', block, re.MULTILINE), (
            f'{vm} must wait for its exact candidate')
        # A block ends where the next job's leading comment begins, so only its steps count.
        steps = block.split('    steps:\n', 1)[1].split('\n  #', 1)[0]
        assert '--package-fetch' not in steps and 'ci-await-candidate' not in steps
        continue
    assert not re.search(r'^    needs:', block, re.MULTILINE), f'{vm} must boot while {package} builds'

    def value(key):
        found = re.search(rf'^      {key}: (.+)$', build, re.MULTILINE)
        return found.group(1) if found else ''
    builder = f"{re.search(r'^    name: (.+)$', build, re.MULTILINE).group(1)} / {value('label')} package contract"
    artifact = f"{value('target')}-package{value('artifact_suffix')}"
    assert (f'scripts/ci-await-candidate --job "{builder}" --artifact {artifact} --directory dist'
            in block), f'{vm} must await the artifact of the job that builds its candidate'
    assert "--package 'dist/*.deb'" in block
    assert 'actions/download-artifact' not in block, 'Only the awaited artifact may be the candidate'
    assert re.search(r'^    permissions:\n      contents: read\n      actions: read\n', block, re.MULTILINE), (
        f'{vm} lists jobs and downloads an artifact of its own run')
    assert 'GH_TOKEN: ${{ github.token }}' in block
# A called workflow cannot exceed its caller's permissions, so the release must grant them.
release = (ROOT / '.github/workflows/release.yml').read_text()
assert re.search(r'^  quality:\n(?:    #.*\n)*    permissions:\n      contents: read\n      actions: read\n'
                 r'    uses: \./\.github/workflows/checks\.yml$', release, re.MULTILINE), (
    'release.yml must let checks.yml read this run\'s jobs and artifacts')
assert re.search(r'^permissions:\n  contents: read\n', workflow, re.MULTILINE)

required = job('required')
needs = re.search(r'^    needs: \[([^\]]*)\]', required, re.MULTILINE)
expected = {'check', *packages, *vms}
assert needs and {value.strip() for value in needs.group(1).split(',')} == expected
assert 'if: ${{ always() }}' in required
# Execute the real aggregate step, rather than reimplementing its acceptance rule.
command = textwrap.dedent(required.split('        run: |\n', 1)[1])
base = {name: {'result': 'success'} for name in sorted(expected)}
for name in sorted(expected):
    for result in ['success', 'failure', 'cancelled', 'skipped']:
        gates = base | {name: {'result': result}}
        execution = subprocess.run(['sh', '-eu', '-c', command], cwd=ROOT,
                                   env=os.environ | {'GATE_RESULTS': json.dumps(gates)},
                                   capture_output=True, text=True, timeout=5)
        assert (execution.returncode == 0) == (result == 'success'), (
            f'aggregate incorrectly accepted/rejected {name}={result}: {execution.stderr}')

# Only superseded PR runs are cancelled; tags and main retain their evidence.
assert re.search(r'^concurrency:\n', workflow, re.MULTILINE)
assert "cancel-in-progress: ${{ github.event_name == 'pull_request' }}" in workflow

native = job('check')
assert 'scripts/check --all' in native
assert 'uses: docker/build-push-action@' in native
assert 'context: target/ci-image-context' in native
assert 'load: true' in native and 'push: false' in native
assert 'CACHE_EPOCH=${{ steps.image-context.outputs.week }}' in native
assert 'cache-from: type=gha,scope=decklock-native-amd64' in native
assert 'cache-to: type=gha,scope=decklock-native-amd64' in native
assert 'identity: ${{ steps.native-image.outputs.imageid }}' in native
assert '--privileged' not in native and '/dev/' not in native
for vm in vms:
    assert 'uses: ./.github/actions/cache-vm' in job(vm)

# The comparison of pointer-authentication algorithms ended (run 38012670359):
# test-vm.py chooses the one emulated profile, and the job runs the whole suite once.
arm = job('vm-ubuntu-aarch64')
assert '    name: Real Wayland and PAM (Ubuntu aarch64)\n' in arm
assert 'strategy:' not in arm and 'matrix' not in arm, 'The ARM VM runs one profile, once'
assert 'continue-on-error:' not in arm
assert '--tcg-pauth' not in arm, 'The CPU profile is test-vm.py\'s choice, not the workflow\'s'
assert '--artifact ubuntu-package-aarch64 ' in arm
assert '--allow-emulation --provision-native' in arm
assert '          name: vm-evidence-ubuntu-aarch64\n' in arm
assert '--suite' not in arm, 'The ARM VM must run the whole guest suite'

recipe = (ROOT / '.github/ci/native-tests.Dockerfile').read_text()
for line in recipe.splitlines():
    if line.startswith('FROM '):
        assert '@sha256:' in line, 'dependency image bases must be immutable'
assert 'ARG CACHE_EPOCH' in recipe and 'bootstrap-native --tests' in recipe
assert '1.93.0' in recipe
assert not re.search(r'^COPY (?:\. |src|target|Cargo)', recipe, re.MULTILINE)

release = (ROOT / '.github/workflows/release.yml').read_text()
assert 'uses: ./.github/workflows/checks.yml' in release and 'needs: quality' in release
print('PASS: concurrent scheduling, 40 aggregate results, PR cancellation, dependency image and VM cache wiring')
