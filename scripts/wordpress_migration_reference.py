#!/usr/bin/env python3
"""Real WordPress export -> offline assessment -> isolated native recovery.

Linux Docker reference gate. Synthetic credentials and raw exports remain private;
the published evidence records only versions, hashes, counts and assertions.
"""
import argparse
import hashlib
import json
import secrets
import socket
import sqlite3
import subprocess
import tempfile
import time
import urllib.request
import uuid
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--output', default='work/m8-wordpress-reference.json')
args = parser.parse_args()
binary = Path(args.binary).resolve()
nonce = uuid.uuid4().hex[:12]
network, database, site, volume = [f'wpalt-m8-{part}-{nonce}' for part in ('net', 'db', 'site', 'files')]
secret, password = secrets.token_hex(24), secrets.token_hex(24)
images = ['wordpress:7.1.2-php8.3-apache', 'wordpress:cli-php8.3', 'mariadb:11.4']

def run(*argv, stdin=None, ok=True):
    result = subprocess.run(argv, input=stdin, text=True, capture_output=True, timeout=240)
    if ok and result.returncode:
        raise RuntimeError(result.stderr.replace(secret, '[redacted]').replace(password, '[redacted]')[-2000:])
    return result

environment = ['-e', 'WORDPRESS_DB_HOST='+database, '-e', 'WORDPRESS_DB_USER=wpalt',
               '-e', 'WORDPRESS_DB_PASSWORD='+secret, '-e', 'WORDPRESS_DB_NAME=wpalt']

def wp(*argv):
    return run('docker', 'run', '--rm', '--network', network, '--user', '33:33',
               '-v', volume+':/var/www/html', *environment, images[1], 'wp', *argv).stdout.strip()

