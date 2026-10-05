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
  run('shop','seed-demo',cfg=initial)
  run('user-add','--email','competitor@example.test','--name','Synthetic competitor','--role','subscriber',cfg=initial,stdin=password+'\n')
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
  if os.uname().sysname=='Linux':
   changed_binary=root/'same-version-different-bytes';shutil.copyfile(binary,changed_binary)
   with changed_binary.open('ab') as f:f.write(b'wpalt M9 executable identity fixture')
   changed_binary.chmod(0o700)
   refusal=subprocess.run([str(changed_binary),'--config',str(configs[1]),'serve','--external-worker'],capture_output=True,text=True,env=env,timeout=30)
   assert refusal.returncode!=0 and 'format-or-configuration-mismatch' in refusal.stderr, 'Same version/schema with different executable bytes must be refused by authority, not port binding'
  login=urllib.request.Request(origin+'/login',method='POST',headers={'Origin':origin,'Content-Type':'application/x-www-form-urlencoded'},data=urllib.parse.urlencode({'email':'owner@example.test','password':password}).encode())
  try:response=opener.open(login,timeout=15)
  except urllib.error.HTTPError as e:response=e
  assert response.status==303;cookie=response.headers['Set-Cookie'].split(';')[0];response.close()
  _,html=request(1,'/admin',headers={'Cookie':cookie});csrf=re.search(rb'name="csrf" value="([^"]+)"',html).group(1).decode()
  owner={'Cookie':cookie,'X-CSRF-Token':csrf}
  def form_request(index,path,values,cookie_value):
   req=urllib.request.Request(f'http://127.0.0.1:{ports[index]}'+path,method='POST',headers={'Origin':origin,'Cookie':cookie_value,'Content-Type':'application/x-www-form-urlencoded'},data=urllib.parse.urlencode(values).encode())
   try:res=opener.open(req,timeout=15)
   except urllib.error.HTTPError as e:res=e
   with res:return res.status,res.headers,res.read()
  status,other_headers,_=form_request(1,'/login',{'email':'competitor@example.test','password':password},'');assert status==303
  shopper_cookies=[cookie,other_headers['Set-Cookie'].split(';')[0]]
  def hidden(raw):return dict(re.findall(r'name="([^"\s]+)" value="([^"<>]*)"',raw.decode()))
  # Three repeated real-node competitions for each domain, not a mock-lock proof.
  for kind in ['physical','booking']:
   product_id,variant_id=sql(f"SELECT p.id||','||v.id FROM shop_products p JOIN shop_variants v ON v.product_id=p.id WHERE p.kind='{kind}'",url).split(',')
   slot_id=sql(f"SELECT id FROM shop_slots WHERE variant_id='{variant_id}'",url) if kind=='booking' else ''
   for iteration in range(3):
    if kind=='physical':sql(f"UPDATE shop_variants SET stock_total=held+sold+1 WHERE id='{variant_id}'",url)
    else:sql(f"UPDATE shop_slots SET capacity=held+booked+1 WHERE id='{slot_id}'",url)
    checkouts=[]
    for index in [0,1]:
     _,raw=request(index,'/shop/products/'+product_id,headers={'Cookie':shopper_cookies[index]});fields=hidden(raw)
     status,_,_=form_request(index,'/shop/cart',{'csrf':fields['csrf'],'version':fields['version'],'variant_id':variant_id,'slot_id':slot_id,'quantity':'1'},shopper_cookies[index]);assert status==303
     _,raw=request(index,'/shop/cart',headers={'Cookie':shopper_cookies[index]})
     checkout=re.search(rb'<form[^>]*action="/shop/checkout"[^>]*>(.*?)</form>',raw,re.S);assert checkout,'Both competitors must review terms while one unit/seat remains'
     payload=hidden(checkout.group(1));payload.update(provider='offline',shipping_address='Synthetic test address' if kind=='physical' else '')
     checkouts.append(payload)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
     futures=[pool.submit(form_request,index,'/shop/checkout',checkouts[index],shopper_cookies[index]) for index in [0,1]];outcomes=[future.result() for future in futures]
    winners=[index for index,result in enumerate(outcomes) if result[0]==303];assert len(winners)==1 and all(result[0] in (303,409,422) for result in outcomes),'Last allocation must have exactly one authoritative winner'
    winner=winners[0];order_path=outcomes[winner][1]['Location'];order_id=order_path.rsplit('/',1)[-1]
    # Retrying the accepted request on the other node returns the same order.
    status,replay_headers,_=form_request(1-winner,'/shop/checkout',checkouts[winner],shopper_cookies[winner]);assert status==303 and replay_headers['Location']==order_path
    version=sql(f"SELECT version FROM shop_orders WHERE id='{order_id}'",url)
    receipt={'csrf':csrf,'action':'paid','version':version,'reference':f'SYNTHETIC-{kind}-{iteration}'}
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
     futures=[pool.submit(form_request,index,'/admin/shop/orders/'+order_id,receipt,cookie) for index in [0,1]];paid=[future.result()[0] for future in futures]
    assert sorted(paid)==[303,409], 'Concurrent owner receipts must not duplicate money or allocation'
    assert sql(f"SELECT payment_state FROM shop_orders WHERE id='{order_id}'",url)=='paid'
    if kind=='physical':assert sql(f"SELECT held||','||sold||','||stock_total FROM shop_variants WHERE id='{variant_id}'",url)==f'0,{iteration+1},{iteration+1}'
    else:assert sql(f"SELECT held||','||booked||','||capacity FROM shop_slots WHERE id='{slot_id}'",url)==f'0,{iteration+1},{iteration+1}'
    for index in [0,1]:
     _,raw=request(index,'/shop/products/'+product_id,headers={'Cookie':shopper_cookies[index]});fields=hidden(raw)
     status,_,_=form_request(index,'/shop/cart',{'csrf':fields['csrf'],'version':fields['version'],'variant_id':variant_id,'slot_id':slot_id,'quantity':'0'},shopper_cookies[index]);assert status==303
  # Independent anti-spam proofs permit retrying one accepted form request key.
  status,created_headers,_=form_request(0,'/admin/forms',{'csrf':csrf,'title':'Cross-node response'},cookie);assert status==303
  form_id=created_headers['Location'].rsplit('/',1)[-1]
  _,raw=request(1,'/api/admin/forms/'+form_id,headers=owner);state=json.loads(raw)
  request(0,'/api/admin/forms/'+form_id,'POST',{'csrf':csrf,'version':state['version'],'definition':state['definition'],'publish':True},owner)
  for iteration in range(3):
   submissions=[]
   for index in [0,1]:
    _,raw=request(index,'/api/spam/challenge','POST',{'resource':'form:'+form_id});challenge=json.loads(raw)
    submissions.append({'version':state['version']+1,'key':str(__import__('uuid').uuid4()) if index==0 else submissions[0]['key'],'values':{'message':'Synthetic shared response '+str(iteration)},'spam':{'token':challenge['token'],'solution':'0','website':''}})
   with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
    futures=[pool.submit(request,index,'/api/forms/'+form_id+'/entries','POST',submissions[index]) for index in [0,1]];accepted=[json.loads(f.result()[1]) for f in futures]
   assert accepted[0]['entry']==accepted[1]['entry'] and sql(f"SELECT COUNT(*) FROM form_entries WHERE form_id='{form_id}'",url)==str(iteration+1)
  post={'title':'Process reference','slug':'process-reference','kind':'post','body':'Shared original body','action':'publish'}
  _,raw=request(0,'/api/admin/content','POST',post,owner);saved=json.loads(raw);id_=saved['id']
  request(1,'/process-reference');unchanged_state=(data/'.process-state.json').stat().st_mtime_ns;headers,raw=request(1,'/process-reference');cached_request_id=headers['X-Request-Id'];assert (data/'.process-state.json').stat().st_mtime_ns==unchanged_state,'Read-only cache hit must not rewrite unchanged durable authority';assert headers['X-Wpalt-Cache']=='hit' and b'Shared original body' in raw
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
  # Withdraw a cached publication on A; B must immediately stop serving it.
  request(1,'/process-reference');request(1,'/process-reference')
  _,raw=request(0,'/api/admin/content/'+id_,'POST',dict(post,version=saved['version'],action='unpublish'),owner);saved=json.loads(raw)
  _,raw=request(1,'/process-reference',status=404);assert b'Concurrent proposal' not in raw
  _,raw=request(0,'/api/admin/content/'+id_,'POST',dict(post,version=saved['version']),owner);saved=json.loads(raw)
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
   blocked=sql(f"SELECT COUNT(*) FROM pg_stat_activity WHERE wait_event_type='Lock' AND application_name='{expected_name}' AND query LIKE '%posts%'")
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
  # Logging out on B invalidates the same credential on A before another write.
  logout=urllib.request.Request(f'http://127.0.0.1:{ports[1]}/logout',method='POST',headers={'Origin':origin,'Cookie':cookie,'Content-Type':'application/x-www-form-urlencoded'},data=urllib.parse.urlencode({'csrf':csrf}).encode())
  try:response=opener.open(logout,timeout=15)
  except urllib.error.HTTPError as e:response=e
  with response:assert response.status==303
  request(0,'/api/admin/content','POST',post,owner,status=401)
  for node in nodes:stop(node)
  # Crash three actual worker cycles, reconcile offline, then retry immediately.
  # Private fixture timestamps advance due work without changing runtime config.
  for iteration in range(4):
   history_file=data/'background-jobs.json'
   if history_file.exists():
    history=json.loads(history_file.read_text());history[0]['started_at']=int(time.time())-3601;history_file.write_text(json.dumps(history))
   holder=subprocess.Popen([a.psql,url,'-X','-qAt','-v','ON_ERROR_STOP=1'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
   holder.stdin.write("BEGIN; LOCK TABLE posts IN ACCESS EXCLUSIVE MODE; SELECT 'held';\n");holder.stdin.flush();assert holder.stdout.readline().strip()=='held'
   worker_log=(root/f'worker-{iteration}.log').open('w');logfiles.append(worker_log)
   worker=subprocess.Popen([binary,'--config',str(configs[0]),'worker','--once'],stdout=worker_log,stderr=worker_log,env=env);processes.append(worker)
   deadline=time.monotonic()+10
   while time.monotonic()<deadline:
    assert worker.poll() is None, 'Worker exited before its controlled native stage'
    blocked=sql(f"SELECT COUNT(*) FROM pg_stat_activity WHERE wait_event_type='Lock' AND application_name='{expected_name}' AND query LIKE '%posts%'")
    if blocked!='0' and (data/'.process-intent').exists():break
    time.sleep(.03)
   else:raise AssertionError('Worker fixture did not reach a held native publication stage')
   if iteration<3:
    worker.kill();worker.wait(timeout=10)
   else:
    worker.terminate()
    try:worker.wait(timeout=.15)
    except subprocess.TimeoutExpired:pass
    else:raise AssertionError('Graceful shutdown abandoned an active native worker stage')
   holder.stdin.write('ROLLBACK;\n\\q\n');holder.stdin.flush();holder.wait(timeout=10);holder=None
   if iteration<3:
    assert (data/'.process-intent').exists()
    preview=json.loads(run('local-resume').stdout);assert preview['paused']
    run('local-resume','--execute',preview['plan'],'--acknowledge-external-effects')
    run('worker','--once')
    history=json.loads(history_file.read_text());assert history[0]['state']=='succeeded' and history[1]['state']=='interrupted','Reconciled worker must retry now, not retain a running cycle for one scheduler interval'
   else:
    worker.wait(timeout=15);assert worker.returncode==0 and not (data/'.process-intent').exists()
    assert json.loads(history_file.read_text())[0]['state']=='succeeded'
  lifecycle=data/'.wpalt.lock';retained=root/'retained-lifecycle';lifecycle.rename(retained)
  victim=root/'unrelated-owner-record';victim.write_bytes(b'unchanged');victim.chmod(0o600)
  lifecycle.symlink_to(victim);run('job-history',ok=False);assert victim.read_bytes()==b'unchanged';lifecycle.unlink()
  os.link(victim,lifecycle);run('job-history',ok=False);assert victim.read_bytes()==b'unchanged';lifecycle.unlink();retained.rename(lifecycle)
  state=json.loads((data/'.process-state.json').read_text());assert (data/'.process-state.json').stat().st_mode&0o077==0
  assert len(state['ceremonies']['entries'])==0
  for f in logfiles:f.flush()
  logs=''.join((root/f'node-{i}.log').read_text() for i in [0,1]);assert password not in logs and challenge['token'] not in logs
  events=[json.loads(line) for line in logs.splitlines() if line.startswith('{')]
  coordinated=[row['fields'] for row in events if row.get('fields',{}).get('event')=='coordinated_request_completed' and row['fields'].get('request_id')==cached_request_id]
  native=[row['fields'] for row in events if row.get('fields',{}).get('event')=='request_completed' and row['fields'].get('request_id')==cached_request_id]
  assert len(coordinated)==len(native)==1 and coordinated[0]['elapsed_us']>=coordinated[0]['admission_us']+coordinated[0]['finalize_us'] and coordinated[0]['elapsed_us']>=native[0]['elapsed_us'],'Queue/finalization performance must be correlated with native request identity'
  report={'format' :'wpalt-m9-local-process-reference-v1','status':'passed','postgres_version':sql('SHOW server_version'),'processes':2,'elapsed_seconds':round(time.monotonic()-started,3),'assertions':['database rejects a second coordination directory','shared session and CSRF across nodes','withdrawn cached publication immediately refused on another node','logout revocation blocks cross-node writes','cross-node cached publication invalidation','one winner for concurrent reviewed version','three cross-node last-stock competitions preserve one winner and one receipt','three cross-node last-seat competitions preserve one winner and one receipt','three concurrent form retries preserve one entry per reviewed request key','shared spam challenge and one-use replay protection','untrusted incomplete body times out without persistent site pause','login abuse budget spans alternating nodes','independent coordinated worker','three terminated native worker cycles require reconciliation and immediate safe retry','graceful worker shutdown drains its blocked stage and clears durable intent','offline lifecycle excludes running nodes','deterministic blocked-writer kill pauses other node','private security headers/correlated pause diagnostics','stale/no-ack resume fails','exact offline graph/external-effect reconciliation','restart preserves committed state and excludes blocked write','private server-side state and redacted logs','unchanged read-only state avoids redundant durable writes','queue/finalization timing shares native request identity','lifecycle symlink/hardlink refusal preserves unrelated owner files',*(['same-version/schema executable mismatch refused before serving'] if os.uname().sysname=='Linux' else [])],'limits':['Same Unix host and exact shared site/configuration; serialized admission, not multi-host leases.','External effects require separate owner reconciliation; no exactly-once network claim.','Performance and remaining M9 workflows require their own evidence.']}
  out=Path(a.output);out.parent.mkdir(parents=True,exist_ok=True);out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
finally:
 if holder and holder.poll() is None:holder.kill();holder.wait(timeout=10)
 for node in processes:
  if node.poll() is None:node.terminate();node.wait(timeout=15)
 for logfile in logfiles:logfile.close()
 sql('DROP SCHEMA '+schema+' CASCADE')
