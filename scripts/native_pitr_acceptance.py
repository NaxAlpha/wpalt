#!/usr/bin/env python3
"""Real PostgreSQL 17 base-backup -> encrypted native WAL -> named-point recovery.
Uses the clean Linux executable inside a pinned official PostgreSQL image. No
source cluster is available during recovery. Fixtures and credentials are private.
"""
import argparse, hashlib, json, os, secrets, shutil, socket, subprocess, time, urllib.request, uuid
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('--binary',required=True)
parser.add_argument('--output',default='work/native-pitr-result.json')
args=parser.parse_args()
binary=Path(args.binary).resolve()
image='postgres:17-trixie@sha256:d74eeac9a635390a49bc21bd49fccd973de707e2a53a76ac49b552b8712ec46f'
root=Path('work/native-pitr-'+uuid.uuid4().hex).resolve();root.mkdir(mode=0o700)
engine=root/'engine';engine.mkdir(mode=0o755)
archive=engine/'archive';archive.mkdir(mode=0o700)
secret=secrets.token_hex(24);owner_password=secrets.token_hex(24)
source='wpalt-pitr-source-'+uuid.uuid4().hex[:12]
target='wpalt-pitr-target-'+uuid.uuid4().hex[:12]
started=time.monotonic();application=None

def run(*arguments,input=None,ok=True):
    result=subprocess.run(arguments,input=input,text=True,capture_output=True,timeout=180)
    if ok and result.returncode:
        raise RuntimeError(result.stderr.replace(secret,'[redacted]').replace(owner_password,'[redacted]')[-2000:])
    return result

def sql(container,statement):
    return run('docker','exec','-i','-e','PGPASSWORD='+secret,container,'psql','-h','127.0.0.1','-U','postgres','-d','postgres','-At','-v','ON_ERROR_STOP=1',input=statement).stdout.strip()

def ready(container):
    for _ in range(300):
        result=run('docker','exec',container,'pg_isready','-h','127.0.0.1','-U','postgres',ok=False)
        if result.returncode==0:return
        time.sleep(.2)
    raise RuntimeError('Native PostgreSQL fixture did not become ready.')

def port(container):
    state=json.loads(run('docker','inspect',container).stdout)[0]
    return int(state['NetworkSettings']['Ports']['5432/tcp'][0]['HostPort'])

def app_config(name,container):
    with socket.socket() as sock:sock.bind(('127.0.0.1',0));http_port=sock.getsockname()[1]
    path=root/(name+'.toml')
    path.write_text(f'database_url="postgres://postgres:{secret}@127.0.0.1:{port(container)}/postgres"\ndata_dir="{root/name}"\nlisten="127.0.0.1:{http_port}"\nbase_url="http://127.0.0.1:{http_port}"\n')
    path.chmod(0o600)
    return path,http_port

