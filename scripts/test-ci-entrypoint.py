#!/usr/bin/env python3
"""Run the real native-image entrypoint against private fake system commands."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='decklock-entrypoint-') as directory:
    temp = Path(directory)
    helpers = temp / 'bin'
    helpers.mkdir()
    stub = helpers / 'stub'
    stub.write_text('''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
name = Path(sys.argv[0]).name
with open(os.environ['OPERATIONS'], 'a') as log:
    log.write(json.dumps([name, *sys.argv[1:]]) + '\\n')
if name == 'getent' and sys.argv[1] == 'passwd':
    print('fixture:x:1000:1000:Fixture:/unused-home:/bin/sh')
if name == 'runuser':
    assert sys.argv[1:4] == ['-u', 'fixture', '--']
    os.environ['HOME'] = '/unused-home'
    os.execvp(sys.argv[4], sys.argv[4:])
''')
    stub.chmod(0o755)
    for name in ['getent', 'groupadd', 'useradd', 'mkdir', 'chown', 'ln', 'runuser', 'cargo']:
        (helpers / name).symlink_to(stub)
    env = os.environ | {'PATH': str(helpers) + ':/usr/bin:/bin', 'OPERATIONS': str(temp / 'ops'),
                        'HOME': str(temp), 'DECKLOCK_CI_UID': '1000', 'DECKLOCK_CI_GID': '1000'}
    probe = 'import json,os,sys; print(json.dumps([os.environ["HOME"],os.environ["CARGO_HOME"],sys.argv[1:]]))'
    command = ['sh', str(ROOT / '.github/ci/native-tests-entrypoint'),
               '/usr/bin/python3', '-c', probe, 'two words', '--all']
    result = subprocess.run(command, cwd=temp, env=env, capture_output=True, text=True, timeout=10)
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout) == ['/tmp/decklock-ci-home', '/workspace/.ci-cargo', ['two words', '--all']]
    operations = [json.loads(line) for line in (temp / 'ops').read_text().splitlines()]
    assert ['cargo', 'fetch', '--locked'] in operations
    assert any(op[:4] == ['runuser', '-u', 'fixture', '--'] for op in operations)
    for uid in ['0', 'invalid']:
        denied = subprocess.run(command, cwd=temp, env=env | {'DECKLOCK_CI_UID': uid},
                                capture_output=True, text=True, timeout=10)
        assert denied.returncode != 0 and denied.stdout == '', 'unsafe UID must fail before running tests'
    assert [json.loads(line) for line in (temp / 'ops').read_text().splitlines()] == operations

workflow = (ROOT / '.github/workflows/checks.yml').read_text()
assert 'github.event.pull_request.number || github.run_id' in workflow, (
    'Main and tag checks for one SHA need independent concurrency groups')
print('PASS: entrypoint identities, private environment, argument forwarding and independent non-PR runs')
