#!/usr/bin/env python3
"""Release admin-read latency, native RSS and correlated SQL counts on a disposable site.
This is a measurement tool, not a timing assertion or substitute for browser acceptance.
"""
import http.cookiejar,json,secrets,socket,subprocess,tempfile,time,urllib.request,urllib.parse
from pathlib import Path
binary=Path('target/release/wpalt').resolve()
with tempfile.TemporaryDirectory(prefix='wpalt-diagnostics-') as tmp:
 root=Path(tmp)
 with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
 origin=f'http://127.0.0.1:{port}';cfg=root/'site.toml';password=secrets.token_urlsafe(24)
 cfg.write_text(f'database_url="sqlite://{root}/site.db?mode=rwc"\ndata_dir="{root}/data"\nlisten="127.0.0.1:{port}"\nbase_url="{origin}"\ndebug=true\nscheduler_seconds=60\n')
 for args,stdin in [(['init','--admin-email','review@example.test'],password+'\n'),(['seed-demo','--posts','1000'],None)]:
  done=subprocess.run([str(binary),'--config',str(cfg),*args],input=stdin,text=True,capture_output=True);assert done.returncode==0,done.stderr
 log=root/'server.log'
 with log.open('w') as file:
  server=subprocess.Popen([str(binary),'--config',str(cfg),'serve'],stderr=file,stdout=file)
  try:
   for _ in range(100):
    try:urllib.request.urlopen(origin+'/health',timeout=1).close();break
    except Exception:assert server.poll() is None;time.sleep(.1)
   rss=lambda:int(subprocess.check_output(['ps','-o','rss=','-p',str(server.pid)],text=True).strip())*1024
   result={'conditions':'macOS native release; 1000 stories + About; debug SQL enabled; 50 warm admin reads; latency includes Python localhost client, excludes browser render; no timing assertions.','idle_initialized_rss_bytes':rss(),'routes':[]}
   opener=urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
   opener.open(urllib.request.Request(origin+'/login',data=urllib.parse.urlencode({'email':'review@example.test','password':password}).encode(),headers={'Origin':origin})).read()
   items=json.load(opener.open(origin+'/api/content'))['items'];id=items[0]['id']
   for route in ['/','/journal-1','/search?q=publishing','/admin/posts',f'/admin/posts/{id}','/admin/builder','/api/admin/design','/admin/forms','/admin/audience','/admin/campaigns','/admin/mail','/admin/engagement','/admin/promotions','/admin/registrations']:
    times=[];request_ids=[]
    for _ in range(50):
     start=time.perf_counter()
     with opener.open(origin+route) as response:response.read();request_ids.append(response.headers['x-request-id'])
     times.append((time.perf_counter()-start)*1000)
    time.sleep(.05)
    records=[json.loads(line) for line in log.read_text().splitlines() if line.startswith('{')]
    counts=[]
    for request_id in request_ids:
     counts.append(sum(1 for record in records if record.get('target')=='sqlx::query' and any(span.get('request_id')==request_id for span in record.get('spans',[]))))
    result['routes'].append({'route':route if not route.startswith('/admin/posts/') else '/admin/posts/{id}','p50_ms':round(sorted(times)[24],3),'p95_ms':round(sorted(times)[47],3),'sql_queries_per_request':sorted(set(counts))})
   result['rss_after_authoring_reads_bytes']=rss()
   Path('work/diagnostics.json').write_text(json.dumps(result,indent=2)+'\n')
   print('Recorded idle/active RSS, admin-read timings and correlated SQL counts.')
  finally:server.terminate();server.wait(timeout=10)
