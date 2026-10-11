#!/usr/bin/env python3
"""Declared serial import/public request observations, not throughput or CWV."""
from html.parser import HTMLParser
import argparse, hashlib, http.cookiejar, json, os, re, secrets, shutil, socket, sqlite3, statistics, subprocess, tempfile, time, urllib.error, urllib.parse, urllib.request, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--profile',choices=['debug','release'],required=True);p.add_argument('--postgres',default=os.environ.get('TEST_DATABASE_URL'));p.add_argument('--psql',default=shutil.which('psql'));p.add_argument('--output',default='work/d07-structured-observations.json');args=p.parse_args();binary=Path(args.binary).resolve()
assert not args.postgres or args.psql
results=[]
for engine in (['sqlite','postgres'] if args.postgres else ['sqlite']):
 with tempfile.TemporaryDirectory(prefix='wpalt-structured-measure-') as temporary:
  root=Path(temporary);schema='wpalt_structured_'+uuid.uuid4().hex;server=None;log=None;connection=None
  def pg(statement):return subprocess.check_output([args.psql,args.postgres,'-X','-qAt','-v','ON_ERROR_STOP=1','-c',statement],text=True).strip()
  with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
  origin=f'http://127.0.0.1:{port}';url='sqlite://'+str(root/'site.db')+'?mode=rwc';password=secrets.token_urlsafe(24)
  if engine=='postgres':
   pg('CREATE SCHEMA '+schema);u=urllib.parse.urlsplit(args.postgres);query=urllib.parse.parse_qsl(u.query)+[('options','-c search_path='+schema)];url=urllib.parse.urlunsplit(u._replace(query=urllib.parse.urlencode(query)))
  config=root/'site.toml';config.write_text(f'database_url={json.dumps(url)}\ndata_dir={json.dumps(str(root/"data"))}\nlisten="127.0.0.1:{port}"\nbase_url="{origin}"\ndebug=true\n')
  def native(*argv,stdin=None):
   r=subprocess.run([str(binary),'--config',str(config),*map(str,argv)],input=stdin,text=True,capture_output=True,timeout=120);assert r.returncode==0,r.stderr.replace(password,'[redacted]');return r.stdout
  try:
   native('init','--admin-email','owner@example.test',stdin=password+'\n');native('seed-demo','--posts','1500')
   log=(root/'server.log').open('w');server=subprocess.Popen([str(binary),'--config',str(config),'serve','--external-worker'],stdout=log,stderr=log)
   client=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
   def get(path):
    with client.open(origin+path,timeout=15) as response:return response.read()
   for _ in range(100):
    try:get('/login');break
    except OSError:time.sleep(.1)
   else:raise RuntimeError('Fixture application not ready')
   data=urllib.parse.urlencode({'email':'owner@example.test','password':password}).encode();client.open(urllib.request.Request(origin+'/login',data=data,headers={'Origin':origin}),timeout=10).read()
   html=get('/admin/builder').decode();csrf=re.search(r'data-csrf="([^"]+)"',html).group(1)
   def post(path,value):
    data=json.dumps(value,separators=(',',':')).encode();start=time.perf_counter()
    with client.open(urllib.request.Request(origin+path,data=data,headers={'Origin':origin,'Content-Type':'application/json','X-CSRF-Token':csrf}),timeout=15) as response:body=response.read()
    return (time.perf_counter()-start)*1000,body
   state=json.loads(get('/api/admin/design'));models=state['registry']['models'];original=models['post'];version=state['model_versions']['post']
   definition=json.loads(json.dumps(original));definition['fields']['public_author']={'kind':'relationship','target':'public_person'}
   person={'label':'Measured public people','fields':{'name':{'kind':'string'}},'structured':{'vocabulary':'30.1','primary':'person','entities':{'person':{'types':['Person'],'properties':{'name':{'kind':'field','path':['name'],'datatype':'Text'},'url':{'kind':'publication','attribute':'url'}}}}}}
   post('/api/admin/models/public_person',{'csrf':csrf,'version':0,'definition':person})
   _,updated=post('/api/admin/models/post',{'csrf':csrf,'version':version,'definition':definition});version=json.loads(updated)['version']
   _,author=post('/api/admin/content',{'title':'Measured public author','slug':'measured-author','kind':'public_person','fields':json.dumps({'name':'Public pen name'}),'body':'Actual public author profile.','action':'publish'});author=json.loads(author)
   for i in range(30):post('/api/admin/content',{'title':'Measured Article '+str(i),'slug':'structured-measured-'+str(i),'kind':'post','fields':json.dumps({'public_author':author['id']}),'body':'Actual measured editorial publication.','action':'publish'})
   public=urllib.request.build_opener(urllib.request.ProxyHandler({}));observations={}
   def observe(name):
    samples=[]
    for i in range(35):
     start=time.perf_counter()
     with public.open(origin+'/structured-measured-0',timeout=15) as response:body=response.read()
     if i>=5:samples.append((time.perf_counter()-start)*1000)
    offset=(root/'server.log').stat().st_size
    with public.open(origin+'/structured-measured-0',timeout=15) as response:response.read()
    time.sleep(.03)
    with (root/'server.log').open() as diagnostics:diagnostics.seek(offset);logged=diagnostics.read(1024*1024)
    shapes=[]
    for line in logged.splitlines():
     try:event=json.loads(line)
     except ValueError:continue
     fields=event.get('fields',{});statement=fields.get('db.statement') or fields.get('summary','')
     if event.get('target','').startswith('sqlx') and statement.lstrip().upper().startswith('SELECT'):shapes.append(fields.get('summary',statement.splitlines()[0])[:200])
    assert shapes,'SQLx SELECT diagnostics must be observed'
    rss=int(subprocess.check_output(['ps','-o','rss=','-p',str(server.pid)],text=True).strip())*1024
    observations[name]={'samples':30,'p50_ms':round(statistics.median(samples),3),'p95_ms':round(sorted(samples)[27],3),'response_bytes':len(body),'observed_selects':len(shapes),'select_shapes':shapes,'warm_application_rss_bytes':rss}
    return body
   baseline=observe('same_fields_disabled')
   definition['structured']={'vocabulary':'30.1','primary':'article','entities':{'article':{'types':['Article'],'properties':{'headline':{'kind':'publication','attribute':'title'},'datePublished':{'kind':'publication','attribute':'published_at'},'author':{'kind':'relationship','path':['public_author'],'entity':'person'}}}}}
   preflights=[]
   for _ in range(6):
    elapsed,updated=post('/api/admin/models/post',{'csrf':csrf,'version':version,'definition':definition});version=json.loads(updated)['version'];preflights.append(elapsed)
   enabled=observe('same_fields_enabled')
   assert b'Published facts' in enabled and b'Public pen name' in enabled
   assert observations['same_fields_enabled']['select_shapes']==observations['same_fields_disabled']['select_shapes'],'Pure graph/facts projection must add zero database queries'
   # Cached lookup is bounded and never runs in a request lacking declarations.
   search=[]
   for i in range(35):
    started=time.perf_counter();terms=json.loads(get('/api/admin/structured/vocabulary?family=property&prefix=address'))
    if i>=5:search.append((time.perf_counter()-started)*1000)
   assert len(terms['items'])<=40
   if engine=='sqlite':database_bytes=sum(p.stat().st_size for p in root.glob('site.db*'))
   else:database_bytes=int(pg("SELECT SUM(pg_total_relation_size(quote_ident(schemaname)||'.'||quote_ident(tablename))) FROM pg_tables WHERE schemaname='"+schema+"'"))
   results.append({'engine':engine,'population':1531,'publications_with_author':30,'request_observations':observations,'extra_projection_selects':0,'schema_preflight_ms':{'samples':5,'p50':round(statistics.median(preflights[1:]),3),'max':round(max(preflights[1:]),3)},'cached_search_ms':{'samples':30,'p50':round(statistics.median(search),3),'p95':round(sorted(search)[27],3)},'database_bytes':database_bytes,'database_scope':'SQLite DB/WAL/SHM' if engine=='sqlite' else 'All fixture PostgreSQL relations/indexes; engine memory excluded'})
  finally:
   if connection:connection.close()
   if server:server.terminate();server.wait(timeout=10)
   if log:log.close()
   if engine=='postgres':pg('DROP SCHEMA '+schema+' CASCADE')
report={'profile':args.profile,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'scope':'1500 seeded posts plus public author and 30 actual related API publications. Same populated fields disabled/enabled, 5 warmups and 30 serial loopback requests; five repeated 1531-record configuration preflight observations. Debug SQL diagnostics enabled; warm RSS is neither peak nor isolated vocabulary-memory attribution. No throughput, TLS, CWV or WordPress efficiency claim. SQLite/PostgreSQL database sizes have differing scopes.','observations':results}
output=Path(args.output);output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
