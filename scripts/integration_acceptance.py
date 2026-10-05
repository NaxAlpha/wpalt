#!/usr/bin/env python3
"""Native owner credential -> external HTTP draft -> revocation/recovery journey."""
import argparse, hashlib, hmac, json, secrets, socket, sqlite3, subprocess, sys, tempfile, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import urllib.error, urllib.request
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--binary',default='target/debug/wpalt');args=parser.parse_args()
binary=Path(args.binary).resolve()
with tempfile.TemporaryDirectory(prefix='wpalt-integration-') as tmp:
    root=Path(tmp)
    with socket.socket() as socket_:socket_.bind(('127.0.0.1',0));port=socket_.getsockname()[1]
    origin=f'http://127.0.0.1:{port}'
    def config(name):
        file=root/(name+'.toml');file.write_text(f'database_url="sqlite://{root/name}.db?mode=rwc"\ndata_dir="{root/name}"\nbase_url="{origin}"\nlisten="127.0.0.1:{port}"\ndebug=true\n');return file
    cfg=config('site');target=config('recovered');password=secrets.token_urlsafe(24)
    def run(*commands,config_=cfg,stdin=None,ok=True):
        r=subprocess.run([str(binary),'--config',str(config_),*map(str,commands)],input=stdin,text=True,capture_output=True)
        assert (r.returncode==0)==ok,(commands,r.stderr)
        return r
    run('init','--admin-email','owner@example.test',stdin=password+'\n')
    run('seed-demo','--posts','30')
    def issue(name,draft):
        file=root/(name+'.token')
        argv=['integration','create','--user-email','owner@example.test','--name',name,'--days','7']
        if draft:argv+=['--draft']
        result=run(*argv,file);metadata=json.loads(result.stdout);token=file.read_text()
        assert len(token)==64 and file.stat().st_mode&0o077==0
        assert token not in result.stdout+result.stderr
        return file,metadata,token
    read_file,reader,read_token=issue('Exporter',False)
    draft_file,writer,draft_token=issue('Draft worker',True)
    inventory=run('integration','list').stdout
    assert read_token not in inventory and draft_token not in inventory
    run('integration','create','--user-email','owner@example.test','--name','Overwrite',read_file,ok=False)
    assert len(json.loads(run('integration','list').stdout)['credentials'])==2
    log=root/'server.log'
    def start(logfile,configuration=cfg):
        process=subprocess.Popen([str(binary),'--config',str(configuration),'serve'],stdout=logfile,stderr=logfile)
        deadline=time.monotonic()+10
        while time.monotonic()<deadline:
            assert process.poll() is None
            try:
                with urllib.request.urlopen(origin+'/health',timeout=1):return process
            except OSError:time.sleep(.1)
        raise AssertionError('Native server did not become ready')
    def stop(process):process.terminate();process.wait(timeout=10);assert process.returncode==0
    def request(path,token=read_token,method='GET',data=None,extra=None,status=200):
        headers={'Authorization':'Bearer '+token};headers.update(extra or {})
        if data is not None:headers['Content-Type']='application/json'
        req=urllib.request.Request(origin+path,method=method,headers=headers,data=json.dumps(data).encode() if data is not None else None)
        try:response=urllib.request.urlopen(req,timeout=10)
        except urllib.error.HTTPError as error:response=error
        with response:
            assert response.status==status,(path,response.status,response.read()[:1000])
            assert response.headers.get('Cache-Control')=='no-store'
            assert not response.headers.get('Set-Cookie')
            return json.loads(response.read())
    with log.open('w') as logfile:
        process=start(logfile)
        try:
            seen=[];path='/api/v1/content'
            while path:
                page=request(path);assert len(page['content'])<=25
                seen.extend(p['id'] for p in page['content']);path='/api/v1/content?after='+page['next'] if page['next'] else None
            assert len(seen)==len(set(seen)) and len(seen)>=30
            draft={'title':'An external proposal','slug':'external-proposal','kind':'post','body':'PRIVATE_EXTERNAL_DRAFT','action':'save','version':0}
            request('/api/v1/content',method='POST',data=draft,status=403)
            saved=request('/api/v1/content',token=draft_token,method='POST',data=draft)
            id_=saved['id'];assert saved['status']=='draft'
            record=request('/api/v1/content/'+id_)['content'];assert record['body']=='PRIVATE_EXTERNAL_DRAFT' and record['published_body']==''
            request('/api/v1/content',extra={'Cookie':'wpalt_session='+read_token},status=403)
            request('/api/v1/content',extra={'Origin':'https://untrusted.example'},status=403)
            request('/api/v1/content/'+id_,token=draft_token,method='PUT',data={**draft,'action':'publish','version':1},status=403)
            request('/api/v1/content/'+id_,token=draft_token,method='PUT',data={**draft,'version':1})
            request('/api/v1/content/'+id_,token=draft_token,method='PUT',data={**draft,'version':1},status=409)
            # Independent process talks to a bounded local model protocol fixture,
            # writes a private review artifact and can only create a native draft.
            received=[]
            class Model(BaseHTTPRequestHandler):
                def log_message(self,*args):pass
                def do_POST(self):
                    assert self.path=='/api/generate'
                    size=int(self.headers.get('Content-Length','0'));assert size<=512*1024
                    payload=json.loads(self.rfile.read(size));received.append(payload)
                    assert not self.headers.get('Authorization')
                    generated=json.dumps({'palette':'paper','font':'serif','reading_width':720,'home_columns':2,'gap':24}) if payload.get('format') else 'A separately reviewed local suggestion.'
                    raw=json.dumps({'response':generated}).encode()
                    self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
            model=ThreadingHTTPServer(('127.0.0.1',0),Model)
            thread=threading.Thread(target=model.serve_forever,daemon=True);thread.start()
            worker=Path(__file__).resolve().parents[1]/'examples/integrations/local_ai_worker.py'
            proposal=root/'proposal.json'
            def external(*argv,token_file=read_file,ok=True):
                result=subprocess.run([sys.executable,str(worker),'--site',origin,'--token-file',str(token_file),*map(str,argv)],text=True,capture_output=True,timeout=40)
                assert (result.returncode==0)==ok,result.stderr
                assert all(secret not in result.stdout+result.stderr for secret in [read_token,draft_token,'PRIVATE_EXTERNAL_DRAFT'])
                return result
            try:
                preview=json.loads(external('suggest','--source',id_,'--slug','local-ai-proposal','--ollama',f'http://127.0.0.1:{model.server_port}','--model','synthetic-contract-fixture','--output',proposal).stdout)
                assert received[0]['stream'] is False and 'PRIVATE_EXTERNAL_DRAFT' in received[0]['prompt']
                base=root/'base-theme.json';base.write_bytes((worker.parents[1]/'themes/field-journal.json').read_bytes());base.chmod(0o600)
                layout_proposal=root/'layout-proposal.json'
                layout_preview=json.loads(external('layout','--source',id_,'--base-theme',base,'--ollama',f'http://127.0.0.1:{model.server_port}','--model','synthetic-contract-fixture','--output',layout_proposal).stdout)
                exported=root/'generated-theme.json'
                external('export-layout',layout_proposal,'--execute','stale','--output',exported,ok=False);assert not exported.exists()
                external('export-layout',layout_proposal,'--execute',layout_preview['plan'],'--output',exported)
                package=json.loads(exported.read_text());assert package['templates']['content']['style']['width']==720
                assert package['templates']['home']['children'][2]['style']['columns']==2
                assert exported.stat().st_mode&0o077==0
                external('export-layout',layout_proposal,'--execute',layout_preview['plan'],'--output',exported,ok=False)

                assert proposal.stat().st_mode&0o077==0
                external('apply',proposal,'--execute','stale-plan',token_file=draft_file,ok=False)
                external('apply',proposal,'--execute',preview['plan'],ok=False) # Read-only key cannot create a draft.
                applied=json.loads(external('apply',proposal,'--execute',preview['plan'],token_file=draft_file).stdout)
                generated=request('/api/v1/content/'+applied['draft_id'])['content']
                assert generated['status']=='draft' and generated['published_body']==''
                assert 'separately reviewed' in generated['body']
                external('apply',proposal,'--execute',preview['plan'],token_file=draft_file,ok=False) # Unique slug prevents duplicate retry effects.
                request('/api/v1/content/'+id_,token=draft_token,method='PUT',data={**draft,'body':'Changed source','version':2})
                external('apply',proposal,'--execute',preview['plan'],token_file=draft_file,ok=False)
                external('suggest','--source',id_,'--slug','refused-cloud','--ollama','https://external.example','--model','fixture','--output',root/'refused.json',ok=False)
                assert not (root/'refused.json').exists()
            finally:model.shutdown();model.server_close();thread.join(timeout=5)
            # Actual external webhook process: retry the same event after a failed
            # destination response, verify signature, persist each delivered cursor.
            webhook_key=secrets.token_bytes(32);key_file=root/'webhook.key';key_file.write_bytes(webhook_key);key_file.chmod(0o600)
            deliveries=[];reject=[True]
            class Receiver(BaseHTTPRequestHandler):
                def log_message(self,*args):pass
                def do_POST(self):
                    raw=self.rfile.read(int(self.headers['Content-Length']))
                    assert len(raw)<=4096 and not self.headers.get('Authorization')
                    assert self.headers['X-Wpalt-Signature']=='sha256='+hmac.new(webhook_key,raw,hashlib.sha256).hexdigest()
                    event=json.loads(raw);assert self.headers['X-Wpalt-Event-ID']==event['id']
                    assert 'PRIVATE_EXTERNAL_DRAFT' not in raw.decode()
                    deliveries.append(event['id'])
                    status=503 if reject[0] else 204;reject[0]=False
                    self.send_response(status);self.end_headers()
            receiver=ThreadingHTTPServer(('127.0.0.1',0),Receiver)
            receiver_thread=threading.Thread(target=receiver.serve_forever,daemon=True);receiver_thread.start()
            checkpoint=root/'delivery.json'
            webhook=worker.with_name('webhook_worker.py')
            def deliver(ok=True):
                result=subprocess.run([sys.executable,str(webhook),'--site',origin,'--token-file',str(read_file),'--destination',f'http://127.0.0.1:{receiver.server_port}/events','--signing-key-file',str(key_file),'--checkpoint',str(checkpoint)],text=True,capture_output=True,timeout=40)
                assert (result.returncode==0)==ok,result.stderr
                assert read_token not in result.stdout+result.stderr
                return json.loads(result.stdout) if ok else None
            try:
                deliver(ok=False);assert not checkpoint.exists()
                result=deliver();assert deliveries[0]==deliveries[1]
                assert checkpoint.stat().st_mode&0o077==0
                while result['has_more']:result=deliver()
                before=len(deliveries);assert deliver()['delivered']==0;assert len(deliveries)==before
                request('/api/v1/events?after=foreign:0:start',status=409)
                cursor=json.loads(checkpoint.read_text())['next'];assert request('/api/v1/events?after='+cursor)['events']==[]
            finally:receiver.shutdown();receiver.server_close();receiver_thread.join(timeout=5)
            run('integration','revoke',writer['id'],ok=False) # Stopped-host lock remains enforced.
        finally:stop(process)
        run('integration','revoke',writer['id'])
        process=start(logfile)
        try:
            request('/api/v1/content',token=draft_token,status=403);request('/api/v1/content')
            assert request('/api/v1/events?after='+cursor)['events']==[]
        finally:stop(process)
        archive=root/'recovery.json';run('backup',archive)
        assert hashlib.sha256(read_token.encode()).hexdigest() not in archive.read_text()
        run('restore',archive,config_=target)
        process=start(logfile,target)
        try:request('/api/v1/content',status=403)
        finally:stop(process)
    journal=[json.loads(line) for line in (root/'site/privileged-audit.jsonl').read_text().splitlines()]
    assert any(event['actor'].endswith(':integration:'+writer['id']) and event['route']=='/api/v1/content' and event['phase']=='response' and event['status']==200 for event in journal)
    text=log.read_text()
    assert all(value not in text for value in [password,read_token,draft_token,'PRIVATE_EXTERNAL_DRAFT'])
    with sqlite3.connect(root/'site.db') as db:
        assert db.execute('SELECT version FROM schema_version').fetchone()[0]==14
        assert db.execute('SELECT COUNT(*) FROM posts').fetchone()[0]==len(seen)+2
print('PASS: native scoped credentials, bounded external pagination/drafts, conflict/publication/origin/cookie denial, stopped-host revocation, fresh recovery without delegated authority and redacted diagnostics')
