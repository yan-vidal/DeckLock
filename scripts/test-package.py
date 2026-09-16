#!/usr/bin/env python3
"""Check archive payload and actual packaged executable; no installation or publication."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tomllib
import tempfile

root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,required=True)
# Each target keeps its own archive name, PAM stack and recipe; nothing is shared
# between distributions beyond the payload itself.
TARGETS={
    'arch':{'slug':'linux','recipe':'PKGBUILD','include':'system-auth',
            'markers':["backup=('etc/pam.d/decklock')",'"$pkgdir/etc/pam.d/decklock"']},
    'fedora':{'slug':'fedora','recipe':'decklock.spec','include':'system-auth',
              'markers':['%config(noreplace) %{_sysconfdir}/pam.d/decklock',
                         'install -Dm644 pam/decklock %{buildroot}%{_sysconfdir}/pam.d/decklock',
                         '%global debug_package %{nil}']},
}
parser.add_argument('--target',choices=sorted(TARGETS),default='arch')
args=parser.parse_args()
target=TARGETS[args.target]
binary=args.binary.resolve()
subprocess.run(['python3', str(root/'scripts/test-release.py')], check=True)
with tempfile.TemporaryDirectory(prefix='decklock-package-test-') as directory:
    temp=Path(directory)
    subprocess.run(['python3',str(root/'scripts/package-release.py'),'--binary',str(binary),'--output',str(temp),'--target',args.target],check=True)
    archive=next(temp.glob('decklock-*.tar.gz'))
    expected=(temp/'SHA256SUMS').read_text().split()[0]
    assert hashlib.sha256(archive.read_bytes()).hexdigest()==expected
    recipe=(temp/target['recipe']).read_text()
    assert expected in recipe
    unpack=temp/'unpack';unpack.mkdir()
    with tarfile.open(archive) as tar:
        assert all(not Path(m.name).is_absolute() and '..' not in Path(m.name).parts and not m.issym() and not m.islnk() for m in tar.getmembers())
        tar.extractall(unpack,filter='data')
    # The recipes read the archive root by name -- $srcdir/<name>/ for makepkg and
    # %setup -n <name> for rpmbuild -- so it is part of the contract, not an
    # implementation detail. Arch keeps the already-published spelling.
    version=tomllib.loads((root/'Cargo.toml').read_text())['package']['version']
    expected_root=f'decklock-{version}-{target["slug"]}-x86_64'
    assert [p.name for p in unpack.iterdir()]==[expected_root],[p.name for p in unpack.iterdir()]
    stage=unpack/expected_root
    executable=stage/'bin/decklock'
    assert executable.stat().st_mode&0o111
    assert executable.read_bytes()==binary.read_bytes()
    for path in (root/'assets/media').rglob('*'):
        if path.is_file():assert (stage/'share/decklock/media'/path.relative_to(root/'assets/media')).read_bytes()==path.read_bytes()
    assert 'Exec=decklock --settings' in (stage/'share/applications/io.github.yan_vidal.DeckLock.desktop').read_text()
    assert 'Icon=io.github.yan_vidal.DeckLock' in (stage/'share/applications/io.github.yan_vidal.DeckLock.desktop').read_text()
    for source, installed in [('decklock.svg', 'scalable/apps/io.github.yan_vidal.DeckLock.svg'), ('decklock-symbolic.svg', 'symbolic/apps/io.github.yan_vidal.DeckLock-symbolic.svg')]:
        assert (stage/'share/icons/hicolor'/installed).read_bytes() == (root/'assets/icons'/source).read_bytes()
    for path in (root/'docs/guide').rglob('*.md'):
        assert (stage/'share/doc/decklock/guide'/path.relative_to(root/'docs/guide')).read_bytes() == path.read_bytes()
    assert (stage/'share/doc/decklock/CHANGELOG.md').read_bytes() == (root/'CHANGELOG.md').read_bytes()
    assert (stage/'pam/decklock').read_bytes()==(root/f'packaging/{args.target}/pam/decklock').read_bytes()
    assert f'auth     include  {target["include"]}' in (stage/'pam/decklock').read_text()
    for marker in target['markers']:
        assert marker in recipe, marker
    manifest=json.loads((stage/'BUILD-INFO.json').read_text())
    assert manifest['source_commit']==subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD'],text=True).strip()
    # The baseline is recorded from the build environment, never copied between
    # distributions; packaging/README.md requires each target to record its own.
    assert manifest['distribution'] and manifest['distribution']!='?',manifest
    assert manifest['required_libraries'],manifest
    env=os.environ.copy()
    for key in ['DISPLAY','WAYLAND_DISPLAY','WAYLAND_SOCKET','DBUS_SESSION_BUS_ADDRESS']:env.pop(key,None)
    env.update(XDG_CONFIG_HOME=str(temp/'config'),XDG_DATA_HOME=str(temp/'data'))
    def cli(arguments,success=True):
        result=subprocess.run([str(executable),*arguments],env=env,capture_output=True,text=True,timeout=10)
        assert result.returncode==0 if success else result.returncode!=0, result.stderr
        return result.stdout
    assert 'Usage:' in cli([])
    assert 'set' in cli(['config','--help'])
    config=temp/'config.toml'
    cli(['--config',str(config),'config','set','layout.padding','48'])
    assert cli(['config','get','layout.padding','--config',str(config)]).strip()=='48'
    before=config.read_bytes()
    cli(['--config',str(config),'config','set','idle_seconds','0'],False)
    assert config.read_bytes()==before
    print(f'PASS[{args.target}]: archive checksums, safe paths, source provenance, recorded baseline, original media, launcher and packaged CLI')