try:
    run('docker','pull',image)
    run('docker','run','-d','--name',source,'--memory','512m','--cpus','2','-e','POSTGRES_PASSWORD='+secret,'-p','127.0.0.1::5432','-v',str(engine)+':/recovery','-v',str(binary)+':/usr/local/bin/wpalt:ro',image,'postgres','-c','shared_buffers=32MB','-c','max_connections=20','-c','wal_level=replica','-c','archive_mode=on','-c','archive_command=wpalt --config /recovery/native.toml wal-store %p %f')
    ready(source)
    system_id=sql(source,'SELECT system_identifier::text FROM pg_control_system();')
    native=engine/'native.toml'
    # The unreachable application URL proves that helpers never connect to it.
    native.write_text(f'database_url="postgres://unavailable@127.0.0.1:1/no_application"\ndata_dir="/tmp/never-opened-app"\n[postgres_archive]\nenabled=true\nsystem_id="{system_id}"\nkey_file="/recovery/key"\ndirectory="/recovery/archive"\nmax_segment_bytes=16777216\n')
    native.chmod(0o600)
    run(str(binary),'recovery-key',str(engine/'key'))
    # Official-image user ownership, not a hardcoded numeric UID.
    run('docker','exec','-u','root',source,'chown','-R','postgres:postgres','/recovery')
    config,_=app_config('source-site',source)
    run(str(binary),'--config',str(config),'init','--admin-email','owner@example.test',input=owner_password+'\n')
    run(str(binary),'--config',str(config),'seed-demo','--posts','3')
    run(str(binary),'--config',str(config),'shop','seed-demo')
    products=json.loads(run(str(binary),'--config',str(config),'shop','report').stdout)['products']
    run('docker','exec','-u','postgres','-e','PGPASSWORD='+secret,source,'pg_basebackup','-h','127.0.0.1','-U','postgres','-D','/recovery/base','-X','stream','-c','fast','--no-password')
    run('docker','exec','-u','postgres',source,'pg_verifybackup','/recovery/base')
    sql(source,"UPDATE posts SET title='Recovered midpoint',published_title='Recovered midpoint' WHERE slug='journal-1';\nSELECT pg_create_restore_point('wpalt_midpoint');\nUPDATE posts SET title='Must not survive',published_title='Must not survive' WHERE slug='journal-1';\nSELECT pg_switch_wal();\n")
    assert sql(source,"SELECT published_title FROM posts WHERE slug='journal-1';")=='Must not survive'
    for _ in range(300):
        outstanding=sql(source,"SELECT COUNT(*) FROM pg_ls_dir('pg_wal/archive_status') AS name WHERE name LIKE '%.ready';")
        if outstanding=='0':break
        time.sleep(.2)
    else:raise RuntimeError('Native archiver did not finish its WAL copies.')
    archived=int(sql(source,'SELECT archived_count FROM pg_stat_archiver;'))
    assert archived>0,'No real native archive_command was executed'
    restored=root/'restored-engine'
    run('docker','cp',source+':/recovery/base',str(restored))
    # Docker cp yields caller-owned local files. Preserve the physical source,
    # add only recovery instructions to the separate destination.
    with (restored/'postgresql.auto.conf').open('a') as settings:
        settings.write("\nrestore_command = 'wpalt --config /recovery/native.toml wal-restore %f %p'\nrecovery_target_name = 'wpalt_midpoint'\nrecovery_target_action = 'promote'\narchive_mode = 'off'\n")
    (restored/'recovery.signal').touch()
    run('docker','rm','-f','-v',source)
    assert run('docker','inspect',source,ok=False).returncode!=0,'Source cluster must be unavailable'
    run('docker','run','-d','--name',target,'--memory','512m','--cpus','2','-e','POSTGRES_PASSWORD='+secret,'-p','127.0.0.1::5432','-v',str(restored)+':/var/lib/postgresql/data','-v',str(engine)+':/recovery','-v',str(binary)+':/usr/local/bin/wpalt:ro',image,'postgres','-c','shared_buffers=32MB')
    ready(target)
    for _ in range(300):
        if sql(target,'SELECT pg_is_in_recovery();')=='f':break
        time.sleep(.2)
    else:raise RuntimeError('Native named-point recovery did not promote at its target.')
    assert sql(target,"SELECT published_title FROM posts WHERE slug='journal-1';")=='Recovered midpoint'
    assert sql(target,'SELECT COUNT(*) FROM users;')=='1'
    assert sql(target,'SELECT pg_is_in_recovery();')=='f'
    recovered,http_port=app_config('recovered-site',target)
    assert json.loads(run(str(binary),'--config',str(recovered),'shop','report').stdout)['products']==products
    log=root/'application.log'
    with log.open('w') as stream:
        application=subprocess.Popen([str(binary),'--config',str(recovered),'serve'],stdout=stream,stderr=stream)
        for _ in range(200):
            try:
                with urllib.request.urlopen(f'http://127.0.0.1:{http_port}/journal-1',timeout=1) as response:
                    page=response.read().decode();assert 'Recovered midpoint' in page and 'Must not survive' not in page;break
            except OSError:time.sleep(.1)
        else:raise RuntimeError('Recovered wpalt did not serve its midpoint publication.')
        application.terminate();application.wait(timeout=10);application=None
    report={'schema':1,'image':image,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'engine':'PostgreSQL 17','base_verified':True,'archive_command_segments':archived,'source_unavailable':True,'named_target':'wpalt_midpoint','midpoint_publication_served':True,'later_mutation_excluded':True,'local_owner_preserved':True,'catalog_products_preserved':products,'elapsed_seconds':round(time.monotonic()-started,3),'limits':['Physical cluster base is a private native directory, not an encrypted wpalt logical package. Protect it independently.','Fixture has no media originals or paid orders; logical fresh-recovery journeys cover those separately.','Engine version/architecture and matching immutable media are operator-managed physical-recovery prerequisites.']}
    Path(args.output).parent.mkdir(parents=True,exist_ok=True);Path(args.output).write_text(json.dumps(report,indent=2)+'\n')
    print('PASS: verified native base backup, encrypted real WAL archiving, original cluster unavailable, named-point recovery and application delivery.')
finally:
    if application:
        application.terminate();application.wait(timeout=10)
    for container in [source,target]:
        run('docker','rm','-f','-v',container,ok=False)
    # Retain no cluster databases, passwords, keys or private application fixtures.
    if root.exists():
        run('docker','run','--rm','-v',str(root)+':/cleanup','--entrypoint','find',image,'/cleanup','-mindepth','1','-delete',ok=False)
        shutil.rmtree(root,ignore_errors=True)
