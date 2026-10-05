#!/usr/bin/env python3
"""Native bounded cluster projection + whole-graph validation, not import or HTTP throughput."""
import argparse,hashlib,json,platform,re,secrets,statistics,subprocess,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--output',default='work/m8-cluster-migration-profile.json');a=p.parse_args()
binary=Path(a.binary).resolve();repo=Path(__file__).resolve().parents[1]
report={'format':'wpalt-cluster-migration-profile-v1','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'machine':platform.platform(),'scope':'Fresh-template capture, selected MailPoet contact projection, complete portable graph validation and JSON preview; no restore, network or request throughput. Native child peak RSS includes startup. Three samples per count; synthetic source only.','profiles':[]}
with tempfile.TemporaryDirectory(prefix='wpalt-cluster-profile-') as tmp:
 root=Path(tmp);config=root/'site.toml';config.write_text(f'database_url="sqlite://{root}/site.db?mode=rwc"\ndata_dir="{root}/data"\n')
 subprocess.run([str(binary),'--config',str(config),'init','--admin-email','owner@example.test'],input=secrets.token_urlsafe(24)+'\n',text=True,stdout=subprocess.DEVNULL,check=True)
 for count in (100,1000,10000):
  source=root/'contacts.json';source.write_text(json.dumps({'format':'wpalt-plugin-clusters-v1','source_site':'https://garden.example','mailpoet':{'version':'synthetic-profile','subscribers':[{'id':str(n),'email':f'reader-{n}@example.test','name':'Reader','status':'subscribed'} for n in range(1,count+1)]}}))
  samples=[]
  for _ in range(3):
   metrics=root/'time.txt';timer=['/usr/bin/time','-l'] if platform.system()=='Darwin' else ['/usr/bin/time','-v','-o',str(metrics)]
   with tempfile.TemporaryFile() as out,tempfile.TemporaryFile() as err:
    started=time.perf_counter();r=subprocess.run([*timer,str(binary),'--config',str(config),'wordpress-prepare',str(repo/'tests/fixtures/wordpress-core.xml'),'--owner-email','owner@example.test','--cluster-export',str(source)],stdout=out,stderr=err,timeout=60);elapsed=(time.perf_counter()-started)*1000
    err.seek(0);errors=err.read().decode();assert r.returncode==0,errors
    text=errors if platform.system()=='Darwin' else metrics.read_text();pattern=r'(\d+)\s+maximum resident set size' if platform.system()=='Darwin' else r'Maximum resident set size \(kbytes\): (\d+)';rss=re.search(pattern,text);assert rss
    out.seek(0);preview=json.load(out);assert preview['cluster_mapping']['counts']['contacts']==count
    samples.append({'wall_ms':round(elapsed,3),'peak_rss_bytes':int(rss.group(1))*(1 if platform.system()=='Darwin' else 1024)})
  report['profiles'].append({'contacts':count,'input_bytes':source.stat().st_size,'median_wall_ms':statistics.median(x['wall_ms'] for x in samples),'largest_peak_rss_bytes':max(x['peak_rss_bytes'] for x in samples),'samples':samples})
report['status']='passed';dest=Path(a.output);dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(json.dumps(report,indent=2)+'\n');print('PASS: cluster projection/graph validation at 100/1000/10000 quarantined contacts')
