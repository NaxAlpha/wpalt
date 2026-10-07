#!/usr/bin/env python3
"""Data-bearing stopped-site native maintenance, preflight, retry and fresh recovery.
The optional schema-14 fixture removes M9 deployment authority from a current seed;
--source-schema 15 retains authority. --source-binary uses a real older executable.
"""
import argparse, hashlib, json, os, socket, time, urllib.request, secrets, shutil, sqlite3, subprocess, tempfile, urllib.parse
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--source-binary');p.add_argument('--source-schema',type=int,choices=[14,15,16,17],default=14);p.add_argument('--psql',default=shutil.which('psql'));a=p.parse_args()
binary=str(Path(a.binary).resolve());source_binary=str(Path(a.source_binary).resolve()) if a.source_binary else binary;env={k:v for k,v in os.environ.items() if not k.startswith('WPALT_')}
root_url=os.environ.get('TEST_DATABASE_URL');schemas=[]
def pg(statement,url=root_url):
 r=subprocess.run([a.psql,url,'-X','-qAt','-v','ON_ERROR_STOP=1','-c',statement],capture_output=True,text=True,timeout=30);assert r.returncode==0,'Isolated maintenance SQL failed';return r.stdout.strip()
def database(root,name,postgres):
 if not postgres:return 'sqlite://'+str(root/(name+'.db'))+'?mode=rwc'
 assert a.psql;schema='wpalt_maintenance_'+secrets.token_hex(12);pg('CREATE SCHEMA '+schema);schemas.append(schema)
 u=urllib.parse.urlsplit(root_url);q=urllib.parse.parse_qsl(u.query);q.append(('options','-c search_path='+schema));return urllib.parse.urlunsplit(u._replace(query=urllib.parse.urlencode(q,quote_via=urllib.parse.quote)))
