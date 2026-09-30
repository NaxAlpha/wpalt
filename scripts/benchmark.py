#!/usr/bin/env python3
"""Reproducible HTTP/document baseline. Uses a fresh database; never clears an existing site.
Optional PostgreSQL URL MUST identify an empty, isolated test database.
"""
import argparse,concurrent.futures,json,math,os,platform,secrets,socket,subprocess,tempfile,time,urllib.request
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',default='target/release/wpalt');p.add_argument('--posts',type=int,default=1000);p.add_argument('--requests',type=int,default=200);p.add_argument('--postgres-url');p.add_argument('--wordpress-url');p.add_argument('--output',default='work/benchmark.json');args=p.parse_args()
binary=Path(args.binary).resolve();results={'machine':{'platform':platform.platform(),'cpu':platform.processor(),'logical_cpus':os.cpu_count()},'posts_requested':args.posts,'requests_per_scenario':args.requests,'binary_bytes':binary.stat().st_size,'conditions':'Full uncompressed HTML document; warm application; no browser asset/render timing; no application response cache; localhost; Python HTTP client overhead included.','profiles':[]}
def percentile(values,p):return sorted(values)[max(0,math.ceil(len(values)*p)-1)]
def load(url,concurrency):
    def one(_):
        start=time.perf_counter()
        with urllib.request.urlopen(url,timeout=20) as r:assert r.status==200;data=r.read()
        return (time.perf_counter()-start)*1000,len(data)
    start=time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as workers:items=list(workers.map(one,range(args.requests)))
    elapsed=time.perf_counter()-start;latencies=[i[0] for i in items]
    return {'concurrency':concurrency,'requests':len(items),'p50_ms':round(percentile(latencies,.5),3),'p95_ms':round(percentile(latencies,.95),3),'p99_ms':round(percentile(latencies,.99),3),'requests_per_second':round(len(items)/elapsed,1),'document_bytes':items[0][1]}
def rss(pid):return int(subprocess.check_output(['ps','-o','rss=','-p',str(pid)],text=True).strip())*1024
with tempfile.TemporaryDirectory(prefix='wpalt-benchmark-') as temporary:
    root=Path(temporary)
    for engine,db in [('sqlite',None),*([('postgres',args.postgres_url)] if args.postgres_url else [])]:
        with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
        origin=f'http://127.0.0.1:{port}';directory=root/engine;directory.mkdir();cfg=directory/'config.toml'
        url=db or f'sqlite://{directory}/site.db?mode=rwc'
        cfg.write_text(f'database_url = "{url}"\ndata_dir = "{directory}/data"\nlisten = "127.0.0.1:{port}"\nbase_url = "{origin}"\n')
        def run(*arguments,input=None):
            r=subprocess.run([str(binary),'--config',str(cfg),*arguments],input=input,text=True,capture_output=True)
            assert r.returncode==0,(arguments,r.stderr)
        run('init','--admin-email','benchmark@example.test',input=secrets.token_urlsafe(24)+'\n');run('seed-demo','--posts',str(args.posts))
        log=(directory/'server.log').open('w');server=subprocess.Popen([str(binary),'--config',str(cfg),'serve'],stdout=log,stderr=log)
        try:
            for _ in range(100):
                try:
                    with urllib.request.urlopen(origin+'/health',timeout=1):break
                except Exception:assert server.poll() is None;time.sleep(.1)
            else:raise AssertionError('Server readiness timeout')
            paths={'home':'/','story':'/journal-1','search':'/search?q=publishing'}
            for endpoint,path in paths.items():
                for _ in range(10):
                    with urllib.request.urlopen(origin+path) as r:r.read()
                for concurrency in [1,10]:results['profiles'].append({'system':'wpalt','database':engine,'endpoint':endpoint,**load(origin+path,concurrency)})
            results.setdefault('runtime',{})[engine]={'rss_after_load_bytes':rss(server.pid),'site_files_bytes':sum(f.stat().st_size for f in directory.rglob('*') if f.is_file() and f.name not in ('server.log','config.toml'))}
        finally:server.terminate();server.wait(timeout=10);log.close()
    if args.wordpress_url:
        for endpoint,path in [('home','/'),('story','/journal-1'),('search','/?s=publishing')]:
            for _ in range(10):
                with urllib.request.urlopen(args.wordpress_url+path) as r:r.read()
            for concurrency in [1,10]:results['profiles'].append({'system':'wordpress-reference','database':'mariadb','endpoint':endpoint,**load(args.wordpress_url+path,concurrency)})
Path(args.output).parent.mkdir(parents=True,exist_ok=True);Path(args.output).write_text(json.dumps(results,indent=2)+'\n')
print('Measured',len(results['profiles']),'HTTP scenarios. Results:',args.output)
