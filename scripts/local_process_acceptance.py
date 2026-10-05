#!/usr/bin/env python3
"""Two real processes: shared authority/cache, conflict, worker and crash reconciliation.
Uses a disposable PostgreSQL schema and synthetic content, never an existing site.
"""
import argparse, concurrent.futures, hashlib, http.cookiejar, json, os, re, secrets, shutil
import socket, subprocess, tempfile, time, urllib.error, urllib.parse, urllib.request
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--psql',default=shutil.which('psql'));p.add_argument('--output',default='work/m9-local-process-reference.json');a=p.parse_args()
root_url=os.environ.get('TEST_DATABASE_URL');assert root_url and a.psql,'Requires TEST_DATABASE_URL and psql'
binary=str(Path(a.binary).resolve());schema='wpalt_process_'+secrets.token_hex(12)
env={k:v for k,v in os.environ.items() if not k.startswith('WPALT_')}
def sql(text, url=root_url):
 r=subprocess.run([a.psql,url,'-X','-qAt','-v','ON_ERROR_STOP=1','-c',text],capture_output=True,text=True,timeout=30);assert r.returncode==0,'Disposable database fixture failed';return r.stdout.strip()
sql('CREATE SCHEMA '+schema)
u=urllib.parse.urlsplit(root_url);query=urllib.parse.parse_qsl(u.query);query.append(('options','-c search_path='+schema));url=urllib.parse.urlunsplit(u._replace(query=urllib.parse.urlencode(query,quote_via=urllib.parse.quote)))
class NoRedirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,*args,**kwargs):return None
opener=urllib.request.build_opener(NoRedirect)
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
processes=[];holder=None;logfiles=[];started=time.monotonic()
try:
 with tempfile.TemporaryDirectory(prefix='wpalt-process-') as temporary:
  root=Path(temporary);data=root/'site';ports=[port(),port()];origin='http://127.0.0.1:'+str(ports[0]);password=secrets.token_urlsafe(24)
  def config(name, index=0, local=True):
   cfg=root/(name+'.toml');cfg.write_text(f'database_url={json.dumps(url)}\ndata_dir={json.dumps(str(data))}\nbase_url={json.dumps(origin)}\nlisten="127.0.0.1:{ports[index]}"\nlocal_processes={str(local).lower()}\ndebug=true\nrequest_timeout_seconds=3\nscheduler_seconds=3600\n[cache]\nenabled=true\n[spam]\nenabled=true\nproof_bits=0\n');cfg.chmod(0o600);return cfg
  initial=config('initialize',local=False);configs=[config('node-a'),config('node-b',1)]
  def run(*args,cfg=configs[0],stdin=None,ok=True):
   r=subprocess.run([binary,'--config',str(cfg),*map(str,args)],input=stdin,text=True,capture_output=True,env=env,timeout=45);assert (r.returncode==0)==ok,'Native CLI outcome differs from expected';return r
  run('init','--admin-email','owner@example.test',cfg=initial,stdin=password+'\n')
  run('seed-demo','--posts','10',cfg=initial)
  def request(index,path,method='GET',body=None,headers=None,status=200):
   h={'Origin':origin};h.update(headers or {});raw=None
   if body is not None:raw=json.dumps(body).encode();h['Content-Type']='application/json'
   req=urllib.request.Request(f'http://127.0.0.1:{ports[index]}'+path,method=method,headers=h,data=raw)
   try:response=opener.open(req,timeout=15)
   except urllib.error.HTTPError as e:response=e
   with response:
    result=response.read();assert response.status==status,(path,response.status,result[:300]);return response.headers,result
  def start(index):
   log=(root/f'node-{index}.log').open('a');logfiles.append(log)
   node=subprocess.Popen([binary,'--config',str(configs[index]),'serve','--external-worker'],stdout=log,stderr=log,env=env);processes.append(node)
   deadline=time.monotonic()+15
   while time.monotonic()<deadline:
    assert node.poll() is None,'Node exited before readiness'
    try:request(index,'/health');return node
    except (OSError,AssertionError):time.sleep(.05)
   raise AssertionError('Node did not become ready')
  def stop(node):
   if node.poll() is None:node.terminate();node.wait(timeout=15);assert node.returncode==0,'Normal node shutdown failed'
  # A different coordination root must be rejected for the same native database.
  wrong=root/'wrong-root.toml';wrong.write_text(configs[1].read_text().replace(str(data),str(root/'other-site')));wrong.chmod(0o600)
  run('serve','--external-worker',cfg=wrong,ok=False)
  nodes=[start(0),start(1)]
  expected_name='wpalt:'+hashlib.sha256(os.fsencode(data.resolve())).hexdigest()[:56]
  names=sql(f"SELECT application_name FROM pg_stat_activity WHERE application_name='{expected_name}'").splitlines()
  assert len(names)>=2 and all(name==expected_name and len(name)==62 for name in names), 'Both database operation identities must exactly match the bounded site identity'
  login=urllib.request.Request(origin+'/login',method='POST',headers={'Origin':origin,'Content-Type':'application/x-www-form-urlencoded'},data=urllib.parse.urlencode({'email':'owner@example.test','password':password}).encode())
  try:response=opener.open(login,timeout=15)
  except urllib.error.HTTPError as e:response=e
  assert response.status==303;cookie=response.headers['Set-Cookie'].split(';')[0];response.close()
  _,html=request(1,'/admin',headers={'Cookie':cookie});csrf=re.search(rb'name="csrf" value="([^"]+)"',html).group(1).decode()
  owner={'Cookie':cookie,'X-CSRF-Token':csrf}
  post={'title':'Process reference','slug':'process-reference','kind':'post','body':'Shared original body','action':'publish'}
  _,raw=request(0,'/api/admin/content','POST',post,owner);saved=json.loads(raw);id_=saved['id']
  request(1,'/process-reference');headers,raw=request(1,'/process-reference');assert headers['X-Wpalt-Cache']=='hit' and b'Shared original body' in raw
  changed=dict(post,version=saved['version'],body='Shared revised body')
  _,raw=request(0,'/api/admin/content/'+id_,'POST',changed,owner);saved=json.loads(raw)
  headers,raw=request(1,'/process-reference');assert headers['X-Wpalt-Cache']=='miss' and b'Shared revised body' in raw and b'Shared original body' not in raw
  # Both writes use one reviewed version. At most one may replace it.
  def compete(index):
   r=urllib.request.Request(f'http://127.0.0.1:{ports[index]}/api/admin/content/{id_}',method='POST',headers={**owner,'Origin':origin,'Content-Type':'application/json'},data=json.dumps(dict(post,version=saved['version'],body='Concurrent proposal '+str(index))).encode())
   try:res=opener.open(r,timeout=15)
   except urllib.error.HTTPError as e:res=e
   with res:return res.status,res.read()
  with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:outcomes=list(pool.map(compete,[0,1]))
  assert sorted(status for status,_ in outcomes)==[200,409]
  saved=json.loads(next(raw for status,raw in outcomes if status==200))
  # Shared spam token is issued on A and accepted on B, with replay denied on A.
  _,raw=request(0,'/api/spam/challenge','POST',{'resource':'comment:process-reference'});challenge=json.loads(raw)
  comment={'name':'Synthetic reader','email':'reader@example.test','body':'A useful synthetic comment','token':challenge['token'],'solution':'0','website':''}
  comment_data=urllib.parse.urlencode(comment).encode()
  def submit(index,status):
   req=urllib.request.Request(f'http://127.0.0.1:{ports[index]}/process-reference/comments',method='POST',headers={'Origin':origin,'Content-Type':'application/x-www-form-urlencoded'},data=comment_data)
   try:res=opener.open(req,timeout=15)
   except urllib.error.HTTPError as e:res=e
   with res:assert res.status==status,(res.status,res.read()[:300])
  submit(1,200);submit(0,403)
  # A body timeout before mutation admission must not create an operator-only pause.
  with socket.create_connection(('127.0.0.1',ports[0]),timeout=8) as slow:
   slow.settimeout(8)
   slow.sendall((f'POST /login HTTP/1.1\r\nHost: 127.0.0.1:{ports[0]}\r\nOrigin: {origin}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 10000\r\nConnection: close\r\n\r\nemail=slow@example.test').encode())
   response=slow.recv(4096);assert b'408' in response.split(b'\r\n',1)[0], 'Incomplete body must time out'
  request(1,'/health');assert not (data/'.process-intent').exists()
  # Alternate nodes; the ninth failure must observe the shared eight-attempt budget.
  for attempt in range(9):
   req=urllib.request.Request(f'http://127.0.0.1:{ports[attempt%2]}/login',method='POST',headers={'Origin':origin,'Content-Type':'application/x-www-form-urlencoded'},data=urllib.parse.urlencode({'email':'unknown-budget@example.test','password':'synthetic-wrong-password'}).encode())
   try:res=opener.open(req,timeout=10)
   except urllib.error.HTTPError as e:res=e
   with res:assert res.status==(401 if attempt<8 else 429),(attempt,res.status)
  run('worker','--once')
  run('backup',root/'blocked-backup.json',ok=False);assert not (root/'blocked-backup.json').exists()
  # Deterministic crash: PostgreSQL prevents the target write from completing.
  holder=subprocess.Popen([a.psql,url,'-X','-qAt','-v','ON_ERROR_STOP=1'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  holder.stdin.write("BEGIN; LOCK TABLE posts IN ACCESS EXCLUSIVE MODE; SELECT 'held';\n");holder.stdin.flush();line=holder.stdout.readline().strip();assert line=='held', ('Lock fixture response',line,holder.stderr.read() if holder.poll() is not None else 'still running')
  pool=concurrent.futures.ThreadPoolExecutor(max_workers=1)
  pending=pool.submit(request,0,'/api/admin/content/'+id_,'POST',dict(post,version=saved['version'],body='Must not infer crash completion'),owner)
  deadline=time.monotonic()+8
  while time.monotonic()<deadline:
   blocked=sql("SELECT COUNT(*) FROM pg_stat_activity WHERE wait_event_type='Lock' AND application_name LIKE 'wpalt:%' AND query LIKE '%posts%'")
   if blocked!='0' and (data/'.process-intent').exists():break
   time.sleep(.03)
  else:raise AssertionError('Crash fixture did not reach a blocked domain read')
  nodes[0].kill();nodes[0].wait(timeout=10)
  headers,_=request(1,'/health',status=503);assert headers.get('Cache-Control')=='no-store' and headers.get('Content-Security-Policy') and headers.get('X-Request-Id')
  run('local-resume',ok=False);assert (data/'.process-intent').exists()
  holder.stdin.write('ROLLBACK;\n\\q\n');holder.stdin.flush();holder.wait(timeout=10);holder=None
  try:pending.result(timeout=10)
  except (OSError,AssertionError):pass
  else:raise AssertionError('Killed writer reported success')
  pool.shutdown(wait=True);stop(nodes[1])
  preview=json.loads(run('local-resume').stdout);assert preview['paused']
  run('local-resume','--execute','stale','--acknowledge-external-effects',ok=False)
  run('local-resume','--execute',preview['plan'],ok=False);assert (data/'.process-intent').exists()
  run('local-resume','--execute',preview['plan'],'--acknowledge-external-effects');assert not (data/'.process-intent').exists()
  nodes=[start(0),start(1)];_,raw=request(1,'/process-reference');assert b'Must not infer crash completion' not in raw
  for node in nodes:stop(node)
  state=json.loads((data/'.process-state.json').read_text());assert (data/'.process-state.json').stat().st_mode&0o077==0
  assert len(state['ceremonies']['entries'])==0
  for f in logfiles:f.flush()
  logs=''.join((root/f'node-{i}.log').read_text() for i in [0,1]);assert password not in logs and challenge['token'] not in logs
  report={'format':'wpalt-m9-local-process-reference-v1','status':'passed','postgres_version':sql('SHOW server_version'),'processes':2,'elapsed_seconds':round(time.monotonic()-started,3),'assertions':['database rejects a second coordination directory','shared session and CSRF across nodes','cross-node cached publication invalidation','one winner for concurrent reviewed version','shared spam challenge and one-use replay protection','untrusted incomplete body times out without persistent site pause','login abuse budget spans alternating nodes','independent coordinated worker','offline lifecycle excludes running nodes','deterministic blocked-writer kill pauses other node','private security headers/correlated pause diagnostics','stale/no-ack resume fails','exact offline graph/external-effect reconciliation','restart preserves committed state and excludes blocked write','private server-side state and redacted logs'],'limits':['Same Unix host and exact shared site/configuration; serialized admission, not multi-host leases.','External effects require separate owner reconciliation; no exactly-once network claim.','Performance and remaining M9 workflows require their own evidence.']}
  out=Path(a.output);out.parent.mkdir(parents=True,exist_ok=True);out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
finally:
 if holder and holder.poll() is None:holder.kill();holder.wait(timeout=10)
 for node in processes:
  if node.poll() is None:node.terminate();node.wait(timeout=15)
 for logfile in logfiles:logfile.close()
 sql('DROP SCHEMA '+schema+' CASCADE')
