#!/usr/bin/env python3
"""Declared serial import/public request observations, not throughput or CWV."""
import xml.etree.ElementTree as ET
import argparse, hashlib, http.cookiejar, json, os, re, secrets, shutil, socket, sqlite3, statistics, subprocess, tempfile, time, urllib.error, urllib.parse, urllib.request, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--profile',choices=['debug','release'],required=True);p.add_argument('--postgres',default=os.environ.get('TEST_DATABASE_URL'));p.add_argument('--psql',default=shutil.which('psql'));p.add_argument('--output',default='work/d06-discovery-observations.json');args=p.parse_args();binary=Path(args.binary).resolve()
assert not args.postgres or args.psql
results=[]
for engine in (['sqlite','postgres'] if args.postgres else ['sqlite']):
 with tempfile.TemporaryDirectory(prefix='wpalt-discovery-measure-') as temporary:
  root=Path(temporary);schema='wpalt_discovery_'+uuid.uuid4().hex;server=None;log=None;connection=None
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
   # Admit the image through the real native upload path, then publish canonical sources.
   boundary='d06-'+uuid.uuid4().hex;upload=b''
   for key,value in [('csrf',csrf),('visibility','public'),('alt','Measured garden')]:upload+=f'--{boundary}\r\nContent-Disposition: form-data; name="{key}"\r\n\r\n{value}\r\n'.encode()
   upload+=f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="d06-measured.png"\r\nContent-Type: image/png\r\n\r\n'.encode()+(ROOT/'tests/fixtures/green.png').read_bytes()+f'\r\n--{boundary}--\r\n'.encode()
   client.open(urllib.request.Request(origin+'/admin/media',data=upload,headers={'Origin':origin,'Content-Type':'multipart/form-data; boundary='+boundary}),timeout=15).read()
   image=json.loads(get('/api/admin/media'))['items'][0]['id']
   for i in range(121):
    value={'title':'Measured news '+str(i),'slug':'measured-news-'+str(i),'kind':'post','body':f'Actual local editorial image. ![Garden](/media/{image})','action':'publish','seo':json.dumps({'schema_type':'Article','social_image':image,'social_alt':'Measured garden','news':{'publication':'Measured Journal','language':'en'}})}
    post('/api/admin/content',value)
   # Refresh fixture statistics before observing the planner and warm requests.
   if engine=='sqlite':
    connection=sqlite3.connect(root/'site.db');connection.execute('ANALYZE');connection.commit()
   else:pg('SET search_path TO '+schema+';ANALYZE')
   public=urllib.request.build_opener(urllib.request.ProxyHandler({}));observations={}
   for name,path in [('image','/discovery-sitemap/image'),('news','/discovery-sitemap/news'),('video_empty','/discovery-sitemap/video'),('index','/sitemap-index.xml'),('public_attached','/measured-news-0'),('public_without_attachment','/journal-1')]:
    samples=[]
    for i in range(35):
     started=time.perf_counter()
     with public.open(origin+path,timeout=15) as response:body=response.read();headers={k.lower():v for k,v in response.headers.items()}
     if i>=5:samples.append((time.perf_counter()-started)*1000)
    if name in ('image','news'):assert len(ET.fromstring(body))==100 and headers.get('link')
    # Observe existing SQLx diagnostics for one additional isolated request.
    offset=(root/'server.log').stat().st_size
    with public.open(origin+path,timeout=15) as response:response.read()
    time.sleep(.03)
    with (root/'server.log').open() as diagnostics:diagnostics.seek(offset);logged=diagnostics.read(1024*1024)
    selects=[]
    for line in logged.splitlines():
     try:event=json.loads(line)
     except ValueError:continue
     fields=event.get('fields',{});statement=fields.get('db.statement') or fields.get('summary','')
     if event.get('target','').startswith('sqlx') and statement.lstrip().upper().startswith('SELECT'):selects.append(fields.get('summary',statement.splitlines()[0])[:200])
    assert selects, 'Enabled SQLx SELECT diagnostics must be observed'
    observations[name]={'samples':30,'p50_ms':round(statistics.median(samples),3),'p95_ms':round(sorted(samples)[27],3),'response_bytes':len(body),'observed_http_select_queries':len(selects),'observed_select_shapes':selects}
   plans={};at=int(time.time())
   for kind in ['image','video','news']:
    indexable="NOT COALESCE((published_seo::jsonb->>'noindex')::boolean,false)" if engine=='postgres' else "COALESCE(json_extract(published_seo,'$.noindex'),0)=0"
    extra={'image':"EXISTS(SELECT 1 FROM published_image_refs r WHERE r.post_id=posts.id)",'video':"COALESCE((published_seo::jsonb->'video'->>'sitemap')::boolean,false)" if engine=='postgres' else "COALESCE(json_extract(published_seo,'$.video.sitemap'),0)=1",'news':("jsonb_typeof(published_seo::jsonb->'news')='object'" if engine=='postgres' else "json_type(published_seo,'$.news')='object'")+f' AND published_at>0 AND published_at>={at-172800} AND published_at<={at}'}[kind]
    query=f"SELECT id,published_slug,published_locale,published_seo,published_title,published_at FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND {indexable} AND {extra} AND id>'' ORDER BY id LIMIT 101"
    if engine=='sqlite':
     if connection is None:connection=sqlite3.connect(root/'site.db')
     plan=[r[3] for r in connection.execute('EXPLAIN QUERY PLAN '+query)]
    else:plan=json.loads(pg('SET search_path TO '+schema+';EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) '+query))
    plans[kind]={'sql':query,'plan':plan}
   if engine=='sqlite':indexes={r[0]:r[1] for r in connection.execute("SELECT name,sql FROM sqlite_master WHERE type='index' AND name LIKE 'discovery_%'")}
   else:indexes=pg("SELECT indexname||':'||indexdef FROM pg_indexes WHERE schemaname='"+schema+"' AND indexname LIKE 'discovery_%'")
   rss=int(subprocess.check_output(['ps','-o','rss=','-p',str(server.pid)],text=True).strip())*1024
   if engine=='sqlite':database_bytes=sum(p.stat().st_size for p in root.glob('site.db*'))
   else:database_bytes=int(pg("SELECT SUM(pg_total_relation_size(quote_ident(schemaname)||'.'||quote_ident(tablename))) FROM pg_tables WHERE schemaname='"+schema+"'"))
   results.append({'engine':engine,'fixture':'1500 seed publications plus 121 actual canonical image/news API publications; one admitted shared PNG; empty video population, real playback separately browser-verified','paths':observations,'query_plans':plans,'discovery_indexes':indexes,'warm_application_rss_bytes':rss,'database_bytes':database_bytes,'database_scope':'SQLite DB/WAL/SHM' if engine=='sqlite' else 'All fixture PostgreSQL relations including indexes; engine memory excluded'})
  finally:
   if connection:connection.close()
   if server:server.terminate();server.wait(timeout=10)
   if log:log.close()
   if engine=='postgres':pg('DROP SCHEMA '+schema+' CASCADE')
report={'profile':args.profile,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'publication_source_sha256':hashlib.sha256((ROOT/'src/discovery/publication.rs').read_bytes()).hexdigest(),'scope':'Five warmups and 30 serial loopback samples/path. Debug logging enabled. Warm application RSS is not peak, database engine memory is excluded, database scopes differ. No TLS/throughput/CWV/equivalent WordPress efficiency claim. Plans record current handler SQL at declared population; empty video plan does not establish populated video scalability.','observations':results}
output=Path(args.output);output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
