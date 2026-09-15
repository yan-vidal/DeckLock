#!/usr/bin/env python3
"""Report the release channel implied by the source version."""
from pathlib import Path
import tomllib
root=Path(__file__).resolve().parent.parent
version=tomllib.loads((root/'Cargo.toml').read_text())['package']['version']
print('prerelease' if '-' in version else 'release')
