#!/usr/bin/env python3
"""Declared serial import/public request observations, not throughput or CWV."""
import argparse, hashlib, http.cookiejar, json, os, re, secrets, shutil, socket, sqlite3, statistics, subprocess, tempfile, time, urllib.error, urllib.parse, urllib.request, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--profile',choices=['debug','release'],required=True);p.add_argument('--postgres',default=os.environ.get('TEST_DATABASE_URL'));p.add_argument('--psql',default=shutil.which('psql'));p.add_argument('--output',default='work/d05-design-observations.json');args=p.parse_args();binary=Path(args.binary).resolve()
assert not args.postgres or args.psql
results=[]
for engine in (['sqlite','postgres'] if args.postgres else ['sqlite']):
 with tempfile.TemporaryDirectory(prefix='wpalt-design-measure-') as temporary:
  root=Path(temporary);schema='wpalt_design_'+uuid.uuid4().hex;server=None;log=None;connection=None
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
   log=(root/'server.log').open('w');server=subprocess.Popen([str(binary),'--config',str(config),'serve'],stdout=log,stderr=log)
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
    with client.open(urllib.request.Request(origin+path,data=data,headers={'Origin':origin,'Content-Type':'application/json'}),timeout=15) as response:body=response.read()
    return (time.perf_counter()-start)*1000,body
   state=json.loads(get('/api/admin/design'));theme=next(t for t in state['themes'] if t['id']==state['active']);groups=[]
   for g in range(10):
    items=[]
    for i in range(10):
     marker=f'{g}_{i}'
     for widget,settings in [('heading',{'title':'Measured heading '+marker,'header_size':'h2'}),('text-editor',{'editor':'<p>A representative locally authored paragraph '+marker+'.</p>'}),('button',{'text':'Explore '+marker,'link':{'url':'/search'}})]:items.append({'id':widget.replace('-','')+marker,'elType':'widget','widgetType':widget,'settings':settings,'elements':[]})
    groups.append({'id':'group'+str(g),'elType':'container','settings':{'flex_direction':'column','gap':{'unit':'px','size':16}},'elements':items})
   request={'source':{'title':'Measured design','type':'page','version':'0.4','page_settings':[],'content':groups},'component':'measured-import','target':'home'}
   envelope={'csrf':csrf,'version':theme['version'],'request':request};path='/api/admin/design/'+theme['id']+'/elementor/'
   for _ in range(5):post(path+'review',envelope)
   latencies=[]
   for _ in range(30):elapsed,body=post(path+'review',envelope);latencies.append(elapsed)
   review=json.loads(body);package_bytes=len(json.dumps(review['package'],separators=(',',':')).encode())
   envelope.update({'fingerprint':review['fingerprint'],'acknowledge_losses':True});_,applied=post(path+'apply',envelope);version=json.loads(applied)['version']
   post('/api/admin/design/'+theme['id'],{'csrf':csrf,'version':version,'package':review['package'],'publish':True})
   # Public clients carry no owner cookie; latency remains a serial loopback observation.
   public=urllib.request.build_opener(urllib.request.ProxyHandler({}));public_latencies=[]
   for i in range(35):
    start=time.perf_counter()
    with public.open(origin+'/',timeout=15) as response:public_body=response.read()
    elapsed=(time.perf_counter()-start)*1000
    if i>=5:public_latencies.append(elapsed)
   assert b'Measured heading 9_9' in public_body
   rss=int(subprocess.check_output(['ps','-o','rss=','-p',str(server.pid)],text=True).strip())*1024
   query='SELECT draft AS package,version,published_version FROM themes WHERE id=$1'
   if engine=='sqlite':
    connection=sqlite3.connect(root/'site.db');plan=[row[3] for row in connection.execute('EXPLAIN QUERY PLAN '+query,{'1':theme['id']})];database_bytes=sum(path.stat().st_size for path in root.glob('site.db*'))
   else:
    plan=json.loads(pg('SET search_path TO '+schema+';EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) '+query.replace('$1',"'"+theme['id']+"'")));database_bytes=int(pg('SELECT pg_total_relation_size(quote_ident('+"'"+schema+"'"+')||'+"'.themes'"+')'))
   def summary(values):return {'samples':len(values),'p50_ms':round(statistics.median(values),3),'p95_ms':round(sorted(values)[int(len(values)*.95)-1],3)}
   results.append({'engine':engine,'published_posts':1500,'source_elements':review['report']['elements'],'request_bytes':len(json.dumps(envelope,separators=(',',':')).encode()),'package_bytes':package_bytes,'review_response_bytes':len(body),'public_response_bytes':len(public_body),'warm_process_rss_bytes':rss,'database_observation_bytes':database_bytes,'database_scope':'SQLite DB/WAL/SHM files' if engine=='sqlite' else 'PostgreSQL themes table including indexes; engine memory and other relations excluded','review':summary(latencies),'public':summary(public_latencies),'theme_load_query':query,'theme_load_plan':plan})
  finally:
   if connection:connection.close()
   if server:server.terminate();server.wait(timeout=10)
   if log:log.close()
   if engine=='postgres':pg('DROP SCHEMA '+schema+' CASCADE')
output=Path(args.output);output.parent.mkdir(parents=True,exist_ok=True);report={'profile':args.profile,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'compiler_source_sha256':hashlib.sha256((ROOT/'src/platform/elementor_design.rs').read_bytes()).hexdigest(),'scope':'1500 published posts plus 310-element design; 5 warmup and 30 serial loopback samples per path, full HTTP review then explicit private apply/publication; warm application RSS is not peak or PostgreSQL engine memory; database scopes differ. No TLS/throughput/CWV/WordPress efficiency comparison.','observations':results};output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
