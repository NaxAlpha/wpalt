#!/usr/bin/env python3
"""Owner CLI journey: effective module admission, no site state or secret output."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
args = parser.parse_args()
binary = str(Path(args.binary).resolve())
env = {k: v for k, v in os.environ.items() if not k.startswith('WPALT_')}
env.update(WPALT_BUSINESS_ENABLED='false', WPALT_MEMBERSHIP_ENABLED='false', WPALT_COMMERCE_ENABLED='false')
with tempfile.TemporaryDirectory(prefix='wpalt-module-journey-') as temporary:
    root = Path(temporary)
    data = root / 'uncreated-site'
    config = root / 'site.toml'
    config.write_text('[engagement]\nenabled=true\n')
    result = subprocess.run([binary, '--config', str(config), '--data-dir', str(data), 'modules'],
                            env=env, check=True, capture_output=True, text=True, timeout=30)
    report = json.loads(result.stdout)
    assert report['format'] == 'wpalt-module-inventory-v1'
    modules = {m['id']: m for m in report['modules']}
    assert not data.exists(), 'Inventory must not open/create a site'
    for module in ['business', 'engagement', 'membership', 'commerce']:
        assert not modules[module]['enabled'], f'{module} ignored effective admission'
    assert modules['publishing']['enabled'] and modules['operations']['enabled']
    owned = [owner for m in modules.values() for owner in m['owns']]
    assert len(owned) == len(set(owned)), 'Domain ownership must be unique'
    assert all(dep in modules for m in modules.values() for dep in m['dependencies'])
    assert 'database_url' not in result.stdout and 'password' not in result.stdout
    invalid = subprocess.run([binary, '--database-url', 'mysql://unsupported', 'modules'],
                             env=env, capture_output=True, timeout=30)
    assert invalid.returncode != 0 and not data.exists(), 'Invalid config must fail before site admission'
print('PASS: owner module inventory, parent admission, ownership, validation and no site/credential output')
