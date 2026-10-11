#!/usr/bin/env python3
"""Pinned current free Yoast publication head/image sitemap plus native authority."""
import argparse, hashlib, http.cookiejar, re, xml.etree.ElementTree as ET, json, secrets, socket, sqlite3, subprocess, tempfile, time, urllib.parse, urllib.request, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--output',default='work/d06-discovery-reference.json');p.add_argument('--structured',action='store_true');args=p.parse_args()
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
 def __init__(self):super().__init__();self.meta={};self.title='';self.in_title=False;self.in_graph=False;self.graphs=[];self.graph_text=''
 def handle_starttag(self,tag,attrs):
  a=dict(attrs)
  if tag=='meta':self.meta[a.get('property',a.get('name',''))]=a.get('content','')
  if tag=='title':self.in_title=True
  if tag=='script' and a.get('type')=='application/ld+json':self.in_graph=True;self.graph_text=''
 def handle_endtag(self,tag):
  if tag=='title':self.in_title=False
  if tag=='script' and self.in_graph:self.graphs.append(json.loads(self.graph_text));self.in_graph=False
 def handle_data(self,data):
  if self.in_title:self.title+=data
  if self.in_graph:self.graph_text+=data
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
  if args.structured:wp('eval',"wp_update_user(['ID'=>1,'display_name'=>'Public pen name']);")
  run('docker','cp',str(ROOT/'tests/fixtures/green.png'),site+':/var/www/html/wpalt-d06-image.png')
  run('docker','cp',str(ROOT/'scripts/reference/d06-seed.php'),site+':/var/www/html/wpalt-d06-source.php')
  source=json.loads(wp('eval-file','/var/www/html/wpalt-d06-source.php'))
  if args.structured:
   # A new publication has equal publication/modification timestamps. The
   # actual free plugin deliberately omits dateModified until a later edit.
   # Perform a real public edit; never force artificial timestamp metadata.
   time.sleep(1.1)
   wp('post','update',str(source['id']),'--post_content=An actual later editorial update. <img src="'+source['image']+'" alt="D06 garden" />')
   wp('eval','YoastSEO()->classes->get(\\Yoast\\WP\\SEO\\Builders\\Indexable_Builder::class)->build_for_id_and_type('+str(int(source['id']))+', "post");')
   source['published_gmt']=wp('post','get',str(source['id']),'--field=post_date_gmt')
   source['modified_gmt']=wp('post','get',str(source['id']),'--field=post_modified_gmt')
   assert source['modified_gmt']>source['published_gmt']
  with client.open(source['url'],timeout=15) as response:source_html=response.read().decode()
  parsed=Head();parsed.feed(source_html)
  assert parsed.title==source['title'];assert parsed.meta['description']==source['description'];assert parsed.meta['og:image']==source['image']
  # Yoast redirects explicit page 0/1 to rewritten URLs; use its admitted
  # first-page query without a page number on the plain-permalink fixture.
  with client.open(origin+'/?sitemap=post',timeout=15) as response:
   assert 'xml' in response.headers.get('Content-Type','').lower(), 'Actual plugin sitemap must be XML'
   source_xml=response.read()
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
  if args.structured:
   source_nodes=[n for g in parsed.graphs for n in g.get('@graph',[])];source_ids={n['@id']:n for n in source_nodes if '@id' in n}
   article=next(n for n in source_nodes if 'Article' in ([n.get('@type')] if isinstance(n.get('@type'),str) else n.get('@type',[])))
   authors=article['author'] if isinstance(article['author'],list) else [article['author']];actual_author=source_ids[authors[0]['@id']]
   assert article['headline']==source['factual_title'] and actual_author['name']=='Public pen name'
   assert article.get('datePublished') and article.get('dateModified')
   registry=json.loads(get('/api/admin/design'));model=registry['registry']['models']['post']
   person={'label':'Public authors','fields':{'name':{'kind':'string'}},'structured':{'vocabulary':'30.1','primary':'person','entities':{'person':{'types':['Person'],'properties':{'name':{'kind':'field','path':['name'],'datatype':'Text'},'url':{'kind':'publication','attribute':'url'}}}}}}
   post('/api/admin/models/public_person',{'csrf':csrf,'version':0,'definition':person})
   author=post('/api/admin/content',{'title':'Public author profile','slug':'public-author-profile','kind':'public_person','body':'Publicly authored biography.','fields':json.dumps({'name':actual_author['name']}),'action':'publish'})
   model['fields'].update({'public_author':{'kind':'relationship','target':'public_person'},'public_image':{'kind':'media'}})
   model['structured']={'vocabulary':'30.1','primary':'article','entities':{'article':{'types':['Article'],'properties':{'headline':{'kind':'publication','attribute':'title'},'datePublished':{'kind':'publication','attribute':'published_at'},'image':{'kind':'image','path':['public_image']},'author':{'kind':'relationship','path':['public_author'],'entity':'person'}}}}}
   post('/api/admin/models/post',{'csrf':csrf,'version':registry['model_versions']['post'],'definition':model})
   post('/api/admin/content',{'title':source['factual_title'],'slug':'native-structured-reference','kind':'post','body':'Actual local editorial image.','fields':json.dumps({'public_author':author['id'],'public_image':image}),'action':'publish','seo':json.dumps({'title':source['title']})})
   structured_html=get('/native-structured-reference',public).decode();structured_head=Head();structured_head.feed(structured_html)
   native_nodes=next(g['@graph'] for g in structured_head.graphs if '@graph' in g);native_article=native_nodes[0]
   assert native_article['headline']==article['headline'];assert native_nodes[1]['name']==actual_author['name'];assert native_article['image']['contentUrl']==native_origin+'/media/'+image
   assert native_article['datePublished'] and 'dateModified' not in native_article
   assert 'Public pen name' in structured_html and 'owner@example.test' not in structured_html
   report['structured']={'source_article_type':article['@type'],'source_public_author_type':actual_author['@type'],'source_headline':article['headline'],'source_actual_publication_dates_present':True,'source_modification_was_actual_later_public_edit':True,'native_matches_explicit_public_author_and_visible_headline':True,'native_local_image_with_dimensions':True,'native_original_publication_date_present':True,'native_does_not_invent_modification_date':True,'source_graph_sha256':hashlib.sha256(json.dumps(source_nodes,sort_keys=True).encode()).hexdigest(),'native_graph_sha256':hashlib.sha256(json.dumps(native_nodes,sort_keys=True).encode()).hexdigest(),'difference':'Free Yoast derives author from public WordPress display_name and emits actual WordPress publication time and a modification time only after the real later published edit. Native author is a separately published explicit identity; original publication time is factual and no new modification date is invented. No premium location execution or complete provider feature equivalence.'}
   report['format']='wpalt-d07-structured-reference-v1'
   report['scope']='Actual pinned free Yoast Article/public author/dates alongside the existing title/image/sitemap reference, compared with independently configured native structured author/image/headline. No premium execution, migration or universal schema parity.'
  report['status']='passed'
 finally:
  if server:server.terminate();server.wait(timeout=10)
  if log:log.close()
  run('docker','rm','-f',site,database,ok=False);run('docker','volume','rm',volume,ok=False);run('docker','network','rm',network,ok=False)
  output=Path(args.output);output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'status':report['status'],'wordpress':report.get('wordpress_version'),'yoast':report.get('yoast_version')}))
