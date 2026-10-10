#!/usr/bin/env python3
"""Declared single-client theme/font resource observations; not production throughput."""
import argparse,hashlib,json,os,re,secrets,shutil,socket,sqlite3,statistics,subprocess,tempfile,time,urllib.error,urllib.parse,urllib.request,uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--build-profile',choices=['debug','release'],required=True);p.add_argument('--postgres',default=os.environ.get('TEST_DATABASE_URL'));p.add_argument('--psql',default=shutil.which('psql'));p.add_argument('--output',default='work/d04-theme-reference.json');args=p.parse_args();binary=Path(args.binary).resolve()
assert not args.postgres or args.psql, 'PostgreSQL observations require psql'
source=(ROOT/'src/theme/asset_web.rs').read_text();query=re.search(r'"(SELECT a.size FROM theme_assets a WHERE a.id=\$1[^"]+)"',source).group(1)
results=[]
for engine in (['sqlite','postgres'] if args.postgres else ['sqlite']):
 with tempfile.TemporaryDirectory(prefix='wpalt-theme-reference-') as temporary:
  root=Path(temporary);schema='wpalt_theme_'+uuid.uuid4().hex;connection=None;server=None;log=None
  def pg(statement):return subprocess.check_output([args.psql,args.postgres,'-X','-qAt','-v','ON_ERROR_STOP=1','-c',statement],text=True).strip()
  with socket.socket() as listener:listener.bind(('127.0.0.1',0));port=listener.getsockname()[1]
  origin=f'http://127.0.0.1:{port}';url='sqlite://'+str(root/'site.db')+'?mode=rwc'
  if engine=='postgres':
   pg('CREATE SCHEMA '+schema);u=urllib.parse.urlsplit(args.postgres);q=urllib.parse.parse_qsl(u.query)+[('options','-c search_path='+schema)];url=urllib.parse.urlunsplit(u._replace(query=urllib.parse.urlencode(q)))
  config=root/'site.toml';config.write_text(f'database_url={json.dumps(url)}\ndata_dir={json.dumps(str(root/"data"))}\nlisten="127.0.0.1:{port}"\nbase_url="{origin}"\ndebug=true\n')
  def native(*argv,stdin=None):
   result=subprocess.run([str(binary),'--config',str(config),*map(str,argv)],input=stdin,text=True,capture_output=True);assert result.returncode==0,result.stderr;return result.stdout
  try:
   native('init','--admin-email','owner@example.test',stdin=secrets.token_urlsafe(24)+'\n');native('seed-demo','--posts','1500')
   fixture=ROOT/'tests/fixtures/fonts';font_id=native('theme','font-import',fixture/'Aboreto-Regular.ttf','--label','Reference font','--source','Unmodified official fixture','--license',fixture/'OFL.txt','--rights').splitlines()[0];assert re.fullmatch('[0-9a-f]{64}',font_id)
   package_file=root/'theme.json';native('theme','export','paper',package_file,'--draft');package=json.loads(package_file.read_text())
   package['fonts']={'local':{'asset':font_id,'weight':400,'style':'normal','display':'swap','fallback':'serif'}};package['tokens']['font']='local:local'
   package['navigations']={'main':{'language':'en','direction':'ltr','label':'Reference navigation','layout':'columns','languages':{},'items':[{'label':f'Group {i}','url':'','description':'Local group','children':[{'label':f'Link {j}','url':'/','description':'','children':[]} for j in range(4)]} for i in range(3)]}}
   def assign(node):
    if node['kind']=='navigation':node['source']='main'
    for child in node.get('children',[]):assign(child)
   assign(package['header']);package_file.write_text(json.dumps(package));
   for _ in range(12):native('theme','import','reference',package_file,'--publish')
   native('theme','activate','reference')
   if engine=='sqlite':
    connection=sqlite3.connect(root/'site.db');connection.execute('ANALYZE');connection.commit()
    plan=[row[3] for row in connection.execute('EXPLAIN QUERY PLAN '+query,{'1':font_id})]
    storage=connection.execute('SELECT COUNT(*),SUM(size) FROM theme_assets').fetchone();revisions=connection.execute("SELECT COUNT(*) FROM theme_asset_references WHERE theme_id='reference'").fetchone()[0]
   else:
    plan=json.loads(pg('SET search_path TO '+schema+';EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) '+query.replace('$1',"'"+font_id+"'")))
    storage=tuple(map(int,pg('SET search_path TO '+schema+';SELECT COUNT(*),SUM(size) FROM theme_assets').split('|')));revisions=int(pg("SET search_path TO "+schema+";SELECT COUNT(*) FROM theme_asset_references WHERE theme_id='reference'"))
   assert storage==(1,48356) and revisions==12
   archive=root/'snapshot.json';native('backup',archive);graph=json.loads(json.loads(archive.read_text())['payload']);assert len(graph['tables']['theme_assets'])==1
   assert len(graph['tables']['theme_asset_references'])==12
   log=(root/'server.log').open('w');server=subprocess.Popen([str(binary),'--config',str(config),'serve'],stdout=log,stderr=log)
   client=urllib.request.build_opener(urllib.request.ProxyHandler({}))
   def get(path,headers=None):
    request=urllib.request.Request(origin+path,headers=headers or {})
    try:
     with client.open(request,timeout=15) as response:return response.status,response.read()
    except urllib.error.HTTPError as error:
     if error.code==304:return 304,error.read()
     raise
   for _ in range(100):
    try:get('/health');break
    except OSError:time.sleep(.1)
   paths=[('/',{},200),('/theme-assets/'+font_id,{},200),('/theme-assets/'+font_id,{'If-None-Match':'"'+font_id+'"'},304)]
   observations=[]
   for path,headers,expected in paths:
    get(path,headers);samples=[];sizes=[]
    for _ in range(30):
     start=time.perf_counter();status,data=get(path,headers);samples.append((time.perf_counter()-start)*1000);assert status==expected;sizes.append(len(data))
    ordered=sorted(samples);observations.append({'path':path,'status':expected,'samples':30,'p50_ms':statistics.median(samples),'p95_ms':ordered[28],'response_bytes':max(sizes)})
   rss=int(subprocess.check_output(['ps','-o','rss=','-p',str(server.pid)],text=True).strip())*1024
   results.append({'engine':engine,'posts':1500,'font_assets':storage[0],'shared_font_bytes':storage[1],'font_revision_references':revisions,'archive_bytes':archive.stat().st_size,'local_fixture_bytes':sum(f.stat().st_size for f in root.rglob('*') if f.is_file()),'app_rss_bytes_after_warmup':rss,'public_font_plan':plan,'plan_engine':('Python SQLite '+sqlite3.sqlite_version if connection else pg('SHOW server_version')),'observations':observations})
  finally:
   if server:server.terminate();server.wait(timeout=15);log.close()
   if connection:connection.close()
   if engine=='postgres':pg('DROP SCHEMA '+schema+' CASCADE')
Path(args.output).write_text(json.dumps({'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'binary_bytes':binary.stat().st_size,'font_query_source_sha256':hashlib.sha256(source.encode()).hexdigest(),'build_profile':args.build_profile,'boundary':'1500-post synthetic fixture, one 48356-byte font shared by 12 retained references; 3 groups/12 links. One sequential warmed localhost client, debug logging enabled, application RSS only (not peak); PostgreSQL server files/RSS excluded. Python SQLite plan is supplemental, not the SQLx engine claim. No TLS/production throughput/WordPress comparison or optional model resources.','results':results},indent=2));print('PASS: bounded font reference storage and declared two-engine resource observations')
