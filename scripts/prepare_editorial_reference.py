#!/usr/bin/env python3
"""Prepare checksum-pinned free analysis dependencies; never vendor them into wpalt."""
import argparse
import hashlib
import json
import urllib.request
import zipfile
from pathlib import Path

PINS = {
    'wordpress-7.1.3.zip': ('https://wordpress.org/wordpress-7.1.3.zip', 'eb58a90ed55147f90f7bd55d0d3334960a69b7e8a4cc142c8ddfea8965218227'),
    'wordpress-seo-28.6.zip': ('https://downloads.wordpress.org/plugin/wordpress-seo.28.6.zip', 'ebb6e2a89d697cabecf2348a06d1efe807fd13ee1faeddcd06e354b98ee2a4d5'),
}
MEMBERS = {
    'wordpress-7.1.3.zip': {
        'wp-lodash.js': 'wordpress/wp-includes/js/dist/vendor/lodash.min.js',
        'wp-hooks.js': 'wordpress/wp-includes/js/dist/hooks.min.js',
        'wp-i18n.js': 'wordpress/wp-includes/js/dist/i18n.min.js',
    },
    'wordpress-seo-28.6.zip': {
        'yoast-feature.js': 'wordpress-seo/js/dist/externals/featureFlag.js',
        'yoast-analysis.js': 'wordpress-seo/js/dist/externals/analysis.js',
        'yoast-researcher.js': 'wordpress-seo/js/dist/languages/en.js',
    },
}

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--output', type=Path, default=Path('work/d09-reference-source'))
    a = p.parse_args()
    a.output.mkdir(parents=True, exist_ok=True)
    receipt = {'wordpress': '7.1.3', 'yoast_free': '28.6', 'archives': {}, 'bundles': {}}
    for name, (url, digest) in PINS.items():
        target = a.output / name
        if not target.exists():
            with urllib.request.urlopen(url, timeout=60) as response:
                data = response.read(100 * 1024 * 1024 + 1)
            assert len(data) <= 100 * 1024 * 1024, 'Reference archive exceeds limit'
            assert hashlib.sha256(data).hexdigest() == digest, 'Reference checksum mismatch'
            target.write_bytes(data)
        data = target.read_bytes()
        assert hashlib.sha256(data).hexdigest() == digest, 'Cached reference checksum mismatch'
        receipt['archives'][name] = {'sha256': digest, 'bytes': len(data), 'url': url}
        with zipfile.ZipFile(target) as archive:
            for filename, member in MEMBERS[name].items():
                assert archive.getinfo(member).file_size <= 5 * 1024 * 1024, 'Bundle exceeds limit'
                bundle = archive.read(member)
                (a.output / filename).write_bytes(bundle)
                receipt['bundles'][filename] = {'member': member, 'sha256': hashlib.sha256(bundle).hexdigest(), 'bytes': len(bundle)}
    (a.output / 'pins.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print('PASS: actual free Yoast and WordPress analysis bundles match pinned archives')

if __name__ == '__main__':
    main()
