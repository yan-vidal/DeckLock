#!/usr/bin/env python3
"""Reject tags that disagree with the source version/package revision."""
from pathlib import Path
import re
import sys
import tomllib
root=Path(__file__).resolve().parent.parent
version=tomllib.loads((root/'Cargo.toml').read_text())['package']['version']
revision=re.search(r'^pkgrel=(\d+)$',(root/'packaging/arch/PKGBUILD.in').read_text(),re.M).group(1)
expected=f'v{version}'+(f'-r{revision}' if revision!='1' else '')
if len(sys.argv)!=2 or sys.argv[1]!=expected:raise SystemExit(f'Tag must match source and package revision: {expected}')
print(f'PASS: release tag {expected}')
