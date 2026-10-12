#!/usr/bin/env python3
"""Actual canonical forms and frozen survey workload; observations, never an SLA."""
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
p.add_argument('--profile', choices=['debug', 'release'], required=True)
p.add_argument('--postgres', default=os.environ.get('TEST_DATABASE_URL'))
p.add_argument('--psql', default=shutil.which('psql'))
p.add_argument('--output', default='work/d10-forms-observations.json')
a = p.parse_args()
binary = Path(a.binary).resolve()
assert not a.postgres or a.psql
observations = []

def distribution(samples):
    return {'samples': len(samples), 'p50_ms': round(statistics.median(samples), 3),
            'p95_ms': round(sorted(samples)[max(0, int(len(samples)*.95)-1)], 3),
            'max_ms': round(max(samples), 3)}

for engine in ['sqlite', 'postgres'] if a.postgres else ['sqlite']:
    with tempfile.TemporaryDirectory(prefix='wpalt-forms-measure-') as temporary:
        root = Path(temporary)
        schema = 'wpalt_forms_' + uuid.uuid4().hex
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
            client = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
            log = (root/'server.log').open('w')
            server = subprocess.Popen([str(binary), '--config', str(config), 'serve', '--external-worker'], stdout=log, stderr=log)
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
            fields=[{'name':'quantity','schema':{'kind':'number','required':True}},{'name':'rate','schema':{'kind':'number','required':True}},
                {'name':'total','schema':{'kind':'number','required':True},'calculation':{'operation':'Expression','expression':{'operation':'Round','decimals':2,'operand':{'operation':'Divide','left':{'operation':'Multiply','operands':[{'operation':'Field','field':'quantity'},{'operation':'Field','field':'rate'}]},'right':{'operation':'Literal','value':3}}}}},
                {'name':'private_note','schema':{'kind':'string','label':'=SUM(1,2)\n"Owner field"'},'visible_when':{'operation':'Greater','field':'total','value':10}}]
            definition={'title':'Measured local quotation','fields':fields,'max_entries':10000}
            post('/api/admin/forms/'+identifier,{'csrf':csrf,'version':1,'definition':definition,'publish':True})
            samples=[]
            for n in range(2000):
                values={'quantity':2 if n%2==0 else 6,'rate':12.5,'total':999,'private_note':private_probe}
                begin=time.perf_counter()
                post('/api/forms/'+identifier+'/entries',{'version':2,'key':str(uuid.uuid4()),'values':values})
                samples.append((time.perf_counter()-begin)*1000)
            def timed(path,expected):
                values=[]
                for n in range(35):
                    begin=time.perf_counter();result=json.loads(get(path));elapsed=(time.perf_counter()-begin)*1000
                    assert result['responses']==expected
                    assert private_probe not in json.dumps(result)
                    if n>=5:values.append(elapsed)
                return result,distribution(values)
            path='/api/admin/forms/'+identifier+'/report?version=2'
            # Actual diagnostics around one summary count its select shapes,
            # rather than assuming a fixed count from the implementation.
            offset=(root/'server.log').stat().st_size
            first=json.loads(get(path));time.sleep(.03)
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
            report,latency=timed(path,2000)
            note=next(field for field in report['fields'] if field['name']=='private_note')
            assert (note['eligible'],note['answered'],note['skipped'])==(1000,1000,1000)
            assert math.isclose(next(field for field in report['fields'] if field['name']=='total')['numeric']['mean'],(8.33+25)/2,rel_tol=1e-12)
            exported=get('/admin/forms/'+identifier+'/report.csv?version=2').decode()
            assert private_probe not in exported
            csv_rows=list(csv.reader(io.StringIO(exported)))
            assert csv_rows and all(len(row)==19 for row in csv_rows), 'Quoted multiline cells must preserve the complete schema'
            assert any(row[2]=='\'=SUM(1,2)\n"Owner field"' for row in csv_rows[1:]), 'Spreadsheet formula labels need literal treatment'
            assert all(row[15]==identifier and row[16]==report['definition_sha256'] for row in csv_rows[1:]), 'Export rows identify the immutable definition'
            preview=[]
            for n in range(35):
                begin=time.perf_counter();body,_=post('/api/admin/forms/'+identifier+'/preview',{'csrf':csrf,'definition':definition,'values':{'quantity':2,'rate':12.5,'private_note':private_probe,'total':999}});result=json.loads(body)
                assert result['values']['total']==8.33 and 'private_note' not in result['values']
                if n>=5:preview.append((time.perf_counter()-begin)*1000)
            cursor_query="SELECT values_json FROM form_entries WHERE form_id='"+identifier+"' AND form_version=2 AND created_at>=0 AND created_at<9223372036854775807 ORDER BY created_at,id LIMIT 10001"
            sql('ANALYZE form_entries' if engine=='postgres' else 'ANALYZE')
            plan=sql(('EXPLAIN (ANALYZE,BUFFERS) ' if engine=='postgres' else 'EXPLAIN QUERY PLAN ')+cursor_query)
            selective_query=cursor_query.replace('created_at>=0','created_at>=4070908800')
            selective_plan=sql(('EXPLAIN (ANALYZE,BUFFERS) ' if engine=='postgres' else 'EXPLAIN QUERY PLAN ')+selective_query)
            if engine=='sqlite':database_bytes=sum(f.stat().st_size for f in root.glob('site.db*'))
            else:database_bytes=int(pg("SELECT SUM(pg_total_relation_size(quote_ident(schemaname)||'.'||quote_ident(tablename))) FROM pg_tables WHERE schemaname='"+schema+"'"))
            with (root/'server.log').open() as diagnostics:
                carry=''
                while chunk:=diagnostics.read(128*1024):
                    joined=carry+chunk
                    assert all(secret not in joined for secret in [password,csrf,private_probe]),'Debug logs disclose a private answer/session secret'
                    carry=joined[-256:]
            observations.append({'engine':engine,'actual_http_responses':2000,'canonical_value_bytes':report['value_bytes'],'fields':len(fields),'report':latency,'working_preview':distribution(preview),'serial_collection':distribution(samples),'report_selects':len(selects),'report_select_shapes':sorted(set(selects)),'cohort_query_plan':plan,'empty_future_cohort_query_plan':selective_plan,'warm_application_rss_bytes':int(subprocess.check_output(['ps','-o','rss=','-p',str(server.pid)],text=True).strip())*1024,'database_bytes':database_bytes,'database_scope':'SQLite DB/WAL/SHM' if engine=='sqlite' else 'All fixture PostgreSQL relations/indexes; database process memory excluded','debug_secret_and_answer_redaction':True})
        finally:
            if server: server.terminate(); server.wait(timeout=15)
            if log: log.close()
            if engine=='postgres': pg('DROP SCHEMA '+schema+' CASCADE')

receipt={'profile':a.profile,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'binary_bytes':binary.stat().st_size,'scope':'2,000 real serial canonical HTTP collections, four fields with nested arithmetic/conditional private answers; single database connection. Five warmups/30 measured serial reports/previews, with debug SQL logging enabled even in release. Warm RSS sample is not peak memory or per-feature attribution; PostgreSQL process memory excluded. No throughput/TLS/CWV/WordPress efficiency/SLA claim.','observations':observations}
target=Path(a.output);target.parent.mkdir(parents=True,exist_ok=True);target.write_text(json.dumps(receipt,indent=2)+'\n')
print('PASS: canonical form collection, frozen survey report, SQL/resource/redaction observations ('+','.join(o['engine'] for o in observations)+')')
