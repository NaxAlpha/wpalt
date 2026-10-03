#!/usr/bin/env python3
"""A real install -> serve -> schedule/restart -> offline backup -> fresh restore journey.
Uses temporary SQLite sites and generated private credentials; prints no secrets.
"""
import argparse, http.cookiejar, json, os, secrets, socket, sqlite3, subprocess, tempfile, time, uuid
import urllib.request, urllib.error, xml.etree.ElementTree as ET
from html.parser import HTMLParser
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('--binary',default='target/debug/wpalt')
args=parser.parse_args()
binary=Path(args.binary).resolve()

class Inputs(HTMLParser):
    def __init__(self): super().__init__(); self.fields={}
    def handle_starttag(self,tag,attributes):
        d=dict(attributes)
        if tag=='input' and d.get('name'): self.fields[d['name']]=d.get('value','')

def port():
    with socket.socket() as s: s.bind(('127.0.0.1',0)); return s.getsockname()[1]

def run(config,*arguments,password=None,ok=True,env=None):
    result=subprocess.run([str(binary),'--config',str(config),*arguments],input=password,text=True,capture_output=True,env=env)
    if ok: assert result.returncode==0,(arguments,result.stderr)
    else: assert result.returncode!=0,arguments
    return result

def wait_ready(origin,process):
    deadline=time.monotonic()+10
    while time.monotonic()<deadline:
        assert process.poll() is None,'Server exited before becoming ready'
        try:
            with urllib.request.urlopen(origin+'/health',timeout=1) as r:
                if r.status==200:return
        except (urllib.error.URLError,TimeoutError): pass
        time.sleep(.1)
    raise AssertionError('Server did not become ready')

def stop(process):
    if process.poll() is None: process.terminate()
    process.wait(timeout=10)
    assert process.returncode==0,'Graceful shutdown failed'

