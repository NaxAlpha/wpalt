#!/usr/bin/env python3
"""Actual pinned free Redirection execution; native behavior and deliberate limits."""
import argparse, hashlib, http.cookiejar, json, os, re, secrets, socket, subprocess, tempfile, time, urllib.error, urllib.parse, urllib.request, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--native-only',action='store_true',help='Verify native fixture only; explicitly does not execute WordPress/plugin');p.add_argument('--output',default='work/d08-redirection-reference.json');args=p.parse_args()
binary=Path(args.binary).resolve();nonce=uuid.uuid4().hex[:12]
network,database,site,volume=[f'wpalt-d08-{name}-{nonce}' for name in ('net','db','site','files')]
secret,password=secrets.token_hex(24),secrets.token_hex(24)
images=['wordpress:7.1.3-php8.3-apache','wordpress:cli-php8.3','mariadb:11.4']
def run(*argv,stdin=None,ok=True):
 r=subprocess.run(argv,input=stdin,text=True,capture_output=True,timeout=240)
 if ok and r.returncode:
  diagnostic=r.stderr.replace(secret,'[redacted]').replace(password,'[redacted]')
  raise RuntimeError(diagnostic[:4000])
 return r
params=['-e','WORDPRESS_DB_HOST='+database,'-e','WORDPRESS_DB_USER=wpalt','-e','WORDPRESS_DB_PASSWORD='+secret,'-e','WORDPRESS_DB_NAME=wpalt']
def wp(*argv):return run('docker','run','--rm','--network',network,'--user','33:33','-v',volume+':/var/www/html',*params,images[1],'php','-d','memory_limit=512M','/usr/local/bin/wp',*argv).stdout.strip()
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
class NoRedirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,*args,**kwargs):return None
client=urllib.request.build_opener(urllib.request.ProxyHandler({}),NoRedirect())
def response(url):
 try:r=client.open(url,timeout=10)
 except urllib.error.HTTPError as e:r=e
 with r:return r.status,r.headers,r.read()
