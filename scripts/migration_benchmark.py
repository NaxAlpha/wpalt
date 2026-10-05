#!/usr/bin/env python3
"""Measure bounded offline assessment using the actual distribution executable.

Per-process wait4 resource usage; input parsing and full report serialization are
included. This is not a restore, database or universal WordPress parity benchmark.
"""
import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--output', default='work/m8-migration-profile.json')
args = parser.parse_args()
binary = Path(args.binary).resolve()
assert hasattr(os, 'wait4'), 'Use a supported POSIX measurement host'
report = {'format': 'wpalt-migration-profile-v1', 'machine': platform.platform(),
          'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
          'scope': 'Offline WXR assessment: process startup, XML/tree admission, classification and complete JSON report serialization. No DB, media mapping, HTML conversion or recovery cost included.',
          'profiles': []}
with tempfile.TemporaryDirectory(prefix='wpalt-migration-profile-') as temporary:
    root = Path(temporary)
    config = root/'offline.toml'
    config.write_text(f'database_url="sqlite://{root}/unused.db?mode=rwc"\ndata_dir="{root}/unused"\n')
    for count in (10, 100, 1000, 5000):
        items = ''.join(f'<item><title>Journal {n}</title><link>https://journal.example/story-{n}/</link><wp:post_id>{n}</wp:post_id><wp:post_type>post</wp:post_type><wp:post_name>story-{n}</wp:post_name><wp:status>publish</wp:status><content:encoded><![CDATA[<h2>Room to think</h2><p>A local story with useful words.</p>]]></content:encoded></item>' for n in range(1, count+1))
        source = root/'source.xml'
        source.write_text('<rss xmlns:wp="http://wordpress.org/export/1.2/" xmlns:content="http://purl.org/rss/1.0/modules/content/"><channel><wp:wxr_version>1.2</wp:wxr_version><wp:base_site_url>https://journal.example</wp:base_site_url>'+items+'</channel></rss>')
        samples = []
        for _ in range(3):
            with tempfile.TemporaryFile() as errors, tempfile.TemporaryFile() as result:
                started = time.perf_counter()
                process = subprocess.Popen([str(binary), '--config', str(config), 'wordpress-assess', str(source)], stdout=result, stderr=errors)
                _, status, usage = os.wait4(process.pid, 0)
                process.returncode = os.waitstatus_to_exitcode(status)
                errors.seek(0)
                assert process.returncode == 0, errors.read().decode(errors='replace')
                elapsed = (time.perf_counter()-started)*1000
                result.seek(0); assessment = json.load(result)
                assert assessment['source_items'] == count and assessment['supported_core_items'] == count
                samples.append({'wall_ms': round(elapsed, 3),
                                'cpu_ms': round((usage.ru_utime+usage.ru_stime)*1000, 3),
                                'peak_rss_bytes': usage.ru_maxrss*(1 if platform.system() == 'Darwin' else 1024)})
        assert not (root/'unused.db').exists() and not (root/'unused').exists()
        report['profiles'].append({'items': count, 'input_bytes': source.stat().st_size,
                                   'input_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
                                   'samples': samples, 'median_wall_ms': statistics.median(sample['wall_ms'] for sample in samples),
                                   'largest_peak_rss_bytes': max(sample['peak_rss_bytes'] for sample in samples)})
report['status'] = 'passed'
output = Path(args.output); output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(report, indent=2)+'\n')
print('PASS: measured offline assessment at 10/100/1000/5000 items without creating site state')
