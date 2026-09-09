#!/usr/bin/env python3
"""Exercise tag and changelog commands against private fixture repositories."""
from pathlib import Path
import shutil
import subprocess
import tempfile
root = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='decklock-release-test-') as temp:
    fixture = Path(temp)
    (fixture/'scripts').mkdir()
    (fixture/'packaging/arch').mkdir(parents=True)
    for name in ['check-release-tag.py', 'release-notes.py']:
        shutil.copy2(root/'scripts'/name, fixture/'scripts'/name)
    def run(name, *args, success=True):
        result = subprocess.run(['python3', str(fixture/'scripts'/name), *args], capture_output=True, text=True, timeout=5)
        assert (result.returncode == 0) == success, result.stderr
        return result.stdout
    for version in ['0.2.0', '0.2.1', '0.2.10']:
        (fixture/'Cargo.toml').write_text(f'[package]\nversion="{version}"\n')
        for revision in [1, 2]:
            (fixture/'packaging/arch/PKGBUILD.in').write_text(f'pkgrel={revision}\n')
            tag = 'v'+version+(f'-r{revision}' if revision != 1 else '')
            run('check-release-tag.py', tag)
            run('check-release-tag.py', 'v0.2', success=False)
            run('check-release-tag.py', 'v9.9.9', success=False)
        (fixture/'CHANGELOG.md').write_text(f'# Changelog\n\n## [{version}] - Unreleased\n\n### Added\n\n- Reviewed change.\n\n## [0.1] - 2026-01-01\n\nOld change.\n')
        notes = run('release-notes.py')
        assert 'Reviewed change.' in notes and 'Old change.' not in notes
        run('release-notes.py', '--require-date', success=False)
        (fixture/'CHANGELOG.md').write_text((fixture/'CHANGELOG.md').read_text().replace('Unreleased', '2026-09-09'))
        run('release-notes.py', '--require-date')
        (fixture/'CHANGELOG.md').write_text('# Missing release')
        run('release-notes.py', success=False)
print('PASS: exact version/revision tags, reviewed notes, required release date and missing-section rejection')
