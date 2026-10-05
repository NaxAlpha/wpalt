#!/usr/bin/env python3
"""Isolated, same-runner publishing/cache reference; not full plugin parity.
Requires Linux Docker. Downloads official images/plugins; reports exact identities.
Private synthetic credentials are never published. Containers/volumes are removed.
"""
import argparse, concurrent.futures, hashlib, json, math, os, platform, secrets, socket, subprocess, time, urllib.request, uuid
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--output',default='work/m7-matched-reference.json');args=p.parse_args()
binary=Path(args.binary).resolve();root=Path('work/m7-reference-'+uuid.uuid4().hex).resolve();root.mkdir(mode=0o700)
nonce=uuid.uuid4().hex[:10];network='wpalt-ref-'+nonce;db='wpalt-ref-db-'+nonce;site='wpalt-ref-site-'+nonce;volume='wpalt-ref-files-'+nonce
secret=secrets.token_hex(24);password=secrets.token_hex(24)
images=['wordpress:7.1.2-php8.3-apache','wordpress:cli-php8.3','mariadb:11.4']
cpus=sorted(os.sched_getaffinity(0))[:2];cpu_set=','.join(map(str,cpus))
report={'schema':1,'source_sha':os.environ.get('GITHUB_SHA','local'),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'machine':platform.platform(),'shared_cpu_set':cpus,'scope':'Equivalent 1,000 synthetic published articles, 20-title page, article, native search, generated sitemap; free ACF fields, SEO, local backup and cache plugins active. WooCommerce active but commerce/learning/payment/security/plugin premium parity is not a benchmark claim. HTTP document timing, not browser rendering.','conditions':'Sequential system/cache phases on one fresh Linux runner. 10 warmups; 200 requests per path/concurrency 1 and 10. Python client overhead included. Server caches tested separately; private/customer routes excluded. wpalt retains its cumulative integrated modules. No numerical pass/fail timing thresholds.','profiles':[]}
def run(*argv,input=None,ok=True,timeout=240):
 r=subprocess.run(argv,input=input,text=True,capture_output=True,timeout=timeout)
 if ok and r.returncode:raise RuntimeError(r.stderr.replace(secret,'[redacted]').replace(password,'[redacted]')[-2500:])
 return r
wp_env=['-e','WORDPRESS_DB_HOST='+db,'-e','WORDPRESS_DB_USER=wpalt','-e','WORDPRESS_DB_PASSWORD='+secret,'-e','WORDPRESS_DB_NAME=wpalt']
def wp(*argv):return run('docker','run','--rm','--network',network,'--user','33:33','-v',volume+':/var/www/html',*wp_env,images[1],'wp',*argv).stdout.strip()
def percentile(v,q):return sorted(v)[math.ceil(len(v)*q)-1]
def measure(url,concurrency):
 def one(_):
  start=time.perf_counter()
  with urllib.request.urlopen(url,timeout=30) as r:assert r.status==200;raw=r.read()
  return (time.perf_counter()-start)*1000,len(raw)
 start=time.perf_counter()
 with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:items=list(pool.map(one,range(200)))
 times=[x[0] for x in items]
 return {'concurrency':concurrency,'requests':200,'p50_ms':round(percentile(times,.5),3),'p95_ms':round(percentile(times,.95),3),'p99_ms':round(percentile(times,.99),3),'requests_per_second':round(200/(time.perf_counter()-start),1),'document_bytes':items[0][1]}
def ready(url,process=None):
 for _ in range(200):
  if process:assert process.poll() is None,'Measured application exited'
  try:
   with urllib.request.urlopen(url,timeout=1) as r:
    if r.status==200:return
  except OSError:pass
  time.sleep(.2)
 raise RuntimeError('Reference readiness timeout')
