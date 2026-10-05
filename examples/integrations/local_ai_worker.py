#!/usr/bin/env python3
"""Separately operated local Ollama worker: review a private proposal, then apply a draft.

No dependencies, cloud account, plugin loading, model tools or publication rights.
The CMS does not launch this process. Its token must explicitly grant private content.
"""
import argparse, hashlib, json, os, stat, urllib.request, uuid
from pathlib import Path
from urllib.parse import urlsplit

MAX_RESPONSE=2*1024*1024
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*args,**kwargs):
        raise ValueError('Redirect refused; credentials and source data stay at the selected endpoint.')

def origin(value,local=False):
    parsed=urlsplit(value)
    loopback=parsed.hostname in ('127.0.0.1','::1')
    if parsed.username or parsed.password or parsed.query or parsed.fragment or parsed.path not in ('','/'):
        raise ValueError('Choose an origin without credentials, path, query or fragment.')
    if parsed.scheme not in ('http','https') or not parsed.hostname or (parsed.scheme=='http' and not loopback) or (local and not loopback):
        raise ValueError('Use HTTPS for the CMS; local AI must use a literal loopback address.')
    return value.rstrip('/')

def read_private(path,limit):
    flags=os.O_RDONLY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_NONBLOCK',0)
    descriptor=os.open(path,flags)
    with os.fdopen(descriptor,'rb') as file:
        info=os.fstat(file.fileno())
        if not stat.S_ISREG(info.st_mode) or (os.name=='posix' and info.st_mode&0o077):
            raise ValueError('Use a private regular file (permissions 0600).')
        value=file.read(limit+1)
    if len(value)>limit:raise ValueError('Private input exceeds its budget.')
    return value

def request(url,token=None,data=None):
    headers={}
    if token:headers['Authorization']='Bearer '+token
    encoded=None
    if data is not None:
        headers['Content-Type']='application/json';encoded=canonical(data)
    # Ignore ambient proxies for explicitly selected local AI/CMS transport.
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),NoRedirect())
    with opener.open(urllib.request.Request(url,headers=headers,data=encoded),timeout=30) as response:
        raw=response.read(MAX_RESPONSE+1)
    if len(raw)>MAX_RESPONSE:raise ValueError('Endpoint response exceeds its budget.')
    return json.loads(raw)

def canonical(value):return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()
def digest(raw):return hashlib.sha256(raw).hexdigest()
def write_private(path,raw):
    with os.fdopen(os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'wb') as file:
        file.write(raw);file.flush();os.fsync(file.fileno())

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site',required=True);parser.add_argument('--token-file',required=True)
    commands=parser.add_subparsers(dest='command',required=True)
    suggest=commands.add_parser('suggest');suggest.add_argument('--source',required=True);suggest.add_argument('--slug',required=True)
    suggest.add_argument('--ollama',default='http://127.0.0.1:11434');suggest.add_argument('--model',required=True);suggest.add_argument('--output',required=True)
    apply=commands.add_parser('apply');apply.add_argument('input');apply.add_argument('--execute',required=True)
    args=parser.parse_args();site=origin(args.site)
    token=read_private(args.token_file,128).decode().strip()
    if len(token)!=64 or any(c not in '0123456789abcdef' for c in token):raise ValueError('Use an owner-issued opaque credential file.')
    if args.command=='suggest':
        source_id=str(uuid.UUID(args.source));ai=origin(args.ollama,local=True)
        if not args.model or len(args.model)>100 or any(ord(c)<32 for c in args.model):raise ValueError('Choose a bounded installed local model name.')
        if not args.slug or len(args.slug)>120 or any(c not in 'abcdefghijklmnopqrstuvwxyz0123456789-' for c in args.slug):raise ValueError('Choose a new lowercase native draft slug.')
        source=request(site+'/api/v1/content/'+source_id,token)['content']
        body=source['body']
        if not isinstance(body,str) or len(body.encode())>512*1024:raise ValueError('Source body exceeds its budget.')
        generated=request(ai+'/api/generate',data={'model':args.model,'stream':False,'options':{'num_predict':2048},'prompt':'Propose a clearer edit of the supplied article. Preserve facts. Return Markdown only. You have no tools or publication authority. Article follows:\n'+body})
        text=generated.get('response')
        if not isinstance(text,str) or not text.strip() or len(text.encode())>512*1024:raise ValueError('Local model returned invalid or oversized text.')
        title=source['title'].encode()[:250].decode(errors='ignore')+' · Suggested edit'
        artifact={'format':'wpalt-local-ai-proposal-v1','site':site,'source_id':source_id,'source_version':source['version'],'source_body_sha256':digest(body.encode()),'model':args.model,
                  'input':{'title':title,'slug':args.slug,'kind':'post','body':text,'import_markdown':True,'action':'save','publish_at':0,'version':0}}
        encoded=canonical(artifact);write_private(args.output,encoded)
        print(json.dumps({'plan':digest(encoded),'proposal_written':True,'publication':'Owner review required in native editor.'}))
    else:
        raw=read_private(args.input,1024*1024);artifact=json.loads(raw)
        if digest(canonical(artifact))!=args.execute or artifact.get('format')!='wpalt-local-ai-proposal-v1' or artifact.get('site')!=site:
            raise ValueError('Proposal or site changed; review the exact current proposal plan.')
        source_id=str(uuid.UUID(artifact['source_id']))
        current=request(site+'/api/v1/content/'+source_id,token)['content']
        if current['version']!=artifact['source_version'] or digest(current['body'].encode())!=artifact['source_body_sha256']:
            raise ValueError('Source changed; generate and review a new proposal.')
        payload=artifact['input']
        if payload.get('action')!='save' or payload.get('publish_at')!=0 or payload.get('version')!=0:
            raise ValueError('This worker only creates a new unscheduled draft.')
        result=request(site+'/api/v1/content',token,data=payload)
        print(json.dumps({'draft_id':result['id'],'version':result['version'],'status':result['status']}))
if __name__=='__main__':
    try:main()
    except Exception:
        # Transport diagnostics can contain private endpoint or source details.
        raise SystemExit('Worker failed. Review endpoints, private input, source version and credential scope; inspect native state before retrying.')
