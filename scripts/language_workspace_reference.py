#!/usr/bin/env python3
"""Populated language pagination, production query plans and scoped localhost timings."""
import argparse, hashlib, http.cookiejar, html, json, os, re, secrets, socket, sqlite3, statistics, subprocess, tempfile, time, urllib.parse, urllib.request, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',default='target/debug/wpalt');p.add_argument('--build-profile',choices=['debug','release','unknown'],default='unknown');p.add_argument('--postgres',default=os.environ.get('TEST_DATABASE_URL'));p.add_argument('--psql',default='/opt/homebrew/opt/postgresql@17/bin/psql');p.add_argument('--output',default='work/d03-language-reference.json');args=p.parse_args();binary=Path(args.binary).resolve()
source=(ROOT/'src/platform/language_web.rs').read_text()
base=re.search(r'"(SELECT id,title,locale,translation_group,status,updated_at FROM posts WHERE 1=1)"',source).group(1)
end=re.search(r'q.push\("( ORDER BY updated_at DESC,id DESC LIMIT 41)"\)',source).group(1)
report=[]
for engine in (['sqlite','postgres'] if args.postgres else ['sqlite']):
 with tempfile.TemporaryDirectory(prefix='wpalt-language-plans-') as tmp:
  root=Path(tmp);schema='wpalt_language_'+uuid.uuid4().hex
  def pg(sql):return subprocess.check_output([args.psql,args.postgres,'-X','-q','-t','-A','-v','ON_ERROR_STOP=1','-c',sql],text=True).strip()
  with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
  origin=f'http://127.0.0.1:{port}';database=root/'site.db'
  url='sqlite://'+str(database)+'?mode=rwc'
  if engine=='postgres':
   pg('CREATE SCHEMA '+schema);parsed=urllib.parse.urlsplit(args.postgres);query=urllib.parse.parse_qsl(parsed.query);query.append(('options','-c search_path='+schema));url=urllib.parse.urlunsplit(parsed._replace(query=urllib.parse.urlencode(query)))
  config=root/'site.toml';config.write_text(f'database_url={json.dumps(url)}\ndata_dir={json.dumps(str(root/"data"))}\nlisten="127.0.0.1:{port}"\nbase_url="{origin}"\ndebug=true\n')
  password=secrets.token_urlsafe(24);server=None;connection=None
  def native(*argv,stdin=None):
   r=subprocess.run([str(binary),'--config',str(config),*map(str,argv)],input=stdin,text=True,capture_output=True);assert r.returncode==0,r.stderr;return r.stdout
  try:
   native('init','--admin-email','owner@example.test',stdin=password+'\n');native('seed-demo','--posts','1500')
   if engine=='sqlite':connection=sqlite3.connect(database)
   def sql(command):
    if connection:
     result=connection.execute(command).fetchall();connection.commit();return result
    return pg('SET search_path TO '+schema+';'+command)
   definition={'default_language':'en','languages':[{'code':'en','label':'English','direction':'ltr','navigation':[],'search_label':'Search'},{'code':'fr','label':'Français','direction':'ltr','navigation':[],'search_label':'Rechercher'}],'business':{'name':'','street':'','city':'','postal_code':'','country':'','telephone':''}}
   sql("UPDATE discovery_settings SET definition='"+json.dumps(definition).replace("'","''")+"',version=version+1 WHERE id=1")
   sql("UPDATE posts SET locale='fr' WHERE id IN (SELECT id FROM posts ORDER BY id LIMIT 750)")
   sql('ANALYZE')
   plans=[]
   for label,condition in [('all',''),('filtered'," AND locale='fr'"),('filtered cursor'," AND locale='fr' AND (updated_at,id)<(9223372036854775807,'ffffffff-ffff-ffff-ffff-ffffffffffff')")]:
    query=base+condition+end
    if connection:
     rows=sql(query);assert len(rows)==41
     plan=[r[3] for r in sql('EXPLAIN QUERY PLAN '+query)]
     assert any('language_workspace' in step for step in plan),plan
    else:
     plan=json.loads(sql('EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) '+query))
     assert plan[0]['Plan']['Actual Rows']==41
     assert 'language_workspace' in json.dumps(plan),plan
    plans.append({'label':label,'query':query,'plan':plan})
   logfile=(root/'server.log').open('w');server=subprocess.Popen([str(binary),'--config',str(config),'serve'],stdout=logfile,stderr=logfile)
   jar=http.cookiejar.CookieJar();client=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPCookieProcessor(jar))
   def get(path):
    with client.open(origin+path,timeout=15) as response:return response.read().decode()
   for _ in range(100):
    try:get('/health');break
    except Exception:time.sleep(.1)
   login=urllib.request.Request(origin+'/login',data=urllib.parse.urlencode({'email':'owner@example.test','password':password}).encode(),headers={'Origin':origin,'Content-Type':'application/x-www-form-urlencoded'})
   with client.open(login,timeout=15):pass
   seen=[];path='/admin/languages?locale=fr';pages=0;latencies=[]
   while path:
    start=time.perf_counter();page=get(path);latencies.append((time.perf_counter()-start)*1000)
    ids=re.findall(r'<h2[^>]*><a href="/admin/languages/([0-9a-f-]{36})"',page);assert 0<len(ids)<=40;seen+=ids;pages+=1
    older=re.search(r'href="(/admin/languages\?locale=fr&amp;after=[^"]+)"',page);path=html.unescape(older.group(1)) if older else None
   assert len(seen)==750 and len(set(seen))==750,(len(seen),pages)
   for _ in range(25):
    start=time.perf_counter();get('/admin/languages?locale=fr');latencies.append((time.perf_counter()-start)*1000)
   samples=sorted(latencies);rss=None
   try:rss=int(subprocess.check_output(['ps','-o','rss=','-p',str(server.pid)],text=True).strip())*1024
   except Exception:pass
   disk=sum(f.stat().st_size for f in root.rglob('*') if f.is_file())
   report.append({'engine':engine,'fixture_posts':1500,'matching_posts':750,'pages':pages,'page_limit':40,'plans':plans,'plan_engine':('Python SQLite '+sqlite3.sqlite_version if connection else pg('SHOW server_version')),'http_samples':len(samples),'http_p50_ms':statistics.median(samples),'http_p95_ms':samples[min(len(samples)-1,int(len(samples)*.95))],'process_rss_bytes':rss,'fixture_local_file_bytes':disk,'build_profile':args.build_profile,'boundary':'Debug request logging enabled; declared build profile '+args.build_profile+'; one sequential localhost client, warmed fixture, authentication included per request; no production throughput claim. PostgreSQL server files/RSS and optional model resources excluded.'})
  finally:
   if server:server.terminate();server.wait(timeout=15);logfile.close()
   if connection:connection.close()
   if engine=='postgres':pg('DROP SCHEMA '+schema+' CASCADE')
Path(args.output).write_text(json.dumps({'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'production_source_sha256':hashlib.sha256(source.encode()).hexdigest(),'results':report},indent=2));print(json.dumps([{'engine':r['engine'],'pages':r['pages'],'http_p95_ms':r['http_p95_ms']} for r in report]))