def free_port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
try:
 for image in images:run('docker','pull',image)
 report['images']={image:json.loads(run('docker','image','inspect',image).stdout)[0]['RepoDigests'] for image in images}
 run('docker','network','create',network);run('docker','volume','create',volume)
 run('docker','run','-d','--name',db,'--network',network,'--cpuset-cpus',cpu_set,'--memory','512m','-e','MARIADB_ROOT_PASSWORD='+secret,'-e','MARIADB_DATABASE=wpalt','-e','MARIADB_USER=wpalt','-e','MARIADB_PASSWORD='+secret,images[2],'--innodb-buffer-pool-size=64M')
 for _ in range(120):
  if run('docker','exec',db,'mariadb-admin','ping','--silent',ok=False).returncode==0:break
  time.sleep(.5)
 else:raise RuntimeError('Reference database readiness timeout')
 port=free_port();origin=f'http://127.0.0.1:{port}'
 run('docker','run','-d','--name',site,'--network',network,'--cpuset-cpus',cpu_set,'--memory','768m','-p',f'127.0.0.1:{port}:80','-v',volume+':/var/www/html',*wp_env,images[0])
 ready(origin)
 wp('core','install','--url='+origin,'--title=The Local Journal','--admin_user=owner','--admin_password='+password,'--admin_email=owner@example.test','--skip-email')
 assert wp('core','version')=='7.1.2'
 wp('config','set','DISABLE_WP_CRON','true','--raw');wp('config','set','WP_CACHE','false','--raw')
 wp('plugin','install','advanced-custom-fields','wordpress-seo','updraftplus','wp-super-cache','woocommerce','--activate')
 report['plugins']=json.loads(wp('plugin','list','--format=json'))
 seed=root/'seed.php'
 seed.write_text('''<?php
 update_option('posts_per_page',20);update_option('permalink_structure','/%postname%/');flush_rewrite_rules();
 foreach(get_posts(['numberposts'=>-1,'post_type'=>'post','post_status'=>'any']) as $p) wp_delete_post($p->ID,true);
 acf_add_local_field_group(['key'=>'group_ref','title'=>'Reference fields','fields'=>[['key'=>'field_ref_subtitle','name'=>'subtitle','label'=>'Subtitle','type'=>'text']],'location'=>[[['param'=>'post_type','operator'=>'==','value'=>'post']]]]);
 $names=[['A quieter place on the web','Our digital spaces should feel like places we own. A little slower, more considered, and built around the stories we want to tell.'],['Notes from the garden','Good things take root with a little patience. These are the things we have been making, reading and learning.'],['Building with intention','A small website can do a lot. Start with clear words, thoughtful structure and tools that stay out of the way.'],['A field guide to independent publishing','Own your words, your audience and your archives. Publishing should not require a collection of accounts to keep your site running.']];
 for($i=1;$i<=1000;$i++){[$title,$lead]=$names[($i-1)%4];$body='<p>'.$lead.'</p><h2>Room to think</h2><p>This is a working wpalt example: structured content, a shared theme and a publishing workflow you can run on your own server.</p><ul><li>Draft and preview before publishing.</li><li>Keep unfinished changes away from your live pages.</li><li>Back up your content and take it with you.</li></ul><blockquote>Useful tools should make good work easier.</blockquote><h3>What comes next</h3><p>Explore the administration panel, edit this story, and switch between the Paper and Ink themes.</p>';$id=wp_insert_post(['post_title'=>$title.' · '.$i,'post_name'=>'journal-'.$i,'post_status'=>'publish','post_content'=>$body]);update_field('field_ref_subtitle',$lead,$id);}
 echo json_encode(['published_posts'=>(int)wp_count_posts()->publish,'acf_subtitle'=>get_field('subtitle',$id),'peak_seed_php_bytes'=>memory_get_peak_usage(true)]);
''')
 # eval-file shares the named volume, not the application container /tmp.
 run('docker','cp',str(seed),site+':/var/www/html/wpalt-seed.php');report['fixture']=json.loads(wp('eval-file','/var/www/html/wpalt-seed.php'));run('docker','exec',site,'rm','/var/www/html/wpalt-seed.php')
 assert report['fixture']['published_posts']==1000 and isinstance(report['fixture']['acf_subtitle'],str) and len(report['fixture']['acf_subtitle'])>50
 # A minimal native reference theme gives both systems a bounded 20-item page.
 theme=root/'theme';theme.mkdir();(theme/'style.css').write_text('/* Theme Name: wpalt synthetic reference */\n')
 (theme/'index.php').write_text('<?php ?><!doctype html><html lang="en"><head><meta charset="utf-8"><?php wp_head(); ?></head><body><main><?php if(have_posts()):while(have_posts()):the_post(); ?><article><h2><a href="<?php the_permalink(); ?>"><?php the_title(); ?></a></h2><p><?php echo esc_html(get_field("subtitle")); ?></p><?php if(is_single())the_content(); ?></article><?php endwhile;endif; ?></main><?php wp_footer(); ?></body></html>')
 run('docker','cp',str(theme),site+':/var/www/html/wp-content/themes/wpalt-reference');wp('theme','activate','wpalt-reference')
 paths={'home':'/','story':'/journal-1/','search':'/?s=publishing','sitemap':'/post-sitemap.xml'}
 for cache in [False,True]:
  if cache:
   wp('config','set','WP_CACHE','true','--raw');wp('eval',"wp_cache_enable(); wp_cache_setting('super_cache_enabled', true); wp_cache_setting('cache_enabled', true);")
  for endpoint,path in paths.items():
   for _ in range(10):
    with urllib.request.urlopen(origin+path,timeout=30) as r:r.read()
   for c in [1,10]:report['profiles'].append({'system':'wordpress','cache_enabled':cache,'endpoint':endpoint,**measure(origin+path,c)})
  report.setdefault('wordpress_runtime',{})['cached' if cache else 'uncached']={'application_cgroup':json.loads(run('docker','stats','--no-stream','--format','{{json .}}',site).stdout),'database_cgroup':json.loads(run('docker','stats','--no-stream','--format','{{json .}}',db).stdout),'application_disk_bytes':int(run('docker','exec',site,'du','-sb','/var/www/html').stdout.split()[0]),'database_disk_bytes':int(run('docker','exec',db,'du','-sb','/var/lib/mysql').stdout.split()[0])}
  if cache:
   count=int(run('docker','exec',site,'sh','-c','find /var/www/html/wp-content/cache -type f | wc -l').stdout)
   assert count>0,'Real plugin must produce cache files';report['cache_files']=count
 # Stop reference services during native measurement: no concurrent reference load.
 run('docker','stop',site,db)
 previous=os.sched_getaffinity(0);os.sched_setaffinity(0,set(cpus))
 try:
  for cached in [False,True]:
   out=root/('wpalt-cached.json' if cached else 'wpalt-uncached.json')
   flags=['--cached'] if cached else []
   run('python3','scripts/benchmark.py','--binary',str(binary),'--posts','1000','--requests','200','--composed','--discovery','--commerce',*flags,'--output',str(out))
   data=json.loads(out.read_text());report.setdefault('wpalt',{})['cached' if cached else 'uncached']=data
 finally:os.sched_setaffinity(0,previous)
 report['memory_interpretation']='WordPress Apache/PHP and MariaDB use distinct container cgroups (page cache/accounting included); wpalt reports process RSS snapshots with its SQLite page cache and embedded runtime. These accounting models differ. No equal-memory ratio or full-feature parity claim. Database/image/runtime dependencies and site bytes are reported separately.'
 report['status']='passed';Path(args.output).parent.mkdir(parents=True,exist_ok=True);Path(args.output).write_text(json.dumps(report,indent=2)+'\n');print('PASS: same-runner publishing/search/sitemap/cache reference with recorded free plugin versions and exact wpalt executable.')
finally:
 for container in [site,db]:run('docker','rm','-f',container,ok=False)
 run('docker','volume','rm',volume,ok=False);run('docker','network','rm',network,ok=False)
 import shutil;shutil.rmtree(root)
