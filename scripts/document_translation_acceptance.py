#!/usr/bin/env python3
"""Readable whole-document proposal journey; optional actual installed local model evidence."""
import argparse, hashlib, json, os, secrets, socket, sqlite3, subprocess, sys, tempfile, threading, time, urllib.error
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'examples/integrations'))
from local_ai_worker import request, canonical, digest
from document_translation_worker import proposal
parser=argparse.ArgumentParser();parser.add_argument('--binary',default='target/debug/wpalt');parser.add_argument('--ollama');parser.add_argument('--model',default='qwen3:1.7b');parser.add_argument('--output',default='work/d03-document-translation-reference.json');args=parser.parse_args()
binary=Path(args.binary).resolve()
# Synthetic publication, no private reference content. Facts recur in distinct contexts.
paragraphs=[
 'On Saturday 12 October, Garden ABC-123 opens at 09:30 and closes at 17:00. Entry is free. Bring your own refillable bottle. Children must remain with an adult. The northern gate is closed; enter through the south gate. The information desk is beside the greenhouse. This timetable applies only to the demonstration event, not to every future opening day. The gardener Maya answers questions at the desk and will explain the site map before visitors enter the growing beds.',
 'The seed workshop begins at 10:15 and has 24 places. Registration is required before entry to the workshop even though garden admission is free. Participants receive one paper envelope and three tomato seeds. The workshop does not promise a harvest. Keep the envelope dry, label it with the planting date, and ask the gardener if your growing space gets less than six hours of sunlight. Visitors who do not join the workshop may still walk around the public demonstration beds.',
 'The accessible path is 120 metres long and starts at the south gate. The path has benches at two marked resting points. Ask Maya for the accessible route rather than taking the steps beside the greenhouse. Dogs must stay on a lead. Assistance dogs are welcome. The map uses the identifier MAP-2026-A; this identifier must remain unchanged when the map description is translated. The walking route ends at the same south gate and does not require crossing the closed northern entrance.',
 'At 12:00, the community kitchen offers soup while supplies last. A bowl costs EUR 4.50; this is separate from free garden admission. Ingredients are listed at the kitchen counter. Tell staff about allergies before ordering. The kitchen cannot guarantee that every dish is free from cross contamination. Visitors may bring their own lunch and use the marked picnic area. Empty packaging belongs in the sorting bins, and reusable dishes must be returned to the counter after lunch.',
 'The rain procedure is specific: if staff announce heavy rain, the seed workshop moves to the covered greenhouse, but outdoor demonstrations stop. Garden entry does not become a paid ticket because of rain. Listen for staff instructions and keep the path clear. The event code RAIN-17 identifies this procedure and should not be translated into another code. The demonstration organisers will publish a separate update if the whole event is cancelled; this article does not itself announce cancellation.',
 'Volunteers meet Maya at 08:45, before public admission starts. Volunteer sign-in is at the greenhouse desk. Each volunteer checks the task list, receives gloves, and returns borrowed tools before leaving. Do not operate powered equipment without the designated supervisor. The code TOOL-204 belongs to the inventory and stays unchanged in translations. No reader is being granted permission to work merely by reading these instructions; the supervisor assigns each task in person.',
 'Photography is permitted in the public beds, but ask permission before photographing identifiable visitors. Do not photograph private registration sheets or children without guardian permission. The garden keeps no public list of attendees. Report a lost item at the information desk; staff describe the item privately to its owner. These instructions protect visitors while allowing ordinary pictures of plants. They are specific event instructions rather than a legal guarantee about every future use of a photograph.',
 'At closing time, volunteers check that borrowed tools are returned and public gates are secured. Visitors leave through the south gate by 17:00. Staff keep the accessible exit available during this check. The archive reference GARDEN-2026-10 remains attached to the event report. Tomorrow\'s work schedule is a separate document; do not infer another free event or another workshop from this closing routine. The final announcement thanks visitors and asks them to check the next dated notice before returning.',
 'Translations should preserve the distinction between free entry and paid soup, the need to register for the seed workshop, and the limits of the rain procedure. A machine-generated draft is reviewed before publication. Reviewers compare times, counts, codes, names, and the direction of the entrance route against this source. Links point to their original destinations, code samples stay unchanged, and the photographs and registration form retain their original references. The local application does not silently publish a generated translation.'
]
body='# Garden ABC-123 event guide\n\n'+'\n\n'.join(paragraphs)+'\n\nRead [the map](/garden-map?q=1).\n\n```rust\nlet reference = "DO_NOT_TRANSLATE_204";\n```\n\nUse `GARDEN_API_V1` in the integration example.'
assert len(body.encode())>4096
with tempfile.TemporaryDirectory(prefix='wpalt-canonical-translation-') as tmp:
 root=Path(tmp)
 with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
 site=f'http://127.0.0.1:{port}';config=root/'site.toml';config.write_text(f'database_url="sqlite://{root}/site.db?mode=rwc"\ndata_dir="{root}/data"\nbase_url="{site}"\nlisten="127.0.0.1:{port}"\n')
 def native(*argv,stdin=None):
  r=subprocess.run([str(binary),'--config',str(config),*map(str,argv)],input=stdin,text=True,capture_output=True);assert r.returncode==0,r.stderr;return r.stdout
 native('init','--admin-email','owner@example.test',stdin=secrets.token_urlsafe(24)+'\n')
 with sqlite3.connect(root/'site.db') as db:
  definition=json.loads(db.execute('SELECT definition FROM discovery_settings').fetchone()[0]);definition['languages'].append({'code':'fr','label':'Français','direction':'ltr','navigation':[],'search_label':'Rechercher'});db.execute('UPDATE discovery_settings SET definition=?,version=version+1',(json.dumps(definition),))
 tokenfile=root/'writer.token';native('integration','create','--user-email','owner@example.test','--name','Document translation','--draft',tokenfile);token=tokenfile.read_text()
 model_server=None;observed=[];fail_once=[False]
 if not args.ollama:
  class Model(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def reply(self,data):
    raw=json.dumps(data).encode();self.send_response(200);self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
   def do_GET(self):self.reply({'models':[{'name':args.model,'digest':'a'*64}]})
   def do_POST(self):
    if fail_once[0]:
     fail_once[0]=False;self.send_response(503);self.end_headers();return
    assert not self.headers.get('Authorization');data=json.loads(self.rfile.read(int(self.headers['Content-Length'])));observed.append(data);assert data['think'] is False and data['stream'] is False
    text=data['prompt'].split('\nText: ',1)[1];self.reply({'response':json.dumps({'text':'Traduction '+text}), 'done':True,'done_reason':'stop','eval_count':1})
  model_server=ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=model_server.serve_forever,daemon=True).start();ai=f'http://127.0.0.1:{model_server.server_port}'
 else:ai=args.ollama
 workspace=root/'proposal';worker=ROOT/'examples/integrations/document_translation_worker.py'
 def external(*argv,ok=True):
  r=subprocess.run([sys.executable,str(worker),'--site',site,'--token-file',str(tokenfile),*map(str,argv)],text=True,capture_output=True,timeout=600);assert (r.returncode==0)==ok,(argv,r.stderr);return r
 logfile=(root/'server.log').open('w');server=subprocess.Popen([str(binary),'--config',str(config),'serve'],stdout=logfile,stderr=logfile)
 try:
  for _ in range(100):
   try:request(site+'/health');break
   except Exception:time.sleep(.1)
  source=request(site+'/api/v1/content',token,data={'title':'Garden ABC-123 event guide','slug':'long-source','kind':'post','body':body,'import_markdown':True,'action':'save','version':0,'locale':'en','translation_group':'document-language-family'})
  original=request(site+'/api/v1/content/'+source['id'],token)['content']
  prepared=external('prepare','--source',source['id'],'--locale','fr','--slug','reviewed-long-french','--model',args.model,'--ollama',ai,'--workspace',workspace)
  start=time.perf_counter();external('resume',workspace,'--max-segments','2')
  partial=json.loads((workspace/'state.json').read_text());assert partial['title_ready'] and len(partial['completed'])==1
  external('apply',workspace,'--execute','0'*64,ok=False)
  first=dict(partial['completed'])
  if model_server:
   fail_once[0]=True;external('resume',workspace,ok=False)
   assert json.loads((workspace/'state.json').read_text())['completed']==first,'Model failure must retain completed work'
  external('resume',workspace)
  state=json.loads((workspace/'state.json').read_text());assert all(state['completed'][k]==v for k,v in first.items());assert len(state['completed'])==len(state['source']['segments'])
  inspected=json.loads(external('inspect',workspace).stdout);assert len(inspected['plan'])==64
  payload=proposal(state)
  def denied(payload,credential=token,extra=None,status=403):
   headers={'Authorization':'Bearer '+credential,'Content-Type':'application/json'};headers.update(extra or {})
   req=urllib.request.Request(site+'/api/v1/translations',data=canonical(payload),headers=headers)
   try:urllib.request.urlopen(req,timeout=10);raise AssertionError('Unsafe proposal admitted')
   except urllib.error.HTTPError as error:assert error.code==status,(error.code,status)
  denied(payload,credential='0'*64)
  denied(payload,extra={'Cookie':'wpalt_session=forged'})
  denied(payload,extra={'Origin':'https://foreign.example'})
  denied({**payload,'segments':[]},status=422)
  denied({**payload,'action':'publish'},status=422)
  assert (workspace/'state.json').stat().st_mode&0o077==0
  html=Path(inspected['review']).read_text();assert '<script' not in html and '<pre lang=' in html
  external('apply',workspace,'--execute','0'*64,ok=False)
  applied=json.loads(external('apply',workspace,'--execute',inspected['plan']).stdout);assert applied['status']=='draft'
  external('apply',workspace,'--execute',inspected['plan'],ok=False)
  draft=request(site+'/api/v1/content/'+applied['id'],token)['content'];assert draft['locale']=='fr' and draft['published_body']=='' and draft['publish_at']==0
  tree=json.loads(draft['document']);source_tree=json.loads(original['document'])
  def skeleton(node):
   if node['type']=='text':node.pop('text',None)
   for child in node.get('content',[]):skeleton(child)
  skeleton(tree['root']);skeleton(source_tree['root']);assert tree==source_tree
  assert 'DO_NOT_TRANSLATE_204' in draft['document'] and 'GARDEN_API_V1' in draft['document'] and '/garden-map?q=1' in draft['document']
  assert request(site+'/api/v1/content/'+source['id'],token)['content']==original
  try:request(site+'/fr/reviewed-long-french');raise AssertionError('Draft became public')
  except urllib.error.HTTPError as error:assert error.code==404
  generated='\n'.join(s['text'] for s in [{'text':state['title']},*[{'text':state['completed'][s['id']]} for s in state['source']['segments']]])
  facts=['ABC-123','09:30','17:00','10:15','24','120','MAP-2026-A','12:00','4.50','RAIN-17','08:45','TOOL-204','GARDEN-2026-10']
  missing=[fact for fact in facts if fact not in generated]
  output={'format':'wpalt-canonical-translation-reference-v1','actual_model':bool(args.ollama),'binary_sha256':digest(binary.read_bytes()),'model':{'name':args.model,'digest':state['model_digest']},'source_bytes':len(body.encode()),'source_segments':len(state['source']['segments']),'generation_seconds':time.perf_counter()-start,'model_metrics':state['metrics'],'missing_exact_fact_markers':missing,'synthetic_translated_title':state['title'],'synthetic_translated_segments':[{'id':s['id'],'source':s['text'],'translation':state['completed'][s['id']]} for s in state['source']['segments']],'assertions':['resumable checkpoints retained','incomplete and changed plans refused','source unchanged','reviewed private draft','canonical structure code and references preserved','duplicate retry refused','invalid credentials cookie foreign origin incomplete set and publication escalation refused'],'quality_boundary':'Exact marker preservation is a limited diagnostic, not fluent or factual translation certification. Human review remains mandatory.'}
  Path(args.output).parent.mkdir(parents=True,exist_ok=True);Path(args.output).write_text(json.dumps(output,ensure_ascii=False,indent=2));print(json.dumps({'pass':True,'actual_model':bool(args.ollama),'source_bytes':len(body.encode()),'segments':len(state['source']['segments']),'missing_exact_fact_markers':missing,'output':args.output}))
 finally:
  server.terminate();server.wait(timeout=15);logfile.close()
  if model_server:model_server.shutdown();model_server.server_close()
