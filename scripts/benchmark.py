#!/usr/bin/env python3
"""Reproducible HTTP/document baseline. Uses a fresh database; never clears an existing site.
Optional PostgreSQL URL MUST identify an empty, isolated test database.
"""
import argparse,concurrent.futures,hashlib,json,math,os,platform,secrets,socket,sqlite3,subprocess,tempfile,time,urllib.request
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--psql',default='psql');p.add_argument('--local-processes',action='store_true');p.add_argument('--repeats',type=int,default=1);p.add_argument('--stress',action='store_true');p.add_argument('--binary',default='target/release/wpalt');p.add_argument('--posts',type=int,default=1000);p.add_argument('--requests',type=int,default=200);p.add_argument('--postgres-url');p.add_argument('--wordpress-url');p.add_argument('--composed',action='store_true');p.add_argument('--discovery',action='store_true');p.add_argument('--commerce',action='store_true');p.add_argument('--debug',action='store_true');p.add_argument('--cached',action='store_true');p.add_argument('--output',default='work/benchmark.json');args=p.parse_args()
assert 1<=args.repeats<=10 and (not args.local_processes or args.postgres_url), 'Local process benchmarks require an isolated PostgreSQL URL and bounded repeats'
binary=Path(args.binary).resolve();results={'machine':{'platform':platform.platform(),'cpu':platform.processor(),'logical_cpus':os.cpu_count()},'posts_requested':args.posts,'requests_per_scenario':args.requests,'binary_bytes':binary.stat().st_size,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'conditions':'Full uncompressed HTML document; warm application; no browser asset/render timing; localhost; Python HTTP client overhead included. Cache allowlist/TTL enabled only with --cached; private/commerce routes bypass; eligible public articles use the page cache.', 'local_process_mode':args.local_processes,'repeats':args.repeats,'stress':args.stress,'routing':'direct alternating local endpoints with identical public origin; no proxy overhead or retries; serialized coordinated admission','cache_enabled':args.cached,'debug_logging':args.debug,'commerce_fixture':('four physical/digital/monthly membership/group-booking products, one UTC slot, no invented payments' if args.commerce else None),'database_bytes_boundary':'PostgreSQL schema tables/indexes/TOAST only; excludes shared engine RSS, cluster catalogs, WAL and server logs. Site file bytes exclude synthetic configuration and logs.','profiles':[]}
def percentile(values,p):return sorted(values)[max(0,math.ceil(len(values)*p)-1)]
def load(urls,concurrency):
    def one(index):
        start=time.perf_counter()
        with urllib.request.urlopen(urls[index%len(urls)],timeout=20) as r:assert r.status==200;data=r.read()
        return (time.perf_counter()-start)*1000,len(data),r.headers.get('X-Request-Id')
    start=time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as workers:items=list(workers.map(one,range(args.requests)))
    elapsed=time.perf_counter()-start;latencies=[i[0] for i in items]
    return {'concurrency':concurrency,'requests':len(items),'p50_ms':round(percentile(latencies,.5),3),'p95_ms':round(percentile(latencies,.95),3),'p99_ms':round(percentile(latencies,.99),3),'requests_per_second':round(len(items)/elapsed,1),'document_bytes':items[0][1], 'request_ids':[i[2] for i in items]}
