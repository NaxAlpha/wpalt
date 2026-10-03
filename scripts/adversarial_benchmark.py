#!/usr/bin/env python3
"""Measure promotion selection with one versus 100 large candidates on disposable SQLite sites.
No timing thresholds: query count, payload and observed RSS/latency are recorded together.
"""
import argparse, hashlib, http.cookiejar, json, os, platform, secrets, socket, sqlite3
import subprocess, tempfile, time, urllib.request, uuid
from pathlib import Path
p = argparse.ArgumentParser(); p.add_argument('--binary', default='target/release/wpalt'); p.add_argument('--output',default='work/adversarial-promotion-performance.json'); args=p.parse_args()
binary=Path(args.binary).resolve(); sha=hashlib.sha256(binary.read_bytes()).hexdigest()
result={'binary_sha256':sha,'platform':platform.platform(),'conditions':'Native release, SQLite, debug SQL enabled, isolated temporary sites; one vs 100 promotions, both 500,000-character variants; only final candidate matches; six selected requests, sequential; Python HTTP overhead included. RSS is sampled after requests, not peak memory.','profiles':[]}
document=json.dumps({'version':1,'root':{'type':'doc','content':[{'type':'paragraph','content':[{'type':'text','text':'Readable. '*50000}]}]}})
def rss(pid): return int(subprocess.check_output(['ps','-o','rss=','-p',str(pid)],text=True).strip())*1024
with tempfile.TemporaryDirectory(prefix='wpalt-adversarial-perf-') as tmp:
 for candidates in [1,100]:
  root=Path(tmp)/str(candidates);root.mkdir()
  with socket.socket() as sock: sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
  origin=f'http://127.0.0.1:{port}';cfg=root/'site.toml';db=root/'site.db'
  cfg.write_text(f'database_url="sqlite://{db}?mode=rwc"\ndata_dir="{root}/data"\nlisten="127.0.0.1:{port}"\nbase_url="{origin}"\ndebug=true\nscheduler_seconds=60\n')
  password=secrets.token_urlsafe(24)
  for command,stdin in [(['init','--admin-email','fixture@example.test'],password+'\n'),(['seed-demo','--posts','1'],None)]:
   run=subprocess.run([str(binary),'--config',str(cfg),*command],input=stdin,text=True,capture_output=True);assert run.returncode==0,run.stderr
  # Offline fixture creation only; no independent writes while the owning process runs.
  with sqlite3.connect(db) as conn:
   conn.execute('UPDATE engagement_settings SET enabled=1 WHERE id=1')
   selected=None
   for n in range(candidates):
    selected=str(uuid.uuid4())
    target=json.dumps({'paths':['/'] if n==candidates-1 else ['/not-this-fixture'], 'device':'all','referrer':'all','starts_at':0,'ends_at':0,'max_impressions':10})
    conn.execute('INSERT INTO business_promotions(id,title,document_a,document_b,target,active,created_at) VALUES(?,?,?,?,?,1,?)',(selected,'Measured offer',document,document,target,n))
  log=root/'server.log'
  with log.open('w') as sink:
   server=subprocess.Popen([str(binary),'--config',str(cfg),'serve'],stdout=sink,stderr=sink)
   try:
    for _ in range(100):
     try: urllib.request.urlopen(origin+'/health',timeout=1).close();break
     except Exception: assert server.poll() is None;time.sleep(.1)
    else: raise AssertionError('Server readiness timeout')
    before=rss(server.pid);jar=http.cookiejar.CookieJar();client=urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
    def send(path,data):return client.open(urllib.request.Request(origin+path,data=json.dumps(data).encode(),headers={'Origin':origin,'Content-Type':'application/json'}),timeout=20)
    with send('/api/engagement/consent',{'allow':True,'recording':False,'policy':1}) as response: response.read()
    timings=[];request_ids=[];sizes=[]
    for _ in range(6):
     start=time.perf_counter()
     with send('/api/engagement/offers',{'path':'/','device':'desktop','referrer':'direct'}) as response:
      raw=response.read();request_ids.append(response.headers['x-request-id']);sizes.append(len(raw));offer=json.loads(raw)["offer"]
     assert offer['id']==selected and len(offer['html'])>=500000
     timings.append((time.perf_counter()-start)*1000)
    after=rss(server.pid)
   finally:server.terminate();server.wait(timeout=10)
  records=[json.loads(line) for line in log.read_text().splitlines() if line.startswith('{')]
  counts=[sum(record.get('target')=='sqlx::query' and any(s.get('request_id')==rid for s in record.get('spans',[])) for record in records) for rid in request_ids]
  assert all(count>0 for count in counts),'SQL telemetry must actually be correlated.'
  result['profiles'].append({'candidates':candidates,'variant_json_bytes':len(document.encode()),'requests':6,'response_bytes':sizes,'latencies_ms':timings,'sql_queries_per_request':counts,'rss_before_bytes':before,'rss_after_bytes':after})
assert set(result['profiles'][0]['sql_queries_per_request'])==set(result['profiles'][1]['sql_queries_per_request']), 'Selecting past 99 unmatched candidates must not add per-candidate queries.'
assert hashlib.sha256(binary.read_bytes()).hexdigest()==sha,'Measured executable changed.'
out=Path(args.output);out.parent.mkdir(parents=True,exist_ok=True);out.write_text(json.dumps(result,indent=2)+'\n')
print('Recorded one/100-candidate large-promotion latency, RSS and constant correlated query count.')