try:
 with tempfile.TemporaryDirectory(prefix='wpalt-maintenance-') as temporary:
  for postgres in [False,*([True] if root_url else [])]:
   root=Path(temporary)/('postgres' if postgres else 'sqlite');root.mkdir();url=database(root,'source',postgres);data=root/'source';cfg=root/'source.toml';cfg.write_text(f'database_url={json.dumps(url)}\ndata_dir={json.dumps(str(data))}\n');cfg.chmod(0o600)
   def run(*args,config=cfg,ok=True,stdin=None,executable=binary):
    r=subprocess.run([executable,'--config',str(config),*map(str,args)],input=stdin,capture_output=True,text=True,env=env,timeout=45);assert (r.returncode==0)==ok,(args,r.returncode,r.stderr[:200]);return r
   def sql(statement):
    if postgres:return pg(statement,url)
    with sqlite3.connect(root/'source.db') as c:
     result=c.execute(statement);rows=result.fetchall();return '\n'.join(str(r[0]) for r in rows)
   run('init','--admin-email','owner@example.test',stdin=secrets.token_urlsafe(24)+'\n',executable=source_binary);run('seed-demo','--posts','10',executable=source_binary);run('shop','seed-demo',executable=source_binary)
   if not a.source_binary:
    if a.source_schema==14:sql('DROP TABLE process_authority')
    sql('UPDATE schema_version SET version='+str(a.source_schema)+' WHERE id=1')
   assert sql('SELECT version FROM schema_version')==str(a.source_schema), 'Independent source must match the explicitly selected native schema'
   before=sql('SELECT COUNT(*) FROM posts');products=sql('SELECT COUNT(*) FROM shop_products')
   run('serve','--external-worker',ok=False);run('job-history',ok=False);assert sql('SELECT version FROM schema_version')==str(a.source_schema)
   preview=json.loads(run('upgrade').stdout);assert preview['source_schema']==a.source_schema and preview['target_schema']==18 and not preview['executed']
   assert sql('SELECT version FROM schema_version')==str(a.source_schema)
   key=root/'recovery.key';run('recovery-key',key)
   point=root/'point.enc';sql("UPDATE settings SET description='Changed after review'")
   run('upgrade','--execute',preview['plan'],'--recovery-output',point,'--key-file',key,ok=False);assert not point.exists() and sql('SELECT version FROM schema_version')==str(a.source_schema)
   preview=json.loads(run('upgrade').stdout);point.write_bytes(b'Owner record');point.chmod(0o600)
   run('upgrade','--execute',preview['plan'],'--recovery-output',point,'--key-file',key,ok=False);assert point.read_bytes()==b'Owner record' and sql('SELECT version FROM schema_version')==str(a.source_schema);point.unlink()
   result=json.loads(run('upgrade','--execute',preview['plan'],'--recovery-output',point,'--key-file',key).stdout)
   assert result['executed'] and result['recovery_sha256']==hashlib.sha256(point.read_bytes()).hexdigest()
   assert point.stat().st_mode&0o077==0 and sql('SELECT version FROM schema_version')=='18' and sql('SELECT COUNT(*) FROM process_authority')=='1'
   assert sql('SELECT COUNT(*) FROM posts')==before and sql('SELECT COUNT(*) FROM shop_products')==products and sql('SELECT description FROM settings')=='Changed after review'
   run('recovery-inspect',point,'--key-file',key,'--source-format','13' if a.source_schema==17 else '12')
   # Interruption/retry: a failed precondition preserves schema/data and an exact
   # subsequent review succeeds. No old executable may open upgraded data.
   fresh_url=database(root,'restored',postgres);fresh=root/'restored.toml';fresh.write_text(f'database_url={json.dumps(fresh_url)}\ndata_dir={json.dumps(str(root/"restored"))}\n');fresh.chmod(0o600)
   restore_point=point
   if not a.source_binary:
    restore_point=root/'converted.enc'
    run('migrate-recovery-v13' if a.source_schema==17 else 'migrate-recovery-v12',point,restore_point,'--key-file',key)
   run('restore',restore_point,'--key-file',key,config=fresh,executable=source_binary)
   recovered=root/'recovered.json';run('backup',recovered,config=fresh,executable=source_binary)
   graph=json.loads(json.loads(recovered.read_text())['payload'])
   assert len(graph['tables']['posts'])==int(before) and len(graph['tables']['shop_products'])==int(products)
   assert graph['tables']['settings'][0]['description']=='Changed after review'
   unsupported_config=cfg
   if postgres:
    with socket.socket() as socket_:socket_.bind(('127.0.0.1',0));http_port=socket_.getsockname()[1]
    source_origin=f'http://127.0.0.1:{http_port}'
    cfg.write_text(cfg.read_text()+f'listen="127.0.0.1:{http_port}"\nbase_url="{source_origin}"\n')
    rebound=root/'rebound.toml';rebound.write_text(f'database_url={json.dumps(url)}\ndata_dir={json.dumps(str(root/"rebound-directory"))}\nbase_url="{source_origin}"\n');rebound.chmod(0o600)
    child=subprocess.Popen([binary,'--config',str(cfg),'serve','--external-worker'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=env)
    try:
     for attempt in range(100):
      assert child.poll() is None,'Source node failed before readiness'
      try:
       with urllib.request.urlopen(source_origin+'/health',timeout=1):break
      except OSError:time.sleep(.05)
     else:raise AssertionError('Source readiness timeout')
     run('upgrade','--rebind-directory','--source-origin',source_origin,config=rebound,ok=False)
    finally:
     child.terminate();child.wait(timeout=15);assert child.returncode==0
    run('job-history',config=rebound,ok=False)
    rebind=json.loads(run('upgrade','--rebind-directory','--source-origin',source_origin,config=rebound).stdout)
    rebind_point=root/'before-rebind.enc'
    run('upgrade','--rebind-directory','--source-origin',source_origin,'--execute',rebind['plan'],'--recovery-output',rebind_point,'--key-file',key,config=rebound,ok=False)
    assert not rebind_point.exists()
    outcome=json.loads(run('upgrade','--rebind-directory','--source-origin',source_origin,'--execute',rebind['plan'],'--recovery-output',rebind_point,'--key-file',key,'--acknowledge-source-stopped',config=rebound).stdout)
    assert outcome['held'] and sql('SELECT held FROM recovery_mode')=='1' and sql('SELECT COUNT(*) FROM sessions')=='0' and sql('SELECT COUNT(*) FROM integration_credentials')=='0'
    run('shop','report',config=rebound,ok=False)
    run('clone-activate','--review','Source node stopped; restored identities and credentials reviewed, matching media and domain graph verified, queues and external effects reconciled.',config=rebound)
    assert sql('SELECT held FROM recovery_mode')=='0'
    run('job-history',config=cfg,ok=False)
    unsupported_config=rebound
   sql('UPDATE schema_version SET version=999 WHERE id=1');run('upgrade',config=unsupported_config,ok=False);assert sql('SELECT version FROM schema_version')=='999'
 print('Source boundary:', 'independently retained native schema-'+str(a.source_schema)+' executable and fresh-target rollback' if a.source_binary else 'schema-equivalent fixture; independent old executable not run')
 print('PASS: data-bearing SQLite/PostgreSQL schema maintenance, runtime refusal, stale/existing recovery refusal, exact execution and fresh recovery')
finally:
 for schema in schemas:pg('DROP SCHEMA '+schema+' CASCADE')
