#!/usr/bin/env python3
"""Stage a distribution-built binary + author media as an archive and a recipe.

Run this inside the target distribution: the binary is shipped as built, so its
linked library baseline is that distribution's. Does not install anything on the
host. Build the package with the emitted recipe (makepkg, or rpmbuild) with the
declared runtime dependencies installed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

root = Path(__file__).resolve().parent.parent


def distribution():
    """Identify the build environment; the baseline is recorded, never assumed."""
    fields = dict(
        line.split('=', 1)
        for line in Path('/etc/os-release').read_text().splitlines()
        if '=' in line
    )
    return fields.get('PRETTY_NAME', fields.get('NAME', '?')).strip('"')

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, default=root/'target/release/decklock')
parser.add_argument('--output', type=Path, default=root/'dist')
# Arch keeps the published archive name: its v0.2.0 assets and the README install
# examples already point at it, and published assets are never replaced.
TARGETS = {
    'arch': {'slug': 'linux', 'label': 'Arch Linux x86_64', 'recipe': ('PKGBUILD.in', 'PKGBUILD')},
    'fedora': {'slug': 'fedora', 'label': 'Fedora x86_64', 'recipe': ('decklock.spec.in', 'decklock.spec')},
}
parser.add_argument('--target', choices=sorted(TARGETS), default='arch')
args = parser.parse_args()
target = TARGETS[args.target]
version = tomllib.loads((root/'Cargo.toml').read_text())['package']['version']
if not args.binary.is_file():
    raise SystemExit('Build the release binary first.')
if subprocess.check_output(['uname', '-m'], text=True).strip() != 'x86_64':
    raise SystemExit('These recipes target x86_64; do not mislabel another architecture.')
args.output.mkdir(parents=True, exist_ok=True)
name = f'decklock-{version}-{target["slug"]}-x86_64'
archive = args.output/f'{name}.tar.gz'
with tempfile.TemporaryDirectory(prefix='decklock-package-') as work:
    stage = Path(work)/name
    (stage/'bin').mkdir(parents=True)
    shutil.copy2(args.binary, stage/'bin/decklock')
    (stage/'bin/decklock').chmod(0o755)
    share = stage/'share'
    shutil.copytree(root/'assets/media', share/'decklock/media')
    shutil.copytree(root/'themes', share/'decklock/themes')
    (share/'applications').mkdir()
    shutil.copy2(root/'packaging/decklock.desktop', share/'applications/io.github.yan_vidal.DeckLock.desktop')
    # Do not shadow `name`: it is the archive root the PKGBUILD's package() reads.
    for source, folder, installed in [
        ('decklock.svg', 'scalable/apps', 'io.github.yan_vidal.DeckLock.svg'),
        ('decklock-symbolic.svg', 'symbolic/apps', 'io.github.yan_vidal.DeckLock-symbolic.svg'),
    ]:
        destination = share/'icons/hicolor'/folder/installed
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root/'assets/icons'/source, destination)
    # Carried in the payload so the PKGBUILD installs it and archive users can
    # see exactly which service file the default pam_service expects.
    (stage/'pam').mkdir()
    shutil.copy2(root/f'packaging/{args.target}/pam/decklock', stage/'pam/decklock')
    docs = share/'doc/decklock'
    docs.mkdir(parents=True)
    for file in ['README.md', 'README.pt-BR.md', 'config.example.toml', 'CHANGELOG.md']:
        shutil.copy2(root/file, docs/file)
    shutil.copytree(root/'docs/guide', docs/'guide')
    commit = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
    (stage/'BUILD-INFO.json').write_text(json.dumps({'version':version,'source_commit':commit,
        'target':target['label'], 'distribution':distribution(), 'required_libraries': [line.split('=>')[0].strip() for line in subprocess.check_output(['ldd',str(args.binary.resolve())],text=True).splitlines() if '=>' in line]},indent=2)+'\n')
    with tarfile.open(archive, 'w:gz') as tar:
        tar.add(stage, arcname=name)
sha = hashlib.sha256(archive.read_bytes()).hexdigest()
source, emitted = target['recipe']
recipe = (root/f'packaging/{args.target}'/source).read_text().replace('@VERSION@',version).replace('@SHA256@',sha)
(args.output/emitted).write_text(recipe)
(args.output/'SHA256SUMS').write_text(f'{sha}  {archive.name}\n')
print(f'Created {archive}\n{args.target} recipe: {args.output / emitted}')
