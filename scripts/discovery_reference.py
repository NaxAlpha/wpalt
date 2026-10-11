#!/usr/bin/env python3
"""Pinned current free Yoast publication head/image sitemap plus native authority."""
import argparse, hashlib, http.cookiejar, re, xml.etree.ElementTree as ET, json, secrets, socket, sqlite3, subprocess, tempfile, time, urllib.parse, urllib.request, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--output',default='work/d06-discovery-reference.json');args=p.parse_args()
binary=Path(args.binary).resolve();nonce=uuid.uuid4().hex[:12]
network,database,site,volume=[f'wpalt-d06-{name}-{nonce}' for name in ('net','db','site','files')]
secret,password=secrets.token_hex(24),secrets.token_hex(24)
images=['wordpress:7.1.3-php8.3-apache','wordpress:cli-php8.3','mariadb:11.4']
def run(*argv,stdin=None,ok=True):
 r=subprocess.run(argv,input=stdin,text=True,capture_output=True,timeout=240)
 if ok and r.returncode:
  diagnostic=r.stderr.replace(secret,'[redacted]').replace(password,'[redacted]')
  raise RuntimeError(diagnostic if len(diagnostic)<=4000 else diagnostic[:2000]+'\n[bounded middle omitted]\n'+diagnostic[-2000:])
 return r
params=['-e','WORDPRESS_DB_HOST='+database,'-e','WORDPRESS_DB_USER=wpalt','-e','WORDPRESS_DB_PASSWORD='+secret,'-e','WORDPRESS_DB_NAME=wpalt']
def wp(*argv):return run('docker','run','--rm','--network',network,'--user','33:33','-v',volume+':/var/www/html',*params,images[1],'php','-d','memory_limit=512M','/usr/local/bin/wp',*argv).stdout.strip()
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
client=urllib.request.build_opener(urllib.request.ProxyHandler({}))
from html.parser import HTMLParser
class Head(HTMLParser):
 def __init__(self):super().__init__();self.meta={};self.title='';self.in_title=False
 def handle_starttag(self,tag,attrs):
  a=dict(attrs)
  if tag=='meta':self.meta[a.get('property',a.get('name',''))]=a.get('content','')
  if tag=='title':self.in_title=True
 def handle_endtag(self,tag):
  if tag=='title':self.in_title=False
 def handle_data(self,data):
  if self.in_title:self.title+=data