with tempfile.TemporaryDirectory(prefix='wpalt-m8-reference-') as temp:
    root = Path(temp)
    report = {'format': 'wpalt-m8-wordpress-reference-v1',
              'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'scope': 'Actual synthetic core WXR export with free Yoast, ACF, Elementor and WPForms installed; core posts/private page, terms/comment/SEO and fresh recovery. Explicit ACF scalars retain draft access; other adapters remain separate gates.'}
    try:
        for image in images:
            run('docker', 'pull', image)
        report['images'] = {image: json.loads(run('docker', 'image', 'inspect', image).stdout)[0]['RepoDigests'] for image in images}
        run('docker', 'network', 'create', network)
        run('docker', 'volume', 'create', volume)
        run('docker', 'run', '-d', '--name', database, '--network', network,
            '-e', 'MARIADB_ROOT_PASSWORD='+secret, '-e', 'MARIADB_DATABASE=wpalt',
            '-e', 'MARIADB_USER=wpalt', '-e', 'MARIADB_PASSWORD='+secret, images[2])
        for _ in range(120):
            if run('docker', 'exec', database, 'mariadb-admin', 'ping', '--silent', ok=False).returncode == 0:
                break
            time.sleep(.5)
        else:
            raise RuntimeError('WordPress reference database not ready')
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]
        origin = f'http://127.0.0.1:{port}'
        run('docker', 'run', '-d', '--name', site, '--network', network,
            '-p', f'127.0.0.1:{port}:80', '-v', volume+':/var/www/html', *environment, images[0])
        for _ in range(120):
            try:
                with urllib.request.urlopen(origin, timeout=1) as response:
                    if response.status == 200: break
            except OSError:
                pass
            time.sleep(.5)
        else:
            raise RuntimeError('WordPress reference application not ready')
        wp('core', 'install', '--url='+origin, '--title=Migration journal', '--admin_user=owner',
           '--admin_password='+password, '--admin_email=owner@example.test', '--skip-email')
        report['wordpress_version'] = wp('core', 'version')
        assert report['wordpress_version'] == '7.1.2'
        wp('plugin', 'install', 'wordpress-seo', 'advanced-custom-fields', 'elementor', 'wpforms-lite', '--activate')
        report['plugins'] = json.loads(wp('plugin', 'list', '--format=json'))
        seed = root/'seed.php'
        seed.write_text(r'''<?php
wp_set_current_user(get_user_by('login','owner')->ID);
foreach (get_posts(['post_type'=>'any','post_status'=>'any','numberposts'=>-1]) as $p) wp_delete_post($p->ID,true);
update_option('permalink_structure','/%postname%/'); flush_rewrite_rules();
$post=wp_insert_post(['post_title'=>'A reference journal','post_name'=>'reference-journal','post_status'=>'publish','post_content'=>'<h2>A calm morning</h2><p>A <strong>real export</strong> with useful words.</p>']);
acf_add_local_field_group(['key'=>'group_m8','title'=>'Migration fields','fields'=>[['key'=>'field_m8_teaser','name'=>'garden_teaser','label'=>'Teaser','type'=>'text'],['key'=>'field_m8_count','name'=>'reading_count','label'=>'Count','type'=>'number'],['key'=>'field_m8_marker','name'=>'show_marker','label'=>'Marker','type'=>'true_false']],'location'=>[[['param'=>'post_type','operator'=>'==','value'=>'post']]]]);
$typed=wp_insert_post(['post_title'=>'Typed fields','post_name'=>'typed-fields','post_status'=>'publish','post_content'=>'<p>Source fields need review.</p>']);
update_field('field_m8_teaser','A registered source field.',$typed);update_field('field_m8_count',12,$typed);update_field('field_m8_marker',0,$typed);
if(get_field('garden_teaser',$typed)!=='A registered source field.' || (int)get_field('reading_count',$typed)!==12 || get_field('show_marker',$typed)!==false) throw new Exception('ACF synthetic field seeding failed');
$landing=wp_insert_post(['post_type'=>'page','post_title'=>'Elementor reference','post_name'=>'elementor-reference','post_status'=>'publish']);
$document=\Elementor\Plugin::$instance->documents->get($landing);
$document->set_is_built_with_elementor(true);
$saved=$document->save(['elements'=>[['id'=>'heading1','elType'=>'widget','widgetType'=>'heading','settings'=>['title'=>'A projected headline','header_size'=>'h2'],'elements'=>[]],['id'=>'copy1','elType'=>'widget','widgetType'=>'text-editor','settings'=>['editor'=>'<p>A plugin-saved paragraph.</p>'],'elements'=>[]]],'settings'=>['post_status'=>'publish']]);
if(!$saved || count($document->get_elements_data())!==2) throw new Exception('Elementor synthetic document save failed');
$page=wp_insert_post(['post_type'=>'page','post_title'=>'Private notes','post_name'=>'private-notes','post_status'=>'private','post_content'=>'<p>Private source notes.</p>']);
$term=wp_insert_term('Migration stories','category',['slug'=>'migration-stories']); wp_set_post_terms($post,[$term['term_id']],'category');
update_post_meta($post,'_yoast_wpseo_metadesc','A real WordPress export migrated locally.');
wp_insert_comment(['comment_post_ID'=>$post,'comment_author'=>'Reference visitor','comment_content'=>'Useful story.','comment_approved'=>1]);
$form_data=['fields'=>['9'=>['id'=>'9','type'=>'email','label'=>'Your email','required'=>'1'],'2'=>['id'=>'2','type'=>'textarea','label'=>'Message']],'settings'=>['form_title'=>'Reference contact']];
$form_id=wpforms()->obj('form')->add('Reference contact',['post_content'=>wpforms_encode($form_data)],['builder'=>false]);
if(!$form_id || get_post_type($form_id)!=='wpforms') throw new Exception('WPForms synthetic form creation failed');
$form_definition=json_decode(get_post($form_id)->post_content,true);
if(count($form_definition['fields'])!==2) throw new Exception('WPForms synthetic fields missing');
file_put_contents('/var/www/html/wpalt-m8-forms.json',json_encode(['format'=>'wpalt-wpforms-source-v1','source_site'=>get_option('siteurl'),'plugin_version'=>WPFORMS_VERSION,'forms'=>[['source_id'=>(string)$form_id,'definition'=>$form_definition]]]));
$exportable=get_posts(['post_type'=>array_values(get_post_types(['can_export'=>true])),'post_status'=>['publish','draft','pending','private','future','inherit'],'numberposts'=>-1]);
$types=[];foreach($exportable as $record){$types[$record->post_type]=($types[$record->post_type]??0)+1;}
echo json_encode(['posts'=>4,'export_items'=>count($exportable),'types'=>$types,'comments'=>1,'category'=>'migration-stories','forms'=>1]);
''')
        run('docker', 'cp', str(seed), site+':/var/www/html/wpalt-m8-seed.php')
        report['source_counts'] = json.loads(wp('eval-file', '/var/www/html/wpalt-m8-seed.php'))
        run('docker', 'exec', site, 'rm', '/var/www/html/wpalt-m8-seed.php')
        forms = root/'forms.json'
        run('docker', 'cp', site+':/var/www/html/wpalt-m8-forms.json', str(forms))
        run('docker', 'exec', site, 'rm', '/var/www/html/wpalt-m8-forms.json')
        report['form_export_sha256'] = hashlib.sha256(forms.read_bytes()).hexdigest()
        export = root/'source.xml'; export.write_text(wp('export', '--stdout', '--quiet'))
        report['export_sha256'] = hashlib.sha256(export.read_bytes()).hexdigest()
        def config(name):
            path = root/(name+'.toml')
            path.write_text(f'database_url="sqlite://{root/name}.db?mode=rwc"\ndata_dir="{root/name}"\nbase_url="http://127.0.0.1:18080"\n')
            return path
        def native(cfg, *commands, stdin=None, ok=True):
            result = run(str(binary), '--config', str(cfg), *map(str, commands), stdin=stdin, ok=False)
            assert (result.returncode == 0) == ok, result.stderr
            return result.stdout
        offline, template, target = config('offline'), config('template'), config('target')
        assessment = json.loads(native(offline, 'wordpress-assess', export))
        report['assessment'] = {key: assessment[key] for key in ('source_items', 'supported_core_items', 'types', 'warnings')}
        assert assessment['source_items'] == report['source_counts']['export_items'] and assessment['supported_core_items'] == 4
        assert assessment['types'] == report['source_counts']['types']
        assert not (root/'offline.db').exists()
        native(template, 'init', '--admin-email', 'owner@example.test', stdin=password+'\n')
        mapping = root/'fields.json'
        mapping.write_text(json.dumps({'format':'wpalt-acf-scalar-map-v1','fields':[
            {'source_name':'garden_teaser','source_key':'field_m8_teaser','target_name':'teaser','kind':'string'},
            {'source_name':'reading_count','source_key':'field_m8_count','target_name':'reading_count','kind':'number'},
            {'source_name':'show_marker','source_key':'field_m8_marker','target_name':'show_marker','kind':'boolean'}]}))
        preview = json.loads(native(template, 'wordpress-prepare', export, '--owner-email', 'owner@example.test', '--field-mapping', mapping, '--elementor-content', '--wpforms-export', forms))
        package = root/'migration.json'
        native(template, 'wordpress-prepare', export, '--owner-email', 'owner@example.test', '--field-mapping', mapping, '--elementor-content', '--wpforms-export', forms, '--execute', preview['plan'], '--output', package)
        native(target, 'restore', package)
        with sqlite3.connect(root/'target.db') as db:
            assert db.execute('SELECT COUNT(*) FROM posts').fetchone()[0] == 4
            journal = db.execute("SELECT status,published_body,seo FROM posts WHERE slug='reference-journal'").fetchone()
            assert journal[0] == 'published' and 'real export' in journal[1], {'observed_status': journal[0], 'assessment': report['assessment']}
            assert json.loads(journal[2])['description'] == 'A real WordPress export migrated locally.'
            assert db.execute("SELECT status FROM posts WHERE slug='private-notes'").fetchone()[0] == 'draft'
            typed = db.execute("SELECT fields,status FROM posts WHERE slug='typed-fields'").fetchone()
            assert json.loads(typed[0]) == {'teaser':'A registered source field.','reading_count':12,'show_marker':False}
            assert typed[1] == 'draft'
            landing = db.execute("SELECT body,status,published_body FROM posts WHERE slug='elementor-reference'").fetchone()
            assert 'A projected headline' in landing[0] and 'A plugin-saved paragraph.' in landing[0]
            assert landing[1] == 'draft' and landing[2] == ''
            projections = [w for w in preview['warnings'] if w['code']=='elementor_content_projection']
            assert len(projections)==1 and projections[0]['report']['elements']==2
            assert db.execute('SELECT COUNT(*) FROM comments').fetchone()[0] == 1
            assert db.execute("SELECT COUNT(*) FROM terms WHERE slug='migration-stories'").fetchone()[0] == 1
            assert db.execute('SELECT COUNT(*) FROM shop_orders').fetchone()[0] == 0
            form = db.execute('SELECT draft,live,published_version,entry_count FROM business_forms').fetchone()
            definition = json.loads(form[0])
            assert [f['name'] for f in definition['fields']] == ['wpforms_9','wpforms_2']
            assert definition['fields'][0]['schema']['required']
            assert not definition['notifications'] and not definition.get('subscription') and not definition.get('registration')
            assert form[1:] == ('',0,0)
            assert db.execute("SELECT items FROM business_usage WHERE kind='forms'").fetchone()[0] == 1
            assert preview['wpforms_mapping']['source_forms'] == report['source_counts']['forms'] == 1

        # Add selected free-plugin operational definitions after the core gate.
        # Plugin-generated pages are outside the earlier exact core inventory.
        wp('plugin', 'install', 'mailpoet', 'sensei-lms', 'woocommerce', '--activate')
        wp('plugin', 'install', 'https://github.com/strangerstudios/paid-memberships-pro/archive/refs/tags/3.8.7.zip', '--activate')
        wp('option','update','woocommerce_currency','USD')
        fixture=Path(__file__).resolve().with_name('wordpress_cluster_reference.php')
        run('docker','cp',str(fixture),site+':/var/www/html/wpalt-m8-cluster-seed.php')
        report['cluster_source_counts']=json.loads(wp('eval-file','/var/www/html/wpalt-m8-cluster-seed.php'))
        report['cluster_plugins']=json.loads(wp('plugin','list','--format=json'))
        clusters=root/'clusters.json';run('docker','cp',site+':/var/www/html/wpalt-m8-clusters.json',str(clusters))
        run('docker','exec',site,'rm','/var/www/html/wpalt-m8-clusters.json','/var/www/html/wpalt-m8-cluster-seed.php')
        report['cluster_export_sha256']=hashlib.sha256(clusters.read_bytes()).hexdigest()
        args=('wordpress-prepare',export,'--owner-email','owner@example.test','--cluster-export',clusters)
        projected=json.loads(native(template,*args));assert projected['cluster_mapping']['counts']==report['cluster_source_counts']
        projected_file=root/'clusters-native.json';native(template,*args,'--execute',projected['plan'],'--output',projected_file)
        clustered=config('clustered');native(clustered,'restore',projected_file)
        with sqlite3.connect(root/'clustered.db') as db:
            assert db.execute('SELECT suppressed FROM audience_contacts').fetchone()[0]==1
            assert db.execute('SELECT COUNT(*) FROM member_policies WHERE enabled=0').fetchone()[0]==2
            course=json.loads(db.execute('SELECT draft FROM member_courses').fetchone()[0])
            assert [l['title'] for l in course['lessons']]==['First reference lesson','Second reference lesson']
            assert db.execute('SELECT price_minor,stock_total,active FROM shop_variants').fetchone()==(1234,4,0)
            assert db.execute('SELECT COUNT(*) FROM posts').fetchone()[0]==6
            assert db.execute("SELECT COUNT(*) FROM posts WHERE status='published'").fetchone()[0]==0
            for name in ['audience_memberships','mail_jobs','member_grants','member_progress','shop_orders','shop_payments']:
                assert db.execute('SELECT COUNT(*) FROM '+name).fetchone()[0]==0
        report['cluster_projection']=projected['cluster_mapping']
        native(target, 'restore', package, ok=False)
        with sqlite3.connect(root/'template.db') as db:
            assert db.execute('SELECT COUNT(*) FROM posts').fetchone()[0] == 0
        report['assertions'] = ['real export accepted offline', 'exact preview execution', 'template unchanged',
                                'counts and relationships recovered', 'private page retained as draft',
                                'literal SEO preserved', 'registered ACF scalars recovered as draft', 'Elementor plugin-saved content projected as draft', 'free WPForms plugin-created definition recovered as unpublished draft with actions disabled', 'no invented orders', 'occupied retry denied']
        report['status'] = 'passed'
        output = Path(args.output); output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, indent=2)+'\n')
        print('PASS: real WordPress/free Yoast export, isolated native recovery and source reconciliation')
    finally:
        if report.get('status') != 'passed':
            report['status'] = 'failed'
            output = Path(args.output); output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(json.dumps(report, indent=2)+'\n')
        for container in (site, database):
            run('docker', 'rm', '-f', container, ok=False)
        run('docker', 'volume', 'rm', volume, ok=False)
        run('docker', 'network', 'rm', network, ok=False)
