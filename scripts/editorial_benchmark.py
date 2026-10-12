#!/usr/bin/env python3
"""Actual large editorial workflow: finite serial observations, never an SLA."""
import argparse
import hashlib
import http.cookiejar
import json
import os
import re
import secrets
import shutil
import socket
import sqlite3
import statistics
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('--binary', required=True)
p.add_argument('--profile', choices=['debug', 'release'], required=True)
p.add_argument('--postgres', default=os.environ.get('TEST_DATABASE_URL'))
p.add_argument('--psql', default=shutil.which('psql'))
p.add_argument('--output', default='work/d09-editorial-observations.json')
a = p.parse_args()
binary = Path(a.binary).resolve()
assert not a.postgres or a.psql
observations = []

def distribution(samples):
    return {'samples': len(samples), 'p50_ms': round(statistics.median(samples), 3),
            'p95_ms': round(sorted(samples)[max(0, int(len(samples)*.95)-1)], 3),
            'max_ms': round(max(samples), 3)}

for engine in ['sqlite', 'postgres'] if a.postgres else ['sqlite']:
    with tempfile.TemporaryDirectory(prefix='wpalt-editorial-measure-') as temporary:
        root = Path(temporary)
        schema = 'wpalt_editorial_' + uuid.uuid4().hex
        server = log = None
        def pg(statement):
            return subprocess.check_output([a.psql, a.postgres, '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-c', statement], text=True).strip()
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]
        origin = f'http://127.0.0.1:{port}'
        url = 'sqlite://' + str(root/'site.db') + '?mode=rwc'
        if engine == 'postgres':
            pg('CREATE SCHEMA ' + schema)
            parsed = urllib.parse.urlsplit(a.postgres)
            url = urllib.parse.urlunsplit(parsed._replace(query=urllib.parse.urlencode(urllib.parse.parse_qsl(parsed.query) + [('options', '-c search_path='+schema)])))
        password = secrets.token_urlsafe(24)
        private_probe = "EDITORIAL_PROBE" + secrets.token_hex(12)
        config = root/'site.toml'
        config.write_text(f'database_url={json.dumps(url)}\ndata_dir={json.dumps(str(root/"data"))}\nlisten="127.0.0.1:{port}"\nbase_url="{origin}"\ndebug=true\ndatabase_connections=1\nscheduler_seconds=3600\n[analysis]\nbatch_sources=32\nbatch_seconds=10\n')
        config.chmod(0o600)
        def native(*argv, stdin=None):
            result = subprocess.run([str(binary), '--config', str(config), *argv], input=stdin, text=True, capture_output=True, timeout=120)
            assert result.returncode == 0, result.stderr.replace(password, '[redacted]')[-1500:]
            return result.stdout
        def sql(statement):
            if engine == 'postgres': return pg('SET search_path TO '+schema+'; '+statement)
            with sqlite3.connect(root/'site.db') as connection:
                return connection.execute(statement).fetchall()
        try:
            native('init', '--admin-email', 'owner@example.test', stdin=password+'\n')
            native('seed-demo', '--posts', '1004')
            run_id = json.loads(native('editorial-analysis', 'start'))['id']
            # SQLite/single-server PostgreSQL use the normal embedded worker.
            # The first scheduler cycle runs immediately; later ones are held so
            # progress/restart evidence stays deterministic without sleeping an hour.
            log = (root/'worker-stage.log').open('w')
            server = subprocess.Popen([str(binary), '--config', str(config), 'serve'], stdout=log, stderr=log)
            history_path = root/'data/background-jobs.json'
            for _ in range(300):
                assert server.poll() is None, 'Normal worker server stopped'
                try: history = json.loads(history_path.read_text())
                except (OSError, ValueError): history = []
                if history and history[0]['state'] in ['succeeded','failed']: break
                time.sleep(.05)
            else: raise AssertionError('Actual normal worker cycle did not finish')
            server.terminate(); server.wait(timeout=15); server=None
            log.close(); log=None
            history = json.loads(native('job-history'))
            stage = next(s for s in history[0]['stages'] if s['name']=='editorial-analysis')
            assert stage['succeeded'] and stage['count']==32, 'Actual normal worker stage must commit the first batch'
            # The application resumes a batch committed by its prior real worker process.
            log = (root/'server.log').open('w')
            server = subprocess.Popen([str(binary), '--config', str(config), 'serve', '--external-worker'], stdout=log, stderr=log)
            client = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
            def get(path):
                with client.open(origin+path, timeout=30) as response: return response.read()
            for _ in range(100):
                assert server.poll() is None, 'Application stopped during readiness'
                try: get('/login'); break
                except OSError: time.sleep(.1)
            else: raise AssertionError('Readiness timeout')
            client.open(urllib.request.Request(origin+'/login', data=urllib.parse.urlencode({'email':'owner@example.test','password':password}).encode(), headers={'Origin':origin}), timeout=15).read()
            csrf = re.search(r'data-csrf="([^"]+)"', get('/admin/builder').decode()).group(1)
            def post(path, value):
                begin = time.perf_counter()
                with client.open(urllib.request.Request(origin+path, data=json.dumps(value).encode(), headers={'Origin':origin, 'Content-Type':'application/json', 'X-CSRF-Token':csrf}), timeout=30) as response: body = response.read()
                return (time.perf_counter()-begin)*1000, json.loads(body)
            def rss(): return int(subprocess.check_output(['ps', '-o', 'rss=', '-p', str(server.pid)], text=True).strip())*1024
            before_rss = rss()
            def finish(identifier):
                timings, query_receipt = [], None
                for _ in range(100):
                    run = next(r for r in json.loads(get('/api/admin/analysis')) if r['id']==identifier)
                    if run['state']=='complete': return run, timings, query_receipt
                    assert run['state'] in ['rendering','classifying'], run['state']
                    offset = (root/'server.log').stat().st_size
                    elapsed, _ = post('/api/admin/analysis/'+identifier, {'csrf':csrf,'version':run['version'],'action':'advance'})
                    timings.append(elapsed)
                    if query_receipt is None:
                        time.sleep(.03)
                        with (root/'server.log').open() as diagnostics:
                            diagnostics.seek(offset); emitted = diagnostics.read(4*1024*1024+1)
                        assert len(emitted)<=4*1024*1024, 'Bounded query receipt overflow'
                        selects = []
                        for line in emitted.splitlines():
                            try: event=json.loads(line)
                            except ValueError: continue
                            fields=event.get('fields',{}); statement=fields.get('db.statement') or fields.get('summary','')
                            if event.get('target','').startswith('sqlx') and statement.lstrip().upper().startswith('SELECT'):
                                selects.append(fields.get('summary',statement.splitlines()[0])[:200])
                        assert selects, 'Actual query diagnostics required'
                        query_receipt={'first_observed_batch_selects':len(selects),'distinct_select_shapes':sorted(set(selects)), 'phase':run['state']}
                raise AssertionError('Finite analysis failed to finish')
            first, samples, query_sample = finish(run_id)
            assert first['total']==1006 and first['cursor']==1006
            pages=json.loads(get('/api/admin/analysis/'+run_id+'/sources'))
            source=next(row for row in pages if row['publication'])
            individual=[]
            for n in range(35):
                elapsed, result=post('/api/admin/insights/'+source['publication'], {'csrf':csrf,'copy':'public','focus':private_probe})
                assert result['copy']=='public'
                if n>=5: individual.append(elapsed)
            _, created=post('/api/admin/analysis', {'csrf':csrf,'focus':private_probe})
            second, reused_timings, _=finish(created['id'])
            assert second['reused']==1006, 'Identical actual HTML must reuse all parser facts'
            sql("ANALYZE analysis_pages" if engine=='postgres' else 'ANALYZE')
            cursor_query="SELECT ordinal,path,publication FROM analysis_pages WHERE job_id='"+created['id']+"' AND ordinal>=500 ORDER BY ordinal LIMIT 32"
            plan=sql(('EXPLAIN (ANALYZE, BUFFERS) ' if engine=='postgres' else 'EXPLAIN QUERY PLAN ')+cursor_query)
            if engine=='sqlite': database_bytes=sum(f.stat().st_size for f in root.glob('site.db*')); derived_bytes=None
            else:
                database_bytes=int(pg("SELECT SUM(pg_total_relation_size(quote_ident(schemaname)||'.'||quote_ident(tablename))) FROM pg_tables WHERE schemaname='"+schema+"'"))
                derived_bytes=int(pg("SELECT SUM(pg_total_relation_size(quote_ident(schemaname)||'.'||quote_ident(tablename))) FROM pg_tables WHERE schemaname='"+schema+"' AND tablename IN ('analysis_jobs','analysis_pages','analysis_inbound','analysis_epoch')"))
            # Debug evidence must not disclose request secrets or a private owner phrase.
            with (root/'server.log').open() as diagnostics:
                carry=''
                while chunk:=diagnostics.read(128*1024):
                    joined=carry+chunk
                    assert all(secret not in joined for secret in [password,csrf,private_probe,private_probe.lower()]), 'Debug logs disclose a request secret/private analysis phrase'
                    carry=joined[-256:]
            observations.append({'engine':engine,'publications':1005,'sources':first['total'],'edges':first['edge_count'],'html_bytes':first['html_bytes'],'serialized_derived_bytes':first['facts_bytes'],'resumed_after_normal_worker_batch':True,'normal_worker_stage_sources':stage['count'],'debug_secret_and_focus_redaction':True,'first_http_batches':distribution(samples),'identical_html_reuse_batches':distribution(reused_timings),'individual_public_insight':distribution(individual),'batch_queries':query_sample,'cursor_query_plan':plan,'retained_runs':2,'reused_sources':second['reused'],'warm_application_rss_before_analysis_bytes':before_rss,'warm_application_rss_after_analysis_bytes':rss(),'database_bytes':database_bytes,'derived_postgres_relations_bytes':derived_bytes,'database_scope':'SQLite DB/WAL/SHM' if engine=='sqlite' else 'All fixture PostgreSQL relations/indexes; database process memory excluded'})
        finally:
            if server: server.terminate(); server.wait(timeout=15)
            if log: log.close()
            if engine=='postgres': pg('DROP SCHEMA '+schema+' CASCADE')

receipt={'profile':a.profile,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'binary_bytes':binary.stat().st_size,'scope':'1004 seeded posts plus about publication and language home; actual single-connection database, real first embedded-worker cycle and restarted HTTP application, finite manual native batches, five warmups/30 serial selected-copy requests. Debug SQL logging enabled even for a release executable. RSS is a warm process sample, neither peak nor per-feature attribution; PostgreSQL process memory excluded. No throughput, TLS, CWV, universal latency or WordPress efficiency claim. Byte budgets describe serialized data, not on-disk or peak allocations.','observations':observations}
target=Path(a.output);target.parent.mkdir(parents=True,exist_ok=True);target.write_text(json.dumps(receipt,indent=2)+'\n')
print('PASS: populated editorial analysis, restart, HTML reuse, query/resource observations ('+','.join(o['engine'] for o in observations)+')')
