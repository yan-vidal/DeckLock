#!/usr/bin/env python3
"""Stage an Arch-built binary + author media as an archive and a makepkg recipe.

Does not install anything on the host. Build the Arch package with the emitted
PKGBUILD using makepkg (with the declared runtime dependencies installed).
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
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, default=root/'target/release/decklock')
parser.add_argument('--output', type=Path, default=root/'dist')
args = parser.parse_args()
version = tomllib.loads((root/'Cargo.toml').read_text())['package']['version'].removesuffix('.0')
if not args.binary.is_file():
    raise SystemExit('Build the release binary first.')
if subprocess.check_output(['uname', '-m'], text=True).strip() != 'x86_64':
    raise SystemExit('This recipe targets Arch x86_64; do not mislabel another architecture.')
args.output.mkdir(parents=True, exist_ok=True)
name = f'decklock-{version}-linux-x86_64'
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
    docs = share/'doc/decklock'
    docs.mkdir(parents=True)
    for file in ['README.md', 'README.pt-BR.md', 'config.example.toml']:
        shutil.copy2(root/file, docs/file)
    commit = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
    (stage/'BUILD-INFO.json').write_text(json.dumps({'version':version,'source_commit':commit,
        'target':'Arch Linux x86_64', 'required_libraries': [line.split('=>')[0].strip() for line in subprocess.check_output(['ldd',str(args.binary.resolve())],text=True).splitlines() if '=>' in line]},indent=2)+'\n')
    with tarfile.open(archive, 'w:gz') as tar:
        tar.add(stage, arcname=name)
sha = hashlib.sha256(archive.read_bytes()).hexdigest()
recipe = (root/'packaging/arch/PKGBUILD.in').read_text().replace('@VERSION@',version).replace('@SHA256@',sha)
(args.output/'PKGBUILD').write_text(recipe)
(args.output/'SHA256SUMS').write_text(f'{sha}  {archive.name}\n')
print(f'Created {archive}\nArch recipe: {args.output / "PKGBUILD"}')
