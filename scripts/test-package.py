#!/usr/bin/env python3
"""Check archive payload and actual packaged executable; no installation or publication."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,required=True)
args=parser.parse_args()
binary=args.binary.resolve()
with tempfile.TemporaryDirectory(prefix='decklock-package-test-') as directory:
    temp=Path(directory)
    subprocess.run(['python3',str(root/'scripts/package-release.py'),'--binary',str(binary),'--output',str(temp)],check=True)
    archive=next(temp.glob('decklock-*.tar.gz'))
    expected=(temp/'SHA256SUMS').read_text().split()[0]
    assert hashlib.sha256(archive.read_bytes()).hexdigest()==expected
    assert expected in (temp/'PKGBUILD').read_text()
    unpack=temp/'unpack';unpack.mkdir()
    with tarfile.open(archive) as tar:
        assert all(not Path(m.name).is_absolute() and '..' not in Path(m.name).parts and not m.issym() and not m.islnk() for m in tar.getmembers())
        tar.extractall(unpack,filter='data')
    stage=next(unpack.iterdir())
    executable=stage/'bin/decklock'
    assert executable.stat().st_mode&0o111
    assert executable.read_bytes()==binary.read_bytes()
    for path in (root/'assets/media').rglob('*'):
        if path.is_file():assert (stage/'share/decklock/media'/path.relative_to(root/'assets/media')).read_bytes()==path.read_bytes()
    assert 'Exec=decklock --settings' in (stage/'share/applications/decklock.desktop').read_text()
    manifest=json.loads((stage/'BUILD-INFO.json').read_text())
    assert manifest['source_commit']==subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD'],text=True).strip()
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
    print('PASS: archive checksums, safe paths, source provenance, original media, launcher and packaged CLI')
