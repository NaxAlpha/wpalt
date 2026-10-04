#!/usr/bin/env python3
"""Reproducible HTTP/document baseline. Uses a fresh database; never clears an existing site.
Optional PostgreSQL URL MUST identify an empty, isolated test database.
"""
import argparse,concurrent.futures,hashlib,json,math,os,platform,secrets,socket,subprocess,tempfile,time,urllib.request
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',default='target/release/wpalt');p.add_argument('--posts',type=int,default=1000);p.add_argument('--requests',type=int,default=200);p.add_argument('--postgres-url');p.add_argument('--wordpress-url');p.add_argument('--composed',action='store_true');p.add_argument('--discovery',action='store_true');p.add_argument('--commerce',action='store_true');p.add_argument('--debug',action='store_true');p.add_argument('--output',default='work/benchmark.json');args=p.parse_args()
binary=Path(args.binary).resolve();results={'machine':{'platform':platform.platform(),'cpu':platform.processor(),'logical_cpus':os.cpu_count()},'posts_requested':args.posts,'requests_per_scenario':args.requests,'binary_bytes':binary.stat().st_size,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'conditions':'Full uncompressed HTML document; warm application; no browser asset/render timing; no application response cache; localhost; Python HTTP client overhead included.','debug_logging':args.debug,'commerce_fixture':('four physical/digital/monthly membership/group-booking products, one UTC slot, no invented payments' if args.commerce else None),'profiles':[]}
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
        cfg.write_text(f'database_url = "{url}"\ndata_dir = "{directory}/data"\nlisten = "127.0.0.1:{port}"\nbase_url = "{origin}"\ndebug = {str(args.debug).lower()}\n')
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
        log=(directory/'server.log').open('w');server=subprocess.Popen([str(binary),'--config',str(cfg),'serve'],stdout=log,stderr=log)
        try:
            for _ in range(100):
                try:
                    with urllib.request.urlopen(origin+'/health',timeout=1):break
                except Exception:assert server.poll() is None;time.sleep(.1)
            else:raise AssertionError('Server readiness timeout')
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
                for concurrency in [1,10]:results['profiles'].append({'system':'wpalt','database':engine,'endpoint':endpoint,**load(origin+path,concurrency)})
            results.setdefault('runtime',{})[engine]={'rss_after_load_bytes':rss(server.pid),'log_bytes':(directory/'server.log').stat().st_size,'site_files_bytes':sum(f.stat().st_size for f in directory.rglob('*') if f.is_file() and f.name not in ('server.log','config.toml'))}
        finally:server.terminate();server.wait(timeout=10);log.close()
    if args.wordpress_url:
        for endpoint,path in [('home','/'),('story','/journal-1'),('search','/?s=publishing')]:
            for _ in range(10):
                with urllib.request.urlopen(args.wordpress_url+path) as r:r.read()
            for concurrency in [1,10]:results['profiles'].append({'system':'wordpress-reference','database':'mariadb','endpoint':endpoint,**load(args.wordpress_url+path,concurrency)})
assert results['binary_sha256']==hashlib.sha256(binary.read_bytes()).hexdigest(),'Measured executable changed during benchmark'
Path(args.output).parent.mkdir(parents=True,exist_ok=True);Path(args.output).write_text(json.dumps(results,indent=2)+'\n')
print('Measured',len(results['profiles']),'HTTP scenarios. Results:',args.output)
