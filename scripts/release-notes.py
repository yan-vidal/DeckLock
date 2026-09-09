#!/usr/bin/env python3
"""Extract the reviewed release summary; tagging requires a real release date."""
import argparse
import datetime
from pathlib import Path
import re
import tomllib
root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--require-date', action='store_true')
args = parser.parse_args()
version = tomllib.loads((root/'Cargo.toml').read_text())['package']['version']
source = (root/'CHANGELOG.md').read_text()
match = re.search(r'^## \[' + re.escape(version) + r'\] - ([^\n]+)\n(.*?)(?=^## |\Z)', source, re.M | re.S)
if not match or not match[2].strip():
    raise SystemExit(f'Missing changelog for {version}')
if args.require_date:
    try:
        datetime.date.fromisoformat(match[1].strip())
    except ValueError:
        raise SystemExit('Approve the release and replace Unreleased with its date before tagging')
print(f'DeckLock {version} — experimental prerelease\n')
print(match[2].strip())