def rss(pid):return int(subprocess.check_output(['ps','-o','rss=','-p',str(pid)],text=True).strip())*1024
with tempfile.TemporaryDirectory(prefix='wpalt-benchmark-') as temporary:
    root=Path(temporary)
    for engine,db in [('sqlite',None),*([('postgres',args.postgres_url)] if args.postgres_url else [])]:
        with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
        origin=f'http://127.0.0.1:{port}';directory=root/engine;directory.mkdir();cfg=directory/'config.toml'
        url=db or f'sqlite://{directory}/site.db?mode=rwc'
        cfg.write_text(f'database_url = "{url}"\ndata_dir = "{directory}/data"\nlisten = "127.0.0.1:{port}"\nbase_url = "{origin}"\ndebug = {str(args.debug).lower()}\n')
        if args.cached:
            with cfg.open('a') as f:f.write('[cache]\nenabled = true\n')
        def run(*arguments,input=None):
            r=subprocess.run([str(binary),'--config',str(cfg),*arguments],input=input,text=True,capture_output=True)
            assert r.returncode==0,(arguments,r.stderr)
        run('init','--admin-email','benchmark@example.test',input=secrets.token_urlsafe(24)+'\n');run('seed-demo','--posts',str(args.posts))
        if args.commerce:run('shop','seed-demo')
        if args.composed:
            package=json.loads((Path(__file__).resolve().parents[1]/'assets/themes/paper.json').read_text())
            package['components']={'benchmark-card':{'parameters':{'title':'string','subtitle':'string','url':'string'},'root':{'id':'benchmark-card-root','kind':'section','style':{'padding':24},'children':[{'id':'benchmark-title','kind':'heading','text':{'bind':'params.title'}},{'id':'benchmark-subtitle','kind':'text','text':{'bind':'params.subtitle'}},{'id':'benchmark-link','kind':'link','text':'Read project','href':{'bind':'params.url'}}]}}}
            package['templates']['home']['children'][-1]={'id':'benchmark-collection','kind':'collection','source':'post','limit':20,'children':[{'id':'benchmark-instance','kind':'component','component':'benchmark-card','arguments':{'title':{'bind':'item.title'},'subtitle':{'bind':'item.fields.subtitle'},'url':{'bind':'item.url'}}}]}
            package_path=directory/'benchmark-theme.json';package_path.write_text(json.dumps(package));run('theme','import','paper',str(package_path),'--publish')
        if not db:
            # Explain the actual application predicates on the populated fixture.
            # These plans are evidence, not fragile assertions about planner text.
            queries={
                'sitemap_page':("SELECT id,published_slug,published_locale,published_seo FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND id>? ORDER BY id LIMIT 1001", ('',)),
                'privacy_subject_history':('SELECT id,kind,state,response,created_at,resolved_at FROM privacy_requests WHERE user_id=? ORDER BY created_at DESC,id DESC LIMIT 51 OFFSET ?', ('fixture',0)),
                'privacy_owner_queue':('SELECT p.id,p.kind,p.state,p.created_at,u.name FROM (SELECT id,user_id,kind,state,created_at FROM privacy_requests ORDER BY state DESC,created_at,id LIMIT 101 OFFSET ?) p JOIN users u ON u.id=p.user_id ORDER BY p.state DESC,p.created_at,p.id', (0,)),
                'account_authored_content':('SELECT id FROM posts WHERE author_id=? ORDER BY id LIMIT 1001', ('fixture',)),
            }
            with sqlite3.connect(directory/'site.db') as connection:
                results.setdefault('fixture_counts',{})[engine]={'posts':connection.execute('SELECT COUNT(*) FROM posts').fetchone()[0]}
                results.setdefault('query_plans',{})[engine]={name:[row[3] for row in connection.execute('EXPLAIN QUERY PLAN '+sql,bindings)] for name,(sql,bindings) in queries.items()}
        if db and args.local_processes:
            with cfg.open('a') as f:f.write('\n')
            # Top-level setting must precede any TOML section.
            cfg.write_text('local_processes=true\n'+cfg.read_text())
        if db:
            def pgsql(statement):
                response=subprocess.run([args.psql,db,'-X','-qAt','-v','ON_ERROR_STOP=1','-c',statement],capture_output=True,text=True,timeout=30)
                assert response.returncode==0, 'Isolated PostgreSQL resource query failed'
                return response.stdout.strip()
            results.setdefault('fixture_counts',{})[engine]={'posts':int(pgsql('SELECT COUNT(*) FROM posts'))}
            results.setdefault('query_plans',{})[engine]={name:json.loads(pgsql('EXPLAIN (FORMAT JSON) '+statement)) for name,statement in {
                'sitemap_page':"SELECT id,published_slug,published_locale,published_seo FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND id>'' ORDER BY id LIMIT 1001",
                'account_authored_content':"SELECT id FROM posts WHERE author_id='fixture' ORDER BY id LIMIT 1001"
            }.items()}
            results.setdefault('database_bytes',{})[engine]=int(pgsql("SELECT COALESCE(SUM(pg_total_relation_size(c.oid)),0) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relkind IN ('r','m')"))
        log=(directory/'server.log').open('w');startup=time.perf_counter();server=subprocess.Popen([str(binary),'--config',str(cfg),'serve',*(['--external-worker'] if db and args.local_processes else [])],stdout=log,stderr=log)
        children=[server];origins=[origin];extra_logs=[]
        try:
            for _ in range(100):
                try:
                    with urllib.request.urlopen(origin+'/health',timeout=1):break
                except Exception:assert server.poll() is None;time.sleep(.1)
            else:raise AssertionError('Server readiness timeout')
            results.setdefault('startup_to_health_ms',{})[engine]=round((time.perf_counter()-startup)*1000,3)
            if db and args.local_processes:
                with socket.socket() as sock:sock.bind(('127.0.0.1',0));second_port=sock.getsockname()[1]
                cfg2=directory/'node-b.toml';cfg2.write_text(cfg.read_text().replace(f'listen = "127.0.0.1:{port}"',f'listen = "127.0.0.1:{second_port}"'))
                second_log=(directory/'node-b.log').open('w');extra_logs.append(second_log)
                second=subprocess.Popen([str(binary),'--config',str(cfg2),'serve','--external-worker'],stdout=second_log,stderr=second_log);children.append(second);origins.append(f'http://127.0.0.1:{second_port}')
                for _ in range(100):
                    try:
                        with urllib.request.urlopen(origins[1]+'/health',timeout=1):break
                    except Exception:assert second.poll() is None;time.sleep(.1)
                else:raise AssertionError('Second node readiness timeout')
                worker_log=(directory/'worker.log').open('w');extra_logs.append(worker_log)
                children.append(subprocess.Popen([str(binary),'--config',str(cfg),'worker'],stdout=worker_log,stderr=worker_log))
            idle_rss=[rss(child.pid) for child in children]
            paths={'home':'/','story':'/journal-1','search':'/search?q=publishing'}
            if args.commerce:
                import re
                with urllib.request.urlopen(origin+'/shop') as response:catalog=response.read().decode()
                product_links=re.findall(r'href="(/shop/products/[0-9a-f-]{36})"',catalog)
                assert len(set(product_links))==4,'Demo catalog must render all four published products'
                paths.update({'catalog':'/shop','product':product_links[0]})
            if args.discovery:paths.update({'sitemap':'/sitemap.xml','sitemap_index':'/sitemap-index.xml'})
            started=time.perf_counter()
            with urllib.request.urlopen(origin) as r:r.read()
            results.setdefault('cold_first_request_ms',{})[engine]=round((time.perf_counter()-started)*1000,3)
            with urllib.request.urlopen(origin+'/assets/builder.js') as r:assets=r.read()
            import gzip
            results['studio_asset_bytes']={'raw':len(assets),'gzip':len(gzip.compress(assets))}
            results['composition_fixture']='20 dynamic cards with typed field bindings and three explicit reusable component parameters' if args.composed else 'default theme'
            for endpoint,path in paths.items():
                for _ in range(10):
                    with urllib.request.urlopen(origin+path) as r:r.read()
                for repeat in range(args.repeats):
                    for concurrency in [1,10,*([32] if args.stress else [])]:results['profiles'].append({'system':'wpalt','database':engine,'endpoint':endpoint,'repeat':repeat+1,'processes':len(origins),**load([o+path for o in origins],concurrency)})
            files={str(f.relative_to(directory)):f.stat().st_size for f in directory.rglob('*') if f.is_file() and f.suffix not in ('.log','.toml')}
            history=directory/'data'/'background-jobs.json'
            cycles=json.loads(history.read_text()) if history.exists() else []
            results.setdefault('runtime',{})[engine]={'rss_ready_bytes_by_process':idle_rss,'rss_after_load_bytes_by_process':[rss(child.pid) for child in children],'rss_ready_bytes':sum(idle_rss),'rss_after_load_bytes':sum(rss(child.pid) for child in children),'processes':len(origins),'workers':len(children)-len(origins),'rss_measurement':'Resident snapshots, not peak memory; includes startup password-verification dummy hash allocation.','log_bytes':(directory/'server.log').stat().st_size,'site_files_bytes':sum(files.values()),'site_file_bytes':files,'background_cycles':cycles}
        finally:
            for child in children:
                child.terminate();child.wait(timeout=65)
                assert child.returncode==0, 'Benchmark process did not drain cleanly'
            log.close()
            for sink in extra_logs:sink.close()
        if args.debug:
            records=[]
            for logfile in [directory/'server.log',directory/'node-b.log']:
                if logfile.exists():records.extend(json.loads(line) for line in logfile.read_text().splitlines() if line.startswith('{'))
            query_counts={}
            for record in records:
                if record.get('target')=='sqlx::query':
                    for span in record.get('spans',[]):
                        rid=span.get('request_id')
                        if rid:query_counts[rid]=query_counts.get(rid,0)+1
            assert query_counts, "Debug SQL telemetry must contain correlated native queries"
            for profile in results['profiles']:
                if profile['database']==engine:
                    request_ids=profile.pop('request_ids')
                    assert all(request_ids), 'Responses must carry request identity'
                    counts=[query_counts.get(rid,0) for rid in request_ids]
                    # Eligible public cache hits can legitimately execute zero native SQL.
                    profile['native_correlated_sql_queries_per_request']={'min':min(counts),'max':max(counts)}
                    profile['coordination_sql_queries_per_request']=1 if engine=='postgres' and args.local_processes else 0
                    profile['sql_queries_per_request']={'min':min(counts)+profile['coordination_sql_queries_per_request'],'max':max(counts)+profile['coordination_sql_queries_per_request']}
        else:
            for profile in results['profiles']:
                if profile['database']==engine:profile.pop('request_ids',None)
    if args.wordpress_url:
        for endpoint,path in [('home','/'),('story','/journal-1'),('search','/?s=publishing')]:
            for _ in range(10):
                with urllib.request.urlopen(args.wordpress_url+path) as r:r.read()
            for concurrency in [1,10]:results['profiles'].append({'system':'wordpress-reference','database':'mariadb','endpoint':endpoint,**load([args.wordpress_url+path],concurrency)})
assert results['binary_sha256']==hashlib.sha256(binary.read_bytes()).hexdigest(),'Measured executable changed during benchmark'
Path(args.output).parent.mkdir(parents=True,exist_ok=True);Path(args.output).write_text(json.dumps(results,indent=2)+'\n')
print('Measured',len(results['profiles']),'HTTP scenarios. Results:',args.output)
