#!/usr/bin/env python3
"""Actual frozen response-document queries and bytes; observations, never an SLA."""
import argparse
import hashlib
import csv
import io
import http.cookiejar
import json
import math
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
p.add_argument('--source-binary',help='Optional authentic native24/portable19 source executable for fresh source recovery')
p.add_argument('--profile', choices=['debug', 'release'], required=True)
p.add_argument('--postgres', default=os.environ.get('TEST_DATABASE_URL'))
p.add_argument('--psql', default=shutil.which('psql'))
p.add_argument('--output', default='work/d11-document-observations.json')
a = p.parse_args()
binary = Path(a.binary).resolve()
assert not a.postgres or a.psql
observations = []
producer = Path(a.source_binary).resolve() if a.source_binary else binary

def distribution(samples):
    return {'samples': len(samples), 'p50_ms': round(statistics.median(samples), 3),
            'p95_ms': round(sorted(samples)[max(0, int(len(samples)*.95)-1)], 3),
            'max_ms': round(max(samples), 3)}

for engine in ['sqlite', 'postgres'] if a.postgres else ['sqlite']:
    with tempfile.TemporaryDirectory(prefix='wpalt-documents-measure-') as temporary:
        root = Path(temporary)
        schema = 'wpalt_documents_' + uuid.uuid4().hex
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
        private_probe = "PRIVATE_ANSWER_PROBE" + secrets.token_hex(12)
        config = root/'site.toml'
        config.write_text(f'database_url={json.dumps(url)}\ndata_dir={json.dumps(str(root/"data"))}\nlisten="127.0.0.1:{port}"\nbase_url="{origin}"\ndebug=true\ndatabase_connections=1\nscheduler_seconds=3600\n')
        config.chmod(0o600)
        key = root/'documents.pk8'
        trust = json.loads(subprocess.check_output([str(binary),'document-key',str(key)],text=True))['public_key_sha256']
        running_binary=producer
        if not a.source_binary:
            with config.open('a') as configuration:
                configuration.write('\n[documents]\nenabled=true\nkey_file='+json.dumps(str(key))+'\n')
        def native(*argv, stdin=None):
            result = subprocess.run([str(running_binary), '--config', str(config), *argv], input=stdin, text=True, capture_output=True, timeout=120)
            assert result.returncode == 0, result.stderr.replace(password, '[redacted]')[-1500:]
            return result.stdout
        def sql(statement):
            if engine == 'postgres': return pg('SET search_path TO '+schema+'; '+statement)
            with sqlite3.connect(root/'site.db') as connection:
                return connection.execute(statement).fetchall()
        try:
            native('init', '--admin-email', 'owner@example.test', stdin=password+'\n')
            client = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
            log = (root/'server.log').open('w')
            server = subprocess.Popen([str(running_binary), '--config', str(config), 'serve', '--external-worker'], stdout=log, stderr=log)
            def get(path):
                with client.open(origin+path, timeout=30) as response:return response.read()
            for _ in range(100):
                assert server.poll() is None,'Application stopped during readiness'
                try:get('/login');break
                except OSError:time.sleep(.05)
            else:raise AssertionError('Readiness timeout')
            def post(path,value,form=False):
                data=urllib.parse.urlencode(value).encode() if form else json.dumps(value).encode()
                with client.open(urllib.request.Request(origin+path,data=data,headers={'Origin':origin,'Content-Type':'application/x-www-form-urlencoded' if form else 'application/json'}),timeout=30) as response:return response.read(),response.url
            post('/login',{'email':'owner@example.test','password':password},True)
            csrf=re.search(r'name="csrf" value="([^"]+)"',get('/admin/forms').decode()).group(1)
            _,location=post('/admin/forms',{'csrf':csrf,'title':'Measured local quotation'},True)
            identifier=location.rsplit('/',1)[1]
            fields=[{'name':'private_note','schema':{'kind':'string','required':True}},
                    {'name':'signature','schema':{'kind':'object','required':True,'fields':{'name':{'kind':'string','required':True},'accepted':{'kind':'boolean','required':True}}},'widget':{'kind':'Signature','statement':'Frozen measured original statement.'}}]
            definition={'title':'Measured private document','fields':fields,'max_entries':10000}
            post('/api/admin/forms/'+identifier,{'csrf':csrf,'version':1,'definition':definition,'publish':True})
            for n in range(1000):
                body,_=post('/api/forms/'+identifier+'/entries',{'version':2,'key':str(uuid.uuid4()),'values':{'private_note':private_probe+' · 日本語 · '*20,'signature':{'name':'Measured declaration','accepted':True}}})
                entry=json.loads(body)['entry']
            original_schema=schema
            if a.source_binary:
                server.terminate();server.wait(timeout=15);server=None
                snapshot=root/'original-d10.json';native('backup',str(snapshot))
                assert json.loads(snapshot.read_bytes())['format']=='wpalt-backup-v19'
                previous=url
                url='sqlite://'+str(root/'recovered.db')+'?mode=rwc'
                if engine=='postgres':
                    schema=original_schema+'_recovered';pg('CREATE SCHEMA '+schema)
                    parsed=urllib.parse.urlsplit(a.postgres)
                    url=urllib.parse.urlunsplit(parsed._replace(query=urllib.parse.urlencode(urllib.parse.parse_qsl(parsed.query)+[('options','-c search_path='+schema)])))
                text=config.read_text().replace(json.dumps(previous),json.dumps(url)).replace(json.dumps(str(root/'data')),json.dumps(str(root/'recovered')))
                config.write_text(text+'\n[documents]\nenabled=true\nkey_file='+json.dumps(str(key))+'\n')
                running_binary=binary;native('restore',str(snapshot))
                log.close();log=(root/'server.log').open('w')
                server=subprocess.Popen([str(binary),'--config',str(config),'serve','--external-worker'],stdout=log,stderr=log)
                for _ in range(100):
                    assert server.poll() is None,'Recovered application stopped during readiness'
                    try:get('/login');break
                    except OSError:time.sleep(.05)
                else:raise AssertionError('Recovery readiness timeout')
                post('/login',{'email':'owner@example.test','password':password},True)
                csrf=re.search(r'name="csrf" value="([^"]+)"',get('/admin/forms').decode()).group(1)
            path='/admin/forms/'+identifier+'/entries/'+entry+'/document'
            offset=(root/'server.log').stat().st_size
            first,_=post(path,{'csrf':csrf,'kind':'html'},True);time.sleep(.03)
            with (root/'server.log').open() as diagnostics:
                diagnostics.seek(offset);emitted=diagnostics.read(4*1024*1024+1)
            assert len(emitted)<=4*1024*1024
            selects=[]
            for line in emitted.splitlines():
                try:event=json.loads(line)
                except ValueError:continue
                details=event.get('fields',{});statement=details.get('db.statement') or details.get('summary','')
                if event.get('target','').startswith('sqlx') and statement.lstrip().upper().startswith('SELECT'):selects.append(details.get('summary',statement.splitlines()[0])[:180])
            assert selects,'Actual SQL diagnostics required'
            latencies=[]
            for n in range(35):
                began=time.perf_counter();last,_=post(path,{'csrf':csrf,'kind':'html'},True)
                if n>=5:latencies.append((time.perf_counter()-began)*1000)
                assert json.loads(last)['format']=='wpalt-attested-document-bundle-v1'
            bundle_file=root/'response.json';bundle_file.write_bytes(last);bundle_file.chmod(0o600)
            verified=json.loads(subprocess.check_output([str(binary),'document-verify',str(bundle_file),'--trusted-key-sha256',trust],text=True))
            assert verified['valid']
            import base64
            manifest=json.loads(base64.b64decode(json.loads(last)['manifest']))
            assert manifest['source']['form_version']==2 and manifest['source']['entry_id']==entry
            accepted={field['name']:field for field in manifest['source']['fields']}
            assert accepted['private_note']['value']==private_probe+' · 日本語 · '*20
            assert accepted['signature']['statement']=='Frozen measured original statement.'
            assert accepted['signature']['value']=={'name':'Measured declaration','accepted':True}
            source_query="SELECT e.values_json,e.created_at,e.form_version,p.definition FROM form_entries e JOIN form_publications p ON p.form_id=e.form_id AND p.version=e.form_version WHERE e.form_id='"+identifier+"' AND e.id='"+entry+"'"
            sql('ANALYZE form_entries' if engine=='postgres' else 'ANALYZE')
            plan=sql(('EXPLAIN (ANALYZE,BUFFERS) ' if engine=='postgres' else 'EXPLAIN QUERY PLAN ')+source_query)
            if engine=='sqlite':database_bytes=sum(f.stat().st_size for f in root.glob(('recovered' if a.source_binary else 'site')+'.db*'))
            else:database_bytes=int(pg("SELECT SUM(pg_total_relation_size(quote_ident(schemaname)||'.'||quote_ident(tablename))) FROM pg_tables WHERE schemaname='"+schema+"'"))
            with (root/'server.log').open() as diagnostics:
                carry=''
                while chunk:=diagnostics.read(128*1024):
                    joined=carry+chunk
                    assert all(secret not in joined for secret in [password,csrf,private_probe]),'Debug logs disclose a private answer/session secret'
                    carry=joined[-256:]
            observations.append({'engine':engine,'actual_http_responses':1000,'fields':len(fields),'html_generation':distribution(latencies),'artifact_bytes':verified['artifact_bytes'],'bundle_bytes':len(last),'document_selects':len(selects),'select_shapes':sorted(set(selects)),'source_query_plan':plan,'warm_application_rss_bytes':int(subprocess.check_output(['ps','-o','rss=','-p',str(server.pid)],text=True).strip())*1024,'database_bytes':database_bytes,'database_scope':'SQLite DB/WAL/SHM' if engine=='sqlite' else 'All fixture PostgreSQL relations/indexes; database process memory excluded','debug_secret_and_answer_redaction':True})
        finally:
            if server: server.terminate(); server.wait(timeout=15)
            if log: log.close()
            if engine=='postgres':
                pg('DROP SCHEMA '+schema+' CASCADE')
                if a.source_binary and schema.endswith('_recovered'):pg('DROP SCHEMA '+schema[:-10]+' CASCADE')

receipt={'authentic_source_binary_sha256':hashlib.sha256(producer.read_bytes()).hexdigest() if a.source_binary else None,'fresh_source_recovery':bool(a.source_binary),'profile':a.profile,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'binary_bytes':binary.stat().st_size,'scope':'1,000 actual serial accepted HTTP acknowledgments, two fields; one database connection. Five warmups/30 measured serial private HTML bundle requests, debug SQL enabled even in release. Actual native signature verification. Warm RSS sample excludes PostgreSQL/scanner/browser workers; not peak, throughput, PDF/scanner latency, feature attribution, TLS/CWV/WordPress efficiency or SLA.','observations':observations}
target=Path(a.output);target.parent.mkdir(parents=True,exist_ok=True);target.write_text(json.dumps(receipt,indent=2)+'\n')
print('PASS: private document source/query/resource/redaction observations ('+','.join(o['engine'] for o in observations)+')')
