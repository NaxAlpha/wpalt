#!/usr/bin/env python3
"""Pin actual free calculator source in ignored work; never vendor it into wpalt."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = '1.58.0'
SHA = '2acd3a0536c5481e5f24fab02a5a79ce0601d8c55d44469274221f3b3bc584f7'
SIZE = 11_414_674
URL = f'https://downloads.wordpress.org/plugin/forminator.{VERSION}.zip'

def prepare(output):
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f'forminator-{VERSION}.zip'
    if not archive.exists():
        with urllib.request.urlopen(URL, timeout=60) as response:
            data = response.read(SIZE + 1)
        if len(data) != SIZE or hashlib.sha256(data).hexdigest() != SHA:
            raise ValueError('Actual free archive differs from the reviewed pin')
        archive.write_bytes(data)
    data = archive.read_bytes()
    if len(data) != SIZE or hashlib.sha256(data).hexdigest() != SHA:
        raise ValueError('Prepared archive differs from the reviewed pin')
    prefix = 'forminator/assets/js/front/calculator/'
    total = 0
    with zipfile.ZipFile(archive) as source:
        files = [member for member in source.infolist() if member.filename.startswith(prefix) and member.filename.endswith('.js')]
        if not 1 <= len(files) <= 200 or len({member.filename for member in files}) != len(files):
            raise ValueError('Unexpected calculator member set')
        for member in files:
            relative = PurePosixPath(member.filename[len(prefix):])
            if relative.is_absolute() or '..' in relative.parts or member.file_size > 512 * 1024:
                raise ValueError('Unsafe calculator source member')
            content = source.read(member)
            total += len(content)
            if total > 5 * 1024 * 1024:
                raise ValueError('Calculator source exceeds work budget')
            target = output / 'modules' / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
    bundle = output / 'free-calculator.js'
    subprocess.run([str(ROOT / 'frontend/node_modules/.bin/esbuild'), str(output / 'modules/front.calculator.js'),
                    '--bundle', '--platform=browser', '--format=iife', '--global-name=FreeForminator', '--outfile=' + str(bundle)],
                   cwd=ROOT, check=True)
    record = {'date': '2026-10-12', 'plugin': 'Forminator', 'version': VERSION, 'download': URL,
              'sha256': SHA, 'bytes': SIZE, 'selected_source_members': len(files), 'selected_source_bytes': total,
              'bundle_sha256': hashlib.sha256(bundle.read_bytes()).hexdigest(), 'bundler': 'locked esbuild0.28.2',
              'scope': 'Pinned source-built free calculator component preparation. Browser execution is separate evidence; no PHP/admin/premium parity or plugin code incorporated into wpalt.'}
    (output / 'pins.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps({'version': VERSION, 'source_members': len(files), 'bundle_sha256': record['bundle_sha256']}))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'work/d10-reference-source')
    prepare(parser.parse_args().output.resolve())
