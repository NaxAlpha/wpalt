#!/usr/bin/env python3
"""Independent real-site observation, revoked credentials and origin outage isolation."""
import argparse,json,os,secrets,socket,subprocess,tempfile,time,urllib.request
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);a=p.parse_args();binary=str(Path(a.binary).resolve())
env={k:v for k,v in os.environ.items() if not k.startswith('WPALT_')};servers=[];logs=[]
with tempfile.TemporaryDirectory(prefix='wpalt-fleet-') as temporary:
 root=Path(temporary);nodes=[];configs=[];tokens=[]
 def native(cfg,*args,stdin=None,ok=True):
  result=subprocess.run([binary,'--config',str(cfg),*map(str,args)],input=stdin,text=True,capture_output=True,env=env,timeout=30)
  assert (result.returncode==0)==ok,'Unexpected native command outcome';return result
 def inspect(manifest,ok=True):
  result=subprocess.run([binary,'--data-dir',str(root/'unconfigured-site'),'fleet-inspect',str(manifest)],capture_output=True,text=True,env=env,timeout=30)
  assert (result.returncode==0)==ok,'Unexpected fleet observation outcome'
  assert all(token not in result.stdout+result.stderr for token in tokens)
  assert 'Synthetic private title' not in result.stdout+result.stderr
  return json.loads(result.stdout) if result.stdout.strip() else None
 def write_manifest(value,name='fleet.json'):
  path=root/name;path.write_text(json.dumps(value));path.chmod(0o600);return path
 try:
  for index in range(2):
   with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
   origin=f'http://127.0.0.1:{port}';cfg=root/f'site-{index}.toml';cfg.write_text(f'database_url="sqlite://{root}/site-{index}.db?mode=rwc"\ndata_dir="{root}/data-{index}"\nbase_url="{origin}"\nlisten="127.0.0.1:{port}"\n');cfg.chmod(0o600);configs.append(cfg)
   native(cfg,'init','--admin-email','owner@example.test',stdin=secrets.token_urlsafe(24)+'\n')
   native(cfg,'seed-demo','--posts','1')
   token_file=root/f'node-{index}.token';native(cfg,'integration','create','--user-email','owner@example.test','--name','Independent fleet read-only',token_file);tokens.append(token_file.read_text().strip())
   # Synthetic draft metadata should be observable to the delegated integration,
   # while the final operator report must not copy that private metadata.
   import sqlite3
   with sqlite3.connect(root/f'site-{index}.db') as db:db.execute("UPDATE posts SET title='Synthetic private title',status='draft'")
   log=(root/f'node-{index}.log').open('w');logs.append(log);server=subprocess.Popen([binary,'--config',str(cfg),'serve','--external-worker'],stdout=log,stderr=log,env=env);servers.append(server)
   deadline=time.monotonic()+10
   while time.monotonic()<deadline:
    assert server.poll() is None
    try:
     with urllib.request.urlopen(origin+'/health',timeout=1):break
    except OSError:time.sleep(.05)
   else:raise AssertionError('Disposable node did not become ready')
   nodes.append({'name':f'node-{index}','origin':origin,'token_file':token_file.name})
  manifest_value={'format':'wpalt-fleet-v1','nodes':nodes};manifest=write_manifest(manifest_value)
  observed=inspect(manifest);assert observed['all_ready'] and len(observed['nodes'])==2
  # Stopping one actual CMS must not stop the independent observer or hide the other.
  servers[0].terminate();servers[0].wait(timeout=15)
  observed=inspect(manifest,ok=False);assert not observed['all_ready']
  assert [node['status'] for node in observed['nodes']]==['unavailable-or-unauthorized','ready']
  assert observed['nodes'][0]['failure_code']=='connection-failed' and observed['nodes'][1]['failure_code'] is None
  # A healthy origin is not sufficient when its delegated identity was revoked.
  servers[1].terminate();servers[1].wait(timeout=15)
  import sqlite3
  with sqlite3.connect(root/'site-1.db') as db:db.execute('DELETE FROM integration_credentials')
  server=subprocess.Popen([binary,'--config',str(configs[1]),'serve','--external-worker'],stdout=logs[1],stderr=logs[1],env=env);servers.append(server)
  deadline=time.monotonic()+10
  while time.monotonic()<deadline:
   try:
    with urllib.request.urlopen(nodes[1]['origin']+'/health',timeout=1):break
   except OSError:time.sleep(.05)
  observed=inspect(manifest,ok=False);assert all(node['status']=='unavailable-or-unauthorized' for node in observed['nodes'])
  assert observed['nodes'][1]['failure_code']=='credential-refused'
  # Redirecting authenticated transport must never forward a delegated credential.
  from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
  import threading
  forwarded=[];initial=[]
  class Destination(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def do_GET(self):forwarded.append(self.headers.get('Authorization'));self.send_response(200);self.end_headers()
  destination=ThreadingHTTPServer(('127.0.0.1',0),Destination)
  class Redirect(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def do_GET(self):
    if self.path=='/health':
     raw=b'{"status":"ok","version":"0.1.0"}';self.send_response(200);self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
    else:
     initial.append(self.headers.get('Authorization'));self.send_response(302);self.send_header('Location',f'http://127.0.0.1:{destination.server_port}/stolen');self.end_headers()
  redirect=ThreadingHTTPServer(('127.0.0.1',0),Redirect);threads=[]
  for endpoint in [destination,redirect]:
   thread=threading.Thread(target=endpoint.serve_forever,daemon=True);thread.start();threads.append(thread)
  try:
   redirected={'format':'wpalt-fleet-v1','nodes':[{'name':'redirect-fixture','origin':f'http://127.0.0.1:{redirect.server_port}','token_file':nodes[0]['token_file']}]}
   observed=inspect(write_manifest(redirected,'redirect.json'),ok=False);assert not observed['all_ready'] and observed['nodes'][0]['failure_code']=='redirect-refused' and initial==['Bearer '+tokens[0]] and forwarded==[]
  finally:
   for endpoint in [destination,redirect]:endpoint.shutdown();endpoint.server_close()
   for thread in threads:thread.join(timeout=5)
  invalid=json.loads(json.dumps(manifest_value));invalid['nodes'][0]['origin']='http://example.org';assert inspect(write_manifest(invalid,'invalid-origin.json'),ok=False) is None
  manifest.chmod(0o644);assert inspect(manifest,ok=False) is None;manifest.chmod(0o600)
  linked=root/'linked.json';linked.symlink_to(manifest);assert inspect(linked,ok=False) is None
  linked.unlink();os.link(manifest,linked);assert inspect(manifest,ok=False) is None;linked.unlink()
  invalid=json.loads(json.dumps(manifest_value));invalid['nodes']*=17;assert inspect(write_manifest(invalid,'too-many.json'),ok=False) is None
  # CLI uses no default-site database even while inspecting disconnected origins.
  assert not (root/'unconfigured-site').exists()
  print('PASS: two native sites, outage isolation, revoked grant, private bounded input and secret-free independent report')
 finally:
  for server in servers:
   if server.poll() is None:server.terminate();server.wait(timeout=15)
  for log in logs:log.close()