with tempfile.TemporaryDirectory(prefix='wpalt-d06-reference-') as temporary:
 root=Path(temporary);server=None;log=None
 report={'format':'wpalt-d06-discovery-reference-v1','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'status':'incomplete','scope':'Actual pinned free Yoast title/description/social image and image sitemap; independently configured native workflow. No premium news/video execution, source migration or full visual equivalence claim.'}
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
  wp('core','install','--url='+origin,'--title=D06 Local Journal','--admin_user=owner','--admin_password='+password,'--admin_email=owner@example.test','--skip-email')
  report['wordpress_version']=wp('core','version');assert report['wordpress_version']=='7.1.3'
  wp('core','verify-checksums');wp('option','update','blog_public','1')
  wp('plugin','install','wordpress-seo','--version=28.6','--activate');wp('plugin','verify-checksums','wordpress-seo','--strict');report['yoast_version']=wp('plugin','get','wordpress-seo','--field=version');assert report['yoast_version']=='28.6'
  run('docker','cp',str(ROOT/'tests/fixtures/green.png'),site+':/var/www/html/wpalt-d06-image.png')
  run('docker','cp',str(ROOT/'scripts/reference/d06-seed.php'),site+':/var/www/html/wpalt-d06-source.php')
  source=json.loads(wp('eval-file','/var/www/html/wpalt-d06-source.php'))
  with client.open(source['url'],timeout=15) as response:source_html=response.read().decode()
  parsed=Head();parsed.feed(source_html)
  assert parsed.title==source['title'];assert parsed.meta['description']==source['description'];assert parsed.meta['og:image']==source['image']
  with client.open(origin+'/?sitemap=post&sitemap_n=1',timeout=15) as response:source_xml=response.read()
  image_ns={'s':'http://www.sitemaps.org/schemas/sitemap/0.9','i':'http://www.google.com/schemas/sitemap-image/1.1'}
  sitemap=ET.fromstring(source_xml);reference_images=[n.text for n in sitemap.findall('.//i:loc',image_ns)];assert source['image'] in reference_images
  report['reference']={'html_sha256':hashlib.sha256(source_html.encode()).hexdigest(),'sitemap_sha256':hashlib.sha256(source_xml).hexdigest(),'title':parsed.title,'description':parsed.meta['description'],'social_image_present':True,'image_sitemap_present':True}
  native_origin=f'http://127.0.0.1:{port()}';cfg=root/'site.toml';cfg.write_text(f'database_url="sqlite://{root}/site.db?mode=rwc"\ndata_dir="{root}/data"\nbase_url="{native_origin}"\nlisten="{native_origin.removeprefix("http://")}"\n')
  def native(*argv,stdin=None):return run(str(binary),'--config',str(cfg),*map(str,argv),stdin=stdin).stdout
  native('init','--admin-email','owner@example.test',stdin=password+'\n')
  log=(root/'native.log').open('w');server=subprocess.Popen([str(binary),'--config',str(cfg),'serve'],stdout=log,stderr=log)
  session=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
  def get(path,opener=session):
   with opener.open(native_origin+path,timeout=15) as response:return response.read()
  for _ in range(100):
   try:get('/login');break
   except OSError:time.sleep(.1)
  else:raise RuntimeError('Native reference not ready')
  session.open(urllib.request.Request(native_origin+'/login',data=urllib.parse.urlencode({'email':'owner@example.test','password':password}).encode(),headers={'Origin':native_origin}),timeout=10).read()
  csrf=re.search(r'data-csrf="([^"]+)"',get('/admin/builder').decode()).group(1)
  boundary='d06-'+nonce;body=b''
  for key,value in [('csrf',csrf),('visibility','public'),('alt','D06 garden')]:body+=f'--{boundary}\r\nContent-Disposition: form-data; name="{key}"\r\n\r\n{value}\r\n'.encode()
  body+=f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="d06-native.png"\r\nContent-Type: image/png\r\n\r\n'.encode()+(ROOT/'tests/fixtures/green.png').read_bytes()+f'\r\n--{boundary}--\r\n'.encode()
  session.open(urllib.request.Request(native_origin+'/admin/media',data=body,headers={'Origin':native_origin,'Content-Type':'multipart/form-data; boundary='+boundary}),timeout=15).read()
  image=json.loads(get('/api/admin/media'))['items'][0]['id']
  def post(path,value):
   with session.open(urllib.request.Request(native_origin+path,data=json.dumps(value).encode(),headers={'Origin':native_origin,'Content-Type':'application/json','X-CSRF-Token':csrf}),timeout=15) as response:return json.loads(response.read())
  value={'csrf':csrf,'title':'D06 actual journal','slug':'d06-actual-journal','kind':'post','body':f'A relevant editorial image. ![D06 garden](/media/{image})','fields':'{}','blocks':'[]','action':'publish','seo':json.dumps({'title':source['title'],'description':source['description'],'schema_type':'Article','social_image':image,'social_alt':'D06 garden'})}
  saved=post('/api/admin/content',value);public=urllib.request.build_opener(urllib.request.ProxyHandler({}));native_html=get('/d06-actual-journal',public).decode();head=Head();head.feed(native_html)
  assert head.title==parsed.title and head.meta['description']==parsed.meta['description'];assert head.meta['og:image']==native_origin+'/media/'+image;assert head.meta['og:image:alt']=='D06 garden'
  native_xml=get('/discovery-sitemap/image',public);assert native_origin+'/media/'+image in [n.text for n in ET.fromstring(native_xml).findall('.//i:loc',image_ns)]
  value['version']=saved['version'];value['action']='save';value['seo']=json.dumps({'title':'PRIVATE_SOURCE_CHANGED','social_image':image});post('/api/admin/content/'+saved['id'],value)
  assert 'PRIVATE_SOURCE_CHANGED' not in get('/d06-actual-journal',public).decode()
  report['native']={'html_sha256':hashlib.sha256(native_html.encode()).hexdigest(),'sitemap_sha256':hashlib.sha256(native_xml).hexdigest(),'matched_title_description_image_relationship':True,'working_copy_isolation':True,'difference':'Owner-local identities and image alt per Open Graph guidance; native canonical editorial image index. Does not copy source plugin rules or assert premium/rendering equivalence.'}
  report['status']='passed'
 finally:
  if server:server.terminate();server.wait(timeout=10)
  if log:log.close()
  run('docker','rm','-f',site,database,ok=False);run('docker','volume','rm',volume,ok=False);run('docker','network','rm',network,ok=False)
  output=Path(args.output);output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'status':report['status'],'wordpress':report.get('wordpress_version'),'yoast':report.get('yoast_version')}))
