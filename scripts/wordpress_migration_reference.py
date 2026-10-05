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
              'scope': 'Actual synthetic core WXR export with free Yoast installed; core posts/private page, terms/comment/SEO and fresh recovery. Other plugin adapters remain separate gates.'}
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
        wp('plugin', 'install', 'wordpress-seo', '--activate')
        report['plugins'] = json.loads(wp('plugin', 'list', '--format=json'))
        seed = root/'seed.php'
        seed.write_text('''<?php
foreach (get_posts(['post_type'=>'any','post_status'=>'any','numberposts'=>-1]) as $p) wp_delete_post($p->ID,true);
update_option('permalink_structure','/%postname%/'); flush_rewrite_rules();
$post=wp_insert_post(['post_title'=>'A reference journal','post_name'=>'reference-journal','post_status'=>'publish','post_content'=>'<h2>A calm morning</h2><p>A <strong>real export</strong> with useful words.</p>']);
$page=wp_insert_post(['post_type'=>'page','post_title'=>'Private notes','post_name'=>'private-notes','post_status'=>'private','post_content'=>'<p>Private source notes.</p>']);
$term=wp_insert_term('Migration stories','category',['slug'=>'migration-stories']); wp_set_post_terms($post,[$term['term_id']],'category');
update_post_meta($post,'_yoast_wpseo_metadesc','A real WordPress export migrated locally.');
wp_insert_comment(['comment_post_ID'=>$post,'comment_author'=>'Reference visitor','comment_content'=>'Useful story.','comment_approved'=>1]);
echo json_encode(['posts'=>2,'comments'=>1,'category'=>'migration-stories']);
''')
        run('docker', 'cp', str(seed), site+':/var/www/html/wpalt-m8-seed.php')
        report['source_counts'] = json.loads(wp('eval-file', '/var/www/html/wpalt-m8-seed.php'))
        run('docker', 'exec', site, 'rm', '/var/www/html/wpalt-m8-seed.php')
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
        assert assessment['source_items'] == 2 and assessment['supported_core_items'] == 2
        assert not (root/'offline.db').exists()
        native(template, 'init', '--admin-email', 'owner@example.test', stdin=password+'\n')
        preview = json.loads(native(template, 'wordpress-prepare', export, '--owner-email', 'owner@example.test'))
        package = root/'migration.json'
        native(template, 'wordpress-prepare', export, '--owner-email', 'owner@example.test', '--execute', preview['plan'], '--output', package)
        native(target, 'restore', package)
        with sqlite3.connect(root/'target.db') as db:
            assert db.execute('SELECT COUNT(*) FROM posts').fetchone()[0] == 2
            journal = db.execute("SELECT status,published_body,seo FROM posts WHERE slug='reference-journal'").fetchone()
            assert journal[0] == 'published' and 'real export' in journal[1], {'observed_status': journal[0], 'assessment': report['assessment']}
            assert json.loads(journal[2])['description'] == 'A real WordPress export migrated locally.'
            assert db.execute("SELECT status FROM posts WHERE slug='private-notes'").fetchone()[0] == 'draft'
            assert db.execute('SELECT COUNT(*) FROM comments').fetchone()[0] == 1
            assert db.execute("SELECT COUNT(*) FROM terms WHERE slug='migration-stories'").fetchone()[0] == 1
            assert db.execute('SELECT COUNT(*) FROM shop_orders').fetchone()[0] == 0
        native(target, 'restore', package, ok=False)
        with sqlite3.connect(root/'template.db') as db:
            assert db.execute('SELECT COUNT(*) FROM posts').fetchone()[0] == 0
        report['assertions'] = ['real export accepted offline', 'exact preview execution', 'template unchanged',
                                'counts and relationships recovered', 'private page retained as draft',
                                'literal SEO preserved', 'no invented orders', 'occupied retry denied']
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
