#!/usr/bin/env python3
"""Actual separately installed local inference -> private proposal -> native draft.

Requires an owner-operated Ollama endpoint and an already installed model; no
runtime/model auto-installation or cloud access occurs in this reference script.
"""
import argparse, hashlib, json, sqlite3, secrets, socket, subprocess, sys, tempfile, time, urllib.request
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'examples/integrations'))
from local_ai_worker import origin, request
p=argparse.ArgumentParser();p.add_argument('--binary',default='target/debug/wpalt');p.add_argument('--ollama',required=True);p.add_argument('--model',required=True);p.add_argument('--output',default='work/m8-local-ai-reference.json');args=p.parse_args()
binary=Path(args.binary).resolve();ai=origin(args.ollama,local=True)
engine=request(ai+'/api/version');models=request(ai+'/api/tags')['models'];model=next(m for m in models if m['name']==args.model)
with tempfile.TemporaryDirectory(prefix='wpalt-real-ai-') as temp:
    root=Path(temp)
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    site=f'http://127.0.0.1:{port}'
    cfg=root/'site.toml';cfg.write_text(f'database_url="sqlite://{root}/site.db?mode=rwc"\ndata_dir="{root}/data"\nbase_url="{site}"\nlisten="127.0.0.1:{port}"\n')
    password=secrets.token_urlsafe(24)
    def native(*argv,stdin=None):
        result=subprocess.run([str(binary),'--config',str(cfg),*map(str,argv)],input=stdin,text=True,capture_output=True,timeout=30)
        assert result.returncode==0,result.stderr
        assert password not in result.stdout+result.stderr
        return result
    native('init','--admin-email','owner@example.test',stdin=password+'\n')
    base=root/'base-theme.json';base.write_bytes((Path(__file__).resolve().parents[1]/'examples/themes/field-journal.json').read_bytes());base.chmod(0o600)
    native('theme','validate',base)
    native('seed-demo','--posts','1')
    token_file=root/'worker.token';native('integration','create','--user-email','owner@example.test','--name','Actual local inference','--draft',token_file)
    token=token_file.read_text();worker=Path(__file__).resolve().parents[1]/'examples/integrations/local_ai_worker.py'
    def external(*argv):
        result=subprocess.run([sys.executable,str(worker),'--site',site,'--token-file',str(token_file),*map(str,argv)],capture_output=True,text=True,timeout=40)
        assert result.returncode==0,result.stderr
        assert token not in result.stdout+result.stderr
        return json.loads(result.stdout)
    with (root/'server.log').open('w') as log:
        server=subprocess.Popen([str(binary),'--config',str(cfg),'serve'],stdout=log,stderr=log)
        try:
            deadline=time.monotonic()+10
            while time.monotonic()<deadline:
                assert server.poll() is None
                try:
                    with urllib.request.urlopen(site+'/health',timeout=1):break
                except OSError:time.sleep(.1)
            else:raise AssertionError('Native server not ready')
            source_body='The community garden opens on Saturday at 09:00. Admission is free. Visitors should bring a reusable bottle.'
            source=request(site+'/api/v1/content',token,{'title':'Community garden notice','slug':'source-notice','kind':'post','body':source_body,'import_markdown':True,'action':'save','version':0,'publish_at':0})
            proposal=root/'proposal.json';start=time.perf_counter()
            preview=external('suggest','--source',source['id'],'--slug','reviewed-proposal','--ollama',ai,'--model',args.model,'--output',proposal)
            generation_seconds=time.perf_counter()-start;artifact=json.loads(proposal.read_text())
            layout_proposal=root/'layout-proposal.json';layout_start=time.perf_counter()
            layout_preview=external('layout','--source',source['id'],'--base-theme',base,'--ollama',ai,'--model',args.model,'--output',layout_proposal)
            layout_seconds=time.perf_counter()-layout_start;layout_artifact=json.loads(layout_proposal.read_text())
            theme_file=root/'generated-theme.json';external('export-layout',layout_proposal,'--execute',layout_preview['plan'],'--output',theme_file)
            assert theme_file.stat().st_mode&0o077==0
            generated=artifact['input']['body'];assert generated.strip() and proposal.stat().st_mode&0o077==0
            applied=external('apply',proposal,'--execute',preview['plan']);assert applied['status']=='draft'
            draft=request(site+'/api/v1/content/'+applied['draft_id'],token)['content'];assert draft['status']=='draft' and not draft['published_body']
            current=request(site+'/api/v1/content/'+source['id'],token)['content'];assert current['version']==source['version'] and current['body']==source_body
            # Only synthetic sample text; retained for human quality review, not
            # described as a proof of general factual accuracy or model quality.
            report={'format':'wpalt-local-ai-reference-v1','status':'in-progress','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'engine':engine,'model':{'name':model['name'],'digest':model['digest'],'bytes':model['size'],'details':model.get('details')},'generation_seconds':generation_seconds,'layout_generation_seconds':layout_seconds,'layout_choices':layout_artifact['layout'],'theme_package_sha256':hashlib.sha256(theme_file.read_bytes()).hexdigest(),'metrics':artifact.get('model_metrics',{}),'source_body_sha256':hashlib.sha256(source_body.encode()).hexdigest(),'generated_body_sha256':hashlib.sha256(generated.encode()).hexdigest(),'source_bytes':len(source_body.encode()),'generated_bytes':len(generated.encode()),'synthetic_generated_sample':generated,'assertions':['actual installed model inference','private source-bound proposal','reviewed unscheduled draft','source unchanged','no public body or inferred access','actual bounded local layout inference and private native package export'],'boundary':'Single synthetic notice and one installed small model; bounded palette/font/width/listing choices on one independent base. No universal semantic quality, arbitrary theme generation, latency or CMS-only footprint claim.'}
            output=Path(args.output);output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n')

        finally:
            server.terminate();server.wait(timeout=10)
    native('theme','validate',theme_file)
    with sqlite3.connect(root/'site.db') as db:before=db.execute('SELECT COUNT(*) FROM themes').fetchone()[0]
    native('theme','import','local-proposal',theme_file)
    with sqlite3.connect(root/'site.db') as db:
        draft=db.execute("SELECT live,published_version FROM themes WHERE id='local-proposal'").fetchone();assert draft==('',0)
        assert db.execute('SELECT COUNT(*) FROM themes').fetchone()[0]==before+1
    native('theme','publish','local-proposal');native('theme','activate','local-proposal')
    with (root/'theme-server.log').open('w') as log:
        server=subprocess.Popen([str(binary),'--config',str(cfg),'serve'],stdout=log,stderr=log)
        try:
            deadline=time.monotonic()+10
            while time.monotonic()<deadline:
                try:
                    with urllib.request.urlopen(site+'/',timeout=1) as response:html=response.read().decode();break
                except OSError:time.sleep(.1)
            else:raise AssertionError('Generated theme did not render')
            assert 'local-proposal' in html and 'Your content, your server.' in html
            assert source_body not in html and 'Community garden notice' not in html
        finally:server.terminate();server.wait(timeout=10)
    report['status']='passed'
    report['assertions']+=['native validation without theme mutation','import retained as unpublished draft','explicit native publication/activation','independent generated theme renders without private article leakage']
    output.write_text(json.dumps(report,indent=2)+'\n')
    print('PASS: actual local content/layout inference, private proposals, native validation and independent theme; model '+args.model)