def path_of(location):return urllib.parse.urlsplit(location).path
with tempfile.TemporaryDirectory(prefix='wpalt-d08-reference-') as temporary:
 root=Path(temporary);server=None;log=None
 report={'format':'wpalt-d08-redirection-reference-v1','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'status':'incomplete','mode':'native-only' if args.native_only else 'actual-free-plugin-and-native','scope':'Actual pinned free Redirection exact/capture redirects, published slug monitoring and local 404 logging. Native independent workflow; intentional syntax/query/privacy and graph-scope differences. No paid-plugin or full-PCRE parity.'}
 try:
  if not args.native_only:
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
     with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(origin,timeout=10) as ready:
      if ready.status==200:break
    except OSError:pass
    time.sleep(.5)
   else:raise RuntimeError('Reference application not ready')
   wp('core','install','--url='+origin,'--title=D08 Reference Journal','--admin_user=owner','--admin_password='+password,'--admin_email=owner@example.test','--skip-email')
   report['wordpress_version']=wp('core','version');assert report['wordpress_version']=='7.1.3';wp('core','verify-checksums')
   wp('rewrite','structure','/%postname%/','--hard');wp('rewrite','flush','--hard')
   archive=root/'redirection.zip'
   archive.write_bytes(urllib.request.urlopen('https://downloads.wordpress.org/plugin/redirection.5.10.1.zip',timeout=60).read())
   sha=hashlib.sha256(archive.read_bytes()).hexdigest();assert sha=='f68a4be447ecba00c64e8a39d7517807ed49cc18e77b7a364c20cb31848235b7';report['plugin_archive_sha256']=sha
   run('docker','cp',str(archive),site+':/var/www/html/redirection.zip');wp('plugin','install','/var/www/html/redirection.zip','--activate');wp('plugin','verify-checksums','redirection','--strict')
   report['redirection_version']=wp('plugin','get','redirection','--field=version');assert report['redirection_version']=='5.10.1'
   run('docker','cp',str(ROOT/'scripts/reference/d08-seed.php'),site+':/var/www/html/wpalt-d08-source.php')
   source=json.loads(wp('eval-file','/var/www/html/wpalt-d08-source.php'));assert source['before']=='/source-old/' and source['after']=='/source-new/' and source['plugin_monitor_created']['redirect_id']>0
   observed=[]
   for path,code,target in [('/exact-old',301,source['after']),('/legacy/source-new',302,source['after']),('/source-old/',301,source['after']),('/partial/story',301,'/new-part/story'),('/except/story',301,source['after'])]:
    status,headers,_=response(origin+path);assert status==code and path_of(headers['Location'])==target,(path,status,headers.get('Location'));observed.append({'path':path,'status':status,'destination':target})
   for _ in range(2):assert response(origin+'/wpalt-reference-missing')[0]==404
   count=int(wp('eval',"global $wpdb; echo $wpdb->get_var(\"SELECT COUNT(*) FROM {$wpdb->prefix}redirection_404 WHERE url='/wpalt-reference-missing'\");"));assert count>=2
   report['free_plugin']={'redirects':observed,'actual_published_slug_change':True,'plugin_monitor_creation_event':source['plugin_monitor_created'],'404_records_for_two_requests':count,'logging_configuration':'IP logging disabled; no reference request payload or client metadata exported.'}
  native_origin=f'http://127.0.0.1:{port()}';config=root/'native.toml';config.write_text(f'database_url={json.dumps("sqlite://"+str(root/"native.db")+"?mode=rwc")}\ndata_dir={json.dumps(str(root/"data"))}\nlisten={json.dumps(native_origin.removeprefix("http://"))}\nbase_url={json.dumps(native_origin)}\n');config.chmod(0o600)
  run(str(binary),'--config',str(config),'init','--admin-email','owner@example.test',stdin=password+'\n')
  log=open(root/'native-private.log','w');server=subprocess.Popen([str(binary),'--config',str(config),'serve','--external-worker'],stdout=log,stderr=log)
  for _ in range(120):
   assert server.poll() is None,'Native application failed to start'
   try:
    if response(native_origin+'/health')[0]==200:break
   except OSError:pass
   time.sleep(.05)
  else:raise RuntimeError('Native readiness timeout')
  cookies=http.cookiejar.CookieJar();owner=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPCookieProcessor(cookies))
  owner.open(urllib.request.Request(native_origin+'/login',urllib.parse.urlencode({'email':'owner@example.test','password':password}).encode(),headers={'Origin':native_origin})).read()
  html=owner.open(native_origin+'/admin/discovery/routes').read().decode();csrf=re.search(r'name="csrf" value="([^"]+)"',html)[1]
  def api(path,data):
   request=urllib.request.Request(native_origin+path,json.dumps({'csrf':csrf,**data}).encode(),headers={'Origin':native_origin,'Content-Type':'application/json','X-CSRF-Token':csrf})
   with owner.open(request,timeout=30) as r:return json.load(r)
  published=api('/api/admin/content',{'title':'Native story','slug':'native-old','kind':'post','fields':'{}','body':'Actual native publication.','action':'publish'})
  policy=json.load(owner.open(native_origin+'/api/discovery/redirect-policy'))
  settings=policy['policy']['settings'];settings.update(automatic_slug_redirects=True,report_404=True)
  settings['rules']=[{'id':'native_capture','label':'Old story paths','pattern':'^/native-legacy/([a-z0-9-]+)$','target':'/$1','code':302,'priority':0,'case_insensitive':False,'enabled':True}]
  api('/api/discovery/redirect-policy',{'version':policy['version'],'settings':settings})
  api('/api/admin/content/'+published['id'],{'title':'Native story','slug':'native-new','kind':'post','fields':'{}','body':'Actual later native publication.','action':'publish','version':published['version']})
  for path,code in [('/native-old',301),('/native-legacy/native-old',302)]:
   status,headers,_=response(native_origin+path+'?probe=discarded');assert status==code and headers['Location']=='/native-new'
  for _ in range(2):assert response(native_origin+'/native-missing?probe=discarded')[0]==404
  html=owner.open(native_origin+'/admin/discovery/routes').read().decode();assert '/native-missing · 2 hits' in html and 'probe=discarded' not in html
  request=urllib.request.Request(native_origin+'/admin/discovery/rendered-links',urllib.parse.urlencode({'csrf':csrf,'output':'json'}).encode(),headers={'Origin':native_origin})
  with owner.open(request,timeout=30) as r:graph=json.load(r)
  assert graph['complete'] and any(s['path']=='/native-new' for s in graph['sources'])
  report['native']={'published_alias_and_temporary_capture_chain':True,'query_discarded':True,'opt_in_path_aggregate':True,'rendered_graph_sources':len(graph['sources']),'rendered_graph_edges':len(graph['edges']),'differences':['Native whole-path Rust regex rejects PCRE lookaround/backreferences and unanchored partial replacements.','Native query strings are discarded and rules cannot target external origins or reserved workflows.','Native reports aggregate admitted public paths without exporting client/header/query data; the free reference uses its own per-request log model.','Native graph covers declared anonymous publications/language landings; external/client/query expansion is not performed.']}
  report['status']='passed'
 finally:
  if server:
   server.terminate();server.wait(timeout=15)
  if log:log.close()
  if not args.native_only:
   for name in (site,database):run('docker','rm','-f',name,ok=False)
   run('docker','volume','rm',volume,ok=False);run('docker','network','rm',network,ok=False)
  output=ROOT/args.output;output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n')
print('PASS: native fixture only; WordPress/plugin not executed' if args.native_only else 'PASS: actual pinned free Redirection and independently configured native redirect/report/rendered-graph workflows')
