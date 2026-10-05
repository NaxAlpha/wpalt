#!/usr/bin/env python3
"""Owner journey: explicit private transfer, redaction, stale plans and safe output."""
import argparse, json, os, subprocess, tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);a=p.parse_args()
binary=str(Path(a.binary).resolve());env={k:v for k,v in os.environ.items() if not k.startswith('WPALT_')}
with tempfile.TemporaryDirectory(prefix='wpalt-config-transfer-') as temporary:
 root=Path(temporary);site=root/'not-created';source=root/'source.toml'
 source.write_text(f'data_dir={json.dumps(str(site))}\nbase_url="https://transfer.example.test"\nrequest_concurrency=19\n[identity]\nclient_secret="synthetic-transfer-secret"\n');source.chmod(0o600)
 def run(*args,ok=True,cfg=source):
  r=subprocess.run([binary,'--config',str(cfg),*map(str,args)],capture_output=True,text=True,env=env,timeout=20)
  assert (r.returncode==0)==ok, (args,r.returncode,r.stderr[:200]);assert 'synthetic-transfer-secret' not in r.stdout+r.stderr;return r
 redacted=root/'redacted.json';private=root/'private.json';out=root/'target.toml'
 run('config-export',redacted);record=json.loads(redacted.read_text());assert not record['contains_secrets'] and 'synthetic-transfer-secret' not in redacted.read_text()
 assert redacted.stat().st_mode&0o077==0
 run('config-import',redacted,'--accept-secrets',ok=False)
 run('config-export',private,'--include-secrets');assert 'synthetic-transfer-secret' in private.read_text() and private.stat().st_mode&0o077==0
 run('config-import',private,ok=False)
 review=json.loads(run('config-import',private,'--accept-secrets').stdout);assert review['configuration']['request_concurrency']==19
 run('config-import',private,'--accept-secrets','--execute','stale','--output',out,ok=False);assert not out.exists()
 # Bind exact bytes: changing a valid field invalidates an earlier review.
 original=private.read_bytes();package=json.loads(original);package['configuration']['request_concurrency']=20;private.write_text(json.dumps(package))
 run('config-import',private,'--accept-secrets','--execute',review['plan'],'--output',out,ok=False);assert not out.exists()
 private.write_bytes(original)
 run('config-import',private,'--accept-secrets','--execute',review['plan'],'--output',out)
 assert out.stat().st_mode&0o077==0 and 'synthetic-transfer-secret' in out.read_text()
 effective=json.loads(run('config',cfg=out).stdout);assert effective==review['configuration']
 before=out.read_bytes();run('config-import',private,'--accept-secrets','--execute',review['plan'],'--output',out,ok=False);assert out.read_bytes()==before
 package=json.loads(original);package['database_schema']=999;private.write_text(json.dumps(package));run('config-import',private,'--accept-secrets',ok=False)
 package=json.loads(original);package['configuration']['request_concurrency']=0;private.write_text(json.dumps(package));run('config-import',private,'--accept-secrets',ok=False)
 private.write_bytes(original);private.chmod(0o644);run('config-import',private,'--accept-secrets',ok=False)
 assert not site.exists(), 'Configuration transfer must not open a site/database'
print('PASS: private configuration round-trip, explicit secret admission, redaction, exact plans, version/validation refusal and no site writes')
