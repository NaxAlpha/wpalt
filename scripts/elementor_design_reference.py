#!/usr/bin/env python3
"""Current free-plugin export/render -> reviewed native draft. Linux Docker gate."""
import argparse, hashlib, json, secrets, socket, sqlite3, subprocess, tempfile, time, urllib.request, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--output',default='work/d05-elementor-reference.json');args=p.parse_args()
binary=Path(args.binary).resolve();nonce=uuid.uuid4().hex[:12]
network,database,site,volume=[f'wpalt-d05-{name}-{nonce}' for name in ('net','db','site','files')]
secret,password=secrets.token_hex(24),secrets.token_hex(24)
images=['wordpress:7.1.3-php8.3-apache','wordpress:cli-php8.3','mariadb:11.4']
def run(*argv,stdin=None,ok=True):
 r=subprocess.run(argv,input=stdin,text=True,capture_output=True,timeout=240)
 if ok and r.returncode:raise RuntimeError(r.stderr.replace(secret,'[redacted]').replace(password,'[redacted]')[-2000:])
 return r
params=['-e','WORDPRESS_DB_HOST='+database,'-e','WORDPRESS_DB_USER=wpalt','-e','WORDPRESS_DB_PASSWORD='+secret,'-e','WORDPRESS_DB_NAME=wpalt']
def wp(*argv):return run('docker','run','--rm','--network',network,'--user','33:33','-v',volume+':/var/www/html',*params,images[1],'php','-d','memory_limit=512M','/usr/local/bin/wp',*argv).stdout.strip()
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
client=urllib.request.build_opener(urllib.request.ProxyHandler({}))
with tempfile.TemporaryDirectory(prefix='wpalt-d05-reference-') as temporary:
 root=Path(temporary);server=None;log=None
 report={'format':'wpalt-d05-elementor-reference-v1','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'status':'incomplete','scope':'Actual free plugin-saved legacy/atomic documents and public rendering; document.get_export_data content/settings wrapped in documented 0.4 JSON. Native review/private apply/publication/recovery. Selected fixtures do not prove complete widget/control or visual equivalence.','comparisons':[]}
 try:
  for image in images:run('docker','pull',image)
  report['images']={image:json.loads(run('docker','image','inspect',image).stdout)[0]['RepoDigests'] for image in images}
  run('docker','network','create',network);run('docker','volume','create',volume)
  run('docker','run','-d','--name',database,'--network',network,'-e','MARIADB_ROOT_PASSWORD='+secret,'-e','MARIADB_DATABASE=wpalt','-e','MARIADB_USER=wpalt','-e','MARIADB_PASSWORD='+secret,images[2])
  for _ in range(120):
   if run('docker','exec',database,'mariadb-admin','ping','--silent',ok=False).returncode==0:break
   time.sleep(.5)
  else:raise RuntimeError('Reference database not ready')
  origin=f'http://127.0.0.1:{port()}'
  run('docker','run','-d','--name',site,'--network',network,'-p',origin.removeprefix('http://')+':80','-v',volume+':/var/www/html',*params,images[0])
  for _ in range(120):
   try:
    with client.open(origin,timeout=1) as response:
     if response.status==200:break
   except OSError:pass
   time.sleep(.5)
  else:raise RuntimeError('Reference application not ready')
  wp('core','install','--url='+origin,'--title=Design reference','--admin_user=owner','--admin_password='+password,'--admin_email=owner@example.test','--skip-email')
  report['wordpress_version']=wp('core','version');assert report['wordpress_version']=='7.1.3'
  wp('plugin','install','elementor','--version=4.3.4','--activate');wp('plugin','verify-checksums','elementor','--strict')
  report['elementor_version']=wp('plugin','get','elementor','--field=version');assert report['elementor_version']=='4.3.4'
  wp('option','update','elementor_experiment-e_atomic_elements','active')
  run('docker','cp',str(ROOT/'scripts/reference/d05-seed.php'),site+':/var/www/html/wpalt-d05-source.php')
  report['source_runtime']=json.loads(wp('eval-file','/var/www/html/wpalt-d05-source.php'))
  native_origin=f'http://127.0.0.1:{port()}';cfg=root/'site.toml'
  cfg.write_text(f'database_url="sqlite://{root}/site.db?mode=rwc"\ndata_dir="{root}/data"\nbase_url="{native_origin}"\nlisten="{native_origin.removeprefix("http://")}"\ndebug=true\n')
  def native(*argv,ok=True,stdin=None):return run(str(binary),'--config',str(cfg),*map(str,argv),stdin=stdin,ok=ok)
  native('init','--admin-email','owner@example.test',stdin=password+'\n');native('seed-demo')
  for name in ('legacy','atomic'):
   export=root/(name+'.json');run('docker','cp',site+':/var/www/html/wpalt-d05-'+name+'.json',str(export));source=json.loads(export.read_text())
   with client.open(report['source_runtime']['documents'][name]['url'],timeout=10) as response:reference_html=response.read().decode()
   markers=[f'D05_{name.upper()}_{suffix}' for suffix in ('HEADING','BODY','LINK')]
   assert all(marker in reference_html for marker in markers),'Actual free source must render all comparison markers'
   request_file=root/(name+'-request.json');request_file.write_text(json.dumps({'source':source,'component':'reference-'+name,'target':'home'}));plan=root/(name+'-review.json')
   native('theme','elementor-review','paper',request_file,plan);review=json.loads(plan.read_text());assert review['report']['elements']>=4
   assert native('theme','elementor-apply','paper',plan,ok=False).returncode!=0,'Loss acknowledgement required'
   native('theme','elementor-apply','paper',plan,'--acknowledge-losses')
   assert native('theme','elementor-apply','paper',plan,'--acknowledge-losses',ok=False).returncode!=0,'Applied exact review must be stale'
   with sqlite3.connect(root/'site.db') as db:
    live,draft=db.execute("SELECT live,draft FROM themes WHERE id='paper'").fetchone();assert markers[0] in draft and markers[0] not in live
   native('theme','publish','paper');native('theme','activate','paper')
   log=(root/(name+'-native.log')).open('w');server=subprocess.Popen([str(binary),'--config',str(cfg),'serve'],stdout=log,stderr=log)
   for _ in range(100):
    try:
     with client.open(native_origin,timeout=1) as response:native_html=response.read().decode()
     break
    except OSError:time.sleep(.1)
   else:raise RuntimeError('Native reference server not ready')
   assert all(marker in native_html for marker in markers)
   assert native_html.index(markers[0])<native_html.index(markers[1])<native_html.index(markers[2]);assert '/local-path' in native_html
   server.terminate();server.wait(timeout=10);server=None;log.close();log=None
   report['comparisons'].append({'name':name,'source_export_sha256':hashlib.sha256(export.read_bytes()).hexdigest(),'source_html_sha256':hashlib.sha256(reference_html.encode()).hexdigest(),'native_html_sha256':hashlib.sha256(native_html.encode()).hexdigest(),'elements':review['report']['elements'],'losses':review['report']['losses'],'ordered_markers_and_local_link':True,'private_then_published':True,'stale_review_refused':True,'differences':'Native layout/typography/Markdown; source kit, raw atomic styles, editor metadata/defaults and unknown controls are reported. Content behavior does not prove pixel or complete-widget parity.'})
  archive=root/'recovery.json';native('backup',archive)
  fresh=root/'fresh.toml';fresh.write_text(f'database_url="sqlite://{root}/fresh.db?mode=rwc"\ndata_dir="{root}/fresh"\nbase_url="http://127.0.0.1:18081"\n')
  run(str(binary),'--config',str(fresh),'restore',str(archive))
  with sqlite3.connect(root/'fresh.db') as db:
   live=db.execute("SELECT live FROM themes WHERE id='paper'").fetchone()[0];assert 'reference-atomic' in live and 'reference-legacy' in live
  report['fresh_recovery_preserved_components']=True;report['status']='passed'
 finally:
  if server:server.terminate();server.wait(timeout=10)
  if log:log.close()
  run('docker','rm','-f',site,database,ok=False);run('docker','volume','rm',volume,ok=False);run('docker','network','rm',network,ok=False)
  output=Path(args.output);output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'status':report['status'],'wordpress':report.get('wordpress_version'),'elementor':report.get('elementor_version'),'comparisons':len(report['comparisons'])}))