with tempfile.TemporaryDirectory(prefix='wpalt-cli-') as temporary:
    root=Path(temporary);origin=f'http://127.0.0.1:{port()}';secret=secrets.token_urlsafe(24)
    def config_for(name):
        cfg=root/(name+'.toml');cfg.write_text(f'database_url = "sqlite://{root/name}.db?mode=rwc"\ndata_dir = "{root/name}"\nbase_url = "{origin}"\nlisten = "{origin.split("://")[1]}"\nscheduler_seconds = 1\ndebug = true\n')
        return cfg
    config=config_for('source');target=config_for('recovered')
    effective=json.loads(run(config,'config').stdout)
    assert effective['base_url']==origin
    env=os.environ.copy();env['WPALT_DEBUG']='false';assert not json.loads(run(config,'config',env=env).stdout)['debug']
    assert json.loads(run(config,'--debug','config',env=env).stdout)['debug']
    run(config,'init','--admin-email','owner@example.test',password=secret+'\n')
    run(config,'init','--admin-email','owner@example.test',password=secret+'\n',ok=False)
    run(config,'seed-demo','--posts','3')
    # Portable themes use the same validator and explicit publication as the studio.
    package_file=root/'paper.json';run(config,'theme','export','paper',str(package_file),'--draft')
    package=json.loads(package_file.read_text());assert package['format']==1
    package['name']='Local variant';package['tokens']['accent']='#6c3ce6'
    imported=root/'variant.json';imported.write_text(json.dumps(package))
    run(config,'theme','import','variant',str(imported))
    run(config,'theme','activate','variant',ok=False)
    run(config,'theme','publish','variant');run(config,'theme','activate','variant')
    exported=root/'variant-live.json';run(config,'theme','export','variant',str(exported))
    assert json.loads(exported.read_text())['tokens']['accent']=='#6c3ce6'
    assert exported.stat().st_mode & 0o077 == 0,'Theme exports must be private'
    run(config,'theme','activate','paper')

    log=root/'server.log'
    with log.open('w') as log_file:
        process=subprocess.Popen([str(binary),'--config',str(config),'serve'],stdout=log_file,stderr=log_file)
        try:
            wait_ready(origin,process)
            # Offline commands cannot race the running server.
            assert 'already in use' in run(config,'backup',str(root/'should-not-exist.json'),ok=False).stderr
            jar=http.cookiejar.CookieJar();client=urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
            from urllib.parse import urlencode
            req=urllib.request.Request(origin+'/login',data=urlencode({'email':'owner@example.test','password':secret}).encode(),headers={'Origin':origin})
            with client.open(req) as r: page=r.read().decode()
            parsed=Inputs();parsed.feed(page);csrf=parsed.fields['csrf']
            content={'title':'Scheduled after restart','slug':'scheduled-restart','kind':'post','body':'DURABLE_SCHEDULE','fields':'{}','blocks':'[]','action':'schedule','publish_at':int(time.time())+3}
            req=urllib.request.Request(origin+'/api/admin/content',data=json.dumps(content).encode(),headers={'Origin':origin,'Content-Type':'application/json','X-CSRF-Token':csrf})
            with client.open(req) as r: assert json.load(r)['status']=='scheduled'
            with urllib.request.urlopen(origin+'/feed.xml') as r: assert ET.fromstring(r.read()).tag=='rss'
            stop(process)
            process=subprocess.Popen([str(binary),'--config',str(config),'serve'],stdout=log_file,stderr=log_file)
            wait_ready(origin,process)
            deadline=time.monotonic()+10
            while True:
                try:
                    with urllib.request.urlopen(origin+'/scheduled-restart') as r: assert b'DURABLE_SCHEDULE' in r.read();break
                except urllib.error.HTTPError as e:
                    assert e.code==404
                    assert time.monotonic()<deadline,'Restarted scheduler did not publish'
                    time.sleep(.1)
            stop(process)
        finally:
            if process.poll() is None:stop(process)
    # Offline membership automation is the same workflow and graph as native UI.
    policy=run(config,'member','policy','--title','CLI academy','--entitlement','cli-academy').stdout.strip()
    uuid.UUID(policy)
    grant=run(config,'member','grant','--email','owner@example.test','--entitlement','cli-academy').stdout.strip()
    run(config,'member','revoke',grant)
    with sqlite3.connect(root/'source.db') as database:
        lesson=database.execute("SELECT id FROM posts WHERE published_slug='journal-3'").fetchone()[0]
        protected=database.execute("SELECT id FROM posts WHERE published_slug='journal-2'").fetchone()[0]
    run(config,'member','protect','--kind','post','--resource',protected,'--policy',policy)
    course={'title':'CLI course','policy_id':policy,'sequential':True,'lessons':[{'id':str(uuid.uuid4()),'title':'CLI lesson','post_id':lesson,'downloads':[],'delay_seconds':0,'opens_at':0,'questions':[],'assignment':'','pass_percent':70,'max_attempts':3}]}
    course_file=root/'course.json';course_file.write_text(json.dumps(course))
    course_id=run(config,'member','course-import',str(course_file),'--publish').stdout.strip()
    exported_course=root/'course-export.json';run(config,'member','course-export',course_id,str(exported_course))
    assert json.loads(exported_course.read_text())==course
    assert exported_course.stat().st_mode & 0o077==0,'Course exports must be private'
    snapshot=root/'snapshot.json';run(config,'backup',str(snapshot));assert snapshot.exists()
    if os.name=='posix':assert snapshot.stat().st_mode & 0o077==0,'Backup permissions expose private data'
    run(target,'restore',str(snapshot));run(target,'restore',str(snapshot),ok=False)
    restored_course=root/'restored-course.json';run(target,'member','course-export',course_id,str(restored_course))
    assert json.loads(restored_course.read_text())==course
    with log.open('a') as log_file:
        process=subprocess.Popen([str(binary),'--config',str(target),'serve'],stdout=log_file,stderr=log_file)
        try:
            wait_ready(origin,process)
            with urllib.request.urlopen(origin+'/scheduled-restart') as r:assert b'DURABLE_SCHEDULE' in r.read()
            with urllib.request.urlopen(origin+'/journal-1') as r:assert r.status==200
        finally:stop(process)
    logs=log.read_text();assert secret not in logs;assert csrf not in logs
    for cookie in jar:assert cookie.value not in logs
    assert 'request_completed' in logs and 'elapsed_us' in logs and 'login_succeeded' in logs
    print('PASS: configuration precedence, initialization, process lock, real login, persisted scheduler/restart, RSS XML, portable theme import/export/publication, private backup, fresh restore and redacted debug logs.')
