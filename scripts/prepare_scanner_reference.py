#!/usr/bin/env python3
"""Extract a pinned official Linux scanner into ignored work, without installation."""
import argparse
import hashlib
import json
import subprocess
import urllib.request
from pathlib import Path

VERSION = '1.5.4'
SIZE = 105_819_396
SHA = '28d6efc5b4423e7830c3559339552eb53870a9eac51ac4efb37d60530d329886'
URL = f'https://github.com/Cisco-Talos/clamav/releases/download/clamav-{VERSION}/clamav-{VERSION}.linux.x86_64.deb'
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--output', type=Path, default=Path('work/d11-linux-scanner'))
a = p.parse_args();out = a.output.resolve();out.mkdir(parents=True,exist_ok=True)
archive = out / f'clamav-{VERSION}.linux.x86_64.deb'
if not archive.exists():
    with urllib.request.urlopen(URL,timeout=60) as response:
        content = response.read(SIZE+1)
    if len(content) != SIZE or hashlib.sha256(content).hexdigest() != SHA:
        raise ValueError('Official scanner archive differs from the reviewed pin')
    archive.write_bytes(content)
content = archive.read_bytes()
if len(content) != SIZE or hashlib.sha256(content).hexdigest() != SHA:
    raise ValueError('Prepared scanner archive differs from the reviewed pin')
subprocess.run(['dpkg-deb','-x',str(archive),str(out/'extracted')],check=True)
record = {'version':VERSION,'url':URL,'bytes':SIZE,'archive_sha256':SHA,
          'trust':'Official GitHub HTTPS download and reviewed digest pin; no independent Linux package-signature claim.',
          'scope':'Extracted controlled-signature fixture only; no global installation or official fresh intelligence.'}
(out/'pins.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
