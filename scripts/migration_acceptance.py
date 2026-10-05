#!/usr/bin/env python3
"""Real core WXR preview -> private package -> fresh restore; no source/network fetch."""
import argparse,json,secrets,subprocess,tempfile,sqlite3
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',default='target/debug/wpalt');a=p.parse_args()
binary=Path(a.binary).resolve();repository=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='wpalt-migration-') as tmp:
    root=Path(tmp);source=repository/'tests/fixtures/wordpress-core.xml'
    def config(name):
        file=root/(name+'.toml');file.write_text(f'database_url="sqlite://{root/name}.db?mode=rwc"\ndata_dir="{root/name}"\nbase_url="http://127.0.0.1:18080"\n');return file
    template=config('template');target=config('target');offline=config('offline')
    def run(cfg,*args,stdin=None,ok=True):
        r=subprocess.run([str(binary),'--config',str(cfg),*map(str,args)],input=stdin,text=True,capture_output=True)
        assert (r.returncode==0)==ok,(args,r.stderr)
        return r
    report=json.loads(run(offline,'wordpress-assess',source).stdout)
    assert report['source_items']==4 and report['supported_core_items']==2
    assert not (root/'offline.db').exists() and not (root/'offline').exists(),'Offline assessment must not initialize state'
    password=secrets.token_urlsafe(24)
    run(template,'init','--admin-email','owner@example.test',stdin=password+'\n')
    media=root/'uploads';(media/'2025').mkdir(parents=True);(media/'2025/garden.png').write_bytes((repository/'tests/fixtures/animated.png').read_bytes())
    args=('wordpress-prepare',source,'--owner-email','owner@example.test','--media-dir',media)
    preview=json.loads(run(template,*args).stdout);assert preview['media_mapped']==1
    output=root/'migration.json'
    run(template,*args,'--execute','wrong-preview','--output',output,ok=False);assert not output.exists()
    run(template,*args,'--execute',preview['plan'],'--output',output)
    assert output.stat().st_mode&0o077==0
    run(template,*args,'--execute',preview['plan'],'--output',output,ok=False)
    with sqlite3.connect(root/'template.db') as db:assert db.execute('SELECT COUNT(*) FROM posts').fetchone()[0]==0
    run(target,'restore',output)
    with sqlite3.connect(root/'target.db') as db:
        assert db.execute('SELECT COUNT(*) FROM posts').fetchone()[0]==2
        assert db.execute("SELECT status FROM posts WHERE slug='private-page'").fetchone()[0]=='draft'
        assert db.execute('SELECT COUNT(*) FROM shop_orders').fetchone()[0]==0
        assert db.execute('SELECT COUNT(*) FROM media').fetchone()[0]==1
    run(target,'restore',output,ok=False)
    changed=root/'changed.xml';changed.write_text(source.read_text().replace('quiet garden','changed garden'))
    run(template,'wordpress-prepare',changed,'--owner-email','owner@example.test','--media-dir',media,'--execute',preview['plan'],'--output',root/'changed.json',ok=False)
    assert not (root/'changed.json').exists()
    # ACF mappings bind their field references/types and never infer public access.
    acf_source=root/'acf.xml';acf_source.write_text(source.read_text().replace('<wp:comment>',(repository/'tests/fixtures/wordpress-acf-values.xml.fragment').read_text()+'<wp:comment>'))
    mapping=repository/'tests/fixtures/wordpress-acf-map.json'
    selected=('wordpress-prepare',acf_source,'--owner-email','owner@example.test','--field-mapping',mapping)
    mapped_preview=json.loads(run(template,*selected).stdout)
    mapped_output=root/'mapped.json';run(template,*selected,'--execute',mapped_preview['plan'],'--output',mapped_output)
    mapped_target=config('mapped');run(mapped_target,'restore',mapped_output)
    with sqlite3.connect(root/'mapped.db') as db:
        row=db.execute("SELECT fields,status FROM posts WHERE slug='garden'").fetchone();fields=json.loads(row[0])
        assert fields=={'teaser':'A field-owned garden story.','reading_count':12,'show_marker':False}
        assert row[1]=='draft'
    changed_map=root/'changed-map.json';selected_fields=json.loads(mapping.read_text());selected_fields['fields'][0]['target_name']='another_teaser';changed_map.write_text(json.dumps(selected_fields))
    run(template,'wordpress-prepare',acf_source,'--owner-email','owner@example.test','--field-mapping',changed_map,'--execute',mapped_preview['plan'],'--output',root/'stale-mapped.json',ok=False)
    assert not (root/'stale-mapped.json').exists()
    # Explicit Elementor projection preserves text and reports omitted design/widgets.
    elementor={'title':'Landing page','type':'page','version':'0.4','page_settings':{'background_color':'#fff'},'content':[
        {'id':'title1','elType':'widget','widgetType':'heading','settings':{'title':'Projected headline','header_size':'h2'},'elements':[]},
        {'id':'copy1','elType':'widget','widgetType':'text-editor','settings':{'editor':'<p>Projected body.</p>'},'elements':[]},
        {'id':'unsupported1','elType':'widget','widgetType':'posts','settings':{},'elements':[]}]}
    elementor_file=root/'elementor.json';elementor_file.write_text(json.dumps(elementor))
    projection=json.loads(run(offline,'elementor-project',elementor_file).stdout)
    draft_file=root/'elementor-draft.json'
    run(offline,'elementor-project',elementor_file,'--execute','stale','--output',draft_file,ok=False)
    assert not draft_file.exists()
    run(offline,'elementor-project',elementor_file,'--execute',projection['plan'],'--output',draft_file)
    assert draft_file.stat().st_mode&0o077==0
    draft=json.loads(draft_file.read_text());assert draft['document']['root']['content'][0]['type']=='heading'
    assert any(w['code']=='unsupported_element' for w in draft['report']['warnings'])
    run(offline,'elementor-project',elementor_file,'--execute',projection['plan'],'--output',draft_file,ok=False)
    assert not (root/'offline.db').exists() and not (root/'offline').exists()
    data=json.dumps(elementor['content'])
    elem_source=root/'elementor.xml';elem_source.write_text(source.read_text().replace('<wp:comment>', '<wp:postmeta><wp:meta_key>_elementor_data</wp:meta_key><wp:meta_value><![CDATA['+data+']]></wp:meta_value></wp:postmeta><wp:comment>'))
    selected=('wordpress-prepare',elem_source,'--owner-email','owner@example.test','--elementor-content')
    elem_preview=json.loads(run(template,*selected).stdout)
    assert any(w['code']=='elementor_content_projection' for w in elem_preview['warnings'])
    elem_output=root/'elementor-recovery.json';run(template,*selected,'--execute',elem_preview['plan'],'--output',elem_output)
    elem_target=config('elementor');run(elem_target,'restore',elem_output)
    with sqlite3.connect(root/'elementor.db') as db:
        row=db.execute("SELECT body,status,published_body FROM posts WHERE slug='garden'").fetchone()
        assert 'Projected headline' in row[0] and 'Projected body.' in row[0]
        assert row[1]=='draft' and row[2]==''
    run(template,'wordpress-prepare',elem_source,'--owner-email','owner@example.test','--execute',elem_preview['plan'],'--output',root/'stale-elementor.json',ok=False)
    assert not (root/'stale-elementor.json').exists()
print('PASS: offline namespace-aware WXR assessment, exact-source preview, private non-overwriting package, local media, untouched template, fresh core recovery and safe private/payment mappings')
