#!/usr/bin/env python3
"""Owner-operated at-least-once delivery of the bounded native content-event feed.

Run once per invocation under your service scheduler. The CMS never runs this
process or fetches the destination. Receivers must deduplicate X-Wpalt-Event-ID.
"""
import argparse, fcntl, hashlib, hmac, json, os, stat, urllib.parse, urllib.request, uuid
from pathlib import Path
from urllib.parse import urlsplit
from local_ai_worker import NoRedirect, canonical, origin, read_private, request


def destination(value):
    parsed=urlsplit(value)
    if not parsed.hostname or parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ValueError('Choose an explicit destination without embedded credentials.')
    if parsed.scheme!='https' and not (parsed.scheme=='http' and parsed.hostname in ('127.0.0.1','::1')):
        raise ValueError('Use HTTPS delivery or literal loopback development.')
    return value


def checkpoint(path,state):
    temp=path.parent/('.wpalt-checkpoint-'+uuid.uuid4().hex)
    try:
        with os.fdopen(os.open(temp,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'wb') as file:
            file.write(canonical(state));file.flush();os.fsync(file.fileno())
        os.replace(temp,path)
        descriptor=os.open(path.parent,os.O_RDONLY|getattr(os,'O_DIRECTORY',0))
        try:os.fsync(descriptor)
        finally:os.close(descriptor)
    finally:
        temp.unlink(missing_ok=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site',required=True);parser.add_argument('--token-file',required=True)
    parser.add_argument('--destination',required=True);parser.add_argument('--signing-key-file',required=True)
    parser.add_argument('--checkpoint',required=True)
    parser.add_argument('--start-at',help='Explicit reconciled event cursor, only for a new checkpoint.')
    args=parser.parse_args();site=origin(args.site);target=destination(args.destination)
    token=read_private(args.token_file,128).decode().strip()
    if len(token)!=64 or any(c not in '0123456789abcdef' for c in token):raise ValueError('Invalid private integration credential.')
    key=read_private(args.signing_key_file,1024)
    if not 32<=len(key)<=1024:raise ValueError('Use at least 32 random signing-key bytes.')
    path=Path(args.checkpoint)
    if not path.parent.is_dir() or path.parent.stat().st_mode&0o077:
        raise ValueError('Use an existing private checkpoint directory (0700).')
    lock=os.open(str(path)+'.lock',os.O_RDWR|os.O_CREAT|getattr(os,'O_NOFOLLOW',0),0o600)
    with os.fdopen(lock,'rb') as handle:
        info=os.fstat(handle.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_mode&0o077:raise ValueError('Invalid checkpoint lock.')
        fcntl.flock(handle,fcntl.LOCK_EX|fcntl.LOCK_NB)
        state={'format':'wpalt-webhook-checkpoint-v1','site':site,'destination_sha256':hashlib.sha256(target.encode()).hexdigest(),'next':args.start_at or ''}
        if path.exists() or path.is_symlink():
            if args.start_at:raise ValueError('A start cursor cannot override existing state.')
            old=json.loads(read_private(path,4096))
            if any(old.get(k)!=state[k] for k in ('format','site','destination_sha256')):raise ValueError('Checkpoint source/destination changed; reconcile before creating new state.')
            state=old
        after=state['next']
        if not isinstance(after,str) or len(after)>100:raise ValueError('Invalid checkpoint cursor.')
        query='?after='+urllib.parse.quote(after,safe='') if after else ''
        feed=request(site+'/api/v1/events'+query,token)
        if feed.get('format')!='wpalt-event-feed-v1' or feed.get('source_origin')!=site or len(feed['events'])>25:
            raise ValueError('Invalid source feed.')
        opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),NoRedirect())
        delivered=0
        for event in feed['events']:
            raw=canonical(event)
            if len(raw)>4096 or event.get('format')!='wpalt-content-event-v1':raise ValueError('Invalid event.')
            event_id=event['id']
            parts=event_id.split(':')
            if len(parts)!=3 or parts[:2]!=[feed['epoch'],str(event['sequence'])] or str(uuid.UUID(parts[2]))!=parts[2]:raise ValueError('Invalid event identity.')
            headers={'Content-Type':'application/json','X-Wpalt-Event-ID':event_id,'X-Wpalt-Signature':'sha256='+hmac.new(key,raw,hashlib.sha256).hexdigest()}
            with opener.open(urllib.request.Request(target,data=raw,headers=headers),timeout=30) as response:
                if not 200<=response.status<300:raise ValueError('Delivery rejected.')
            state['next']=event_id;checkpoint(path,state);delivered+=1
        # Includes the empty initial journal: persisting its epoch detects resets.
        if not feed['events']:
            state['next']=feed['next'];checkpoint(path,state)
        print(json.dumps({'delivered':delivered,'has_more':feed['has_more'],'delivery':'at-least-once; receiver deduplication required'}))


if __name__=='__main__':
    try:main()
    except Exception:
        raise SystemExit('Delivery paused. Check source authority, replay gaps/reset, destination and checkpoint; reconcile receiver state before retrying. No credentials or endpoint details are logged.')
