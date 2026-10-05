#!/usr/bin/env python3
"""Verify exact-source clean builds; publish retry-safe dated development releases."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
FILES = {'wpalt', 'BUILD.json', 'INSTALL.md', 'wpalt.example.toml', 'operations.md',
         'theme-authoring.md', 'membership-learning.md', 'm5-contract.md', 'commerce-reservations.md', 'm6-contract.md', 'resilient-operations.md', 'm7-contract.md', 'migration-and-extensions.md', 'm8-contract.md', 'THIRD_PARTY_NOTICES.md'}


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def verify(directory, source):
    if len(source) != 40 or any(c not in '0123456789abcdef' for c in source):
        raise ValueError('Expected a complete source commit')
    archives = list(directory.glob('*.tar.gz'))
    if len(archives) != 1:
        raise ValueError('Expected exactly one clean build archive')
    archive = archives[0]
    checksum, name = (directory / 'SHA256SUMS').read_text().strip().split()
    if name != archive.name or digest(archive.read_bytes()) != checksum:
        raise ValueError('Archive checksum mismatch')
    with tarfile.open(archive) as tar:
        members = tar.getmembers()
        if len(members) != len(FILES) or {m.name for m in members} != FILES:
            raise ValueError('Unexpected, duplicated or missing archive members')
        if any(not m.isfile() or m.size > 200_000_000 for m in members):
            raise ValueError('Unsafe archive member')
        files = {m.name: tar.extractfile(m).read() for m in members}
    metadata = json.loads(files['BUILD.json'])
    if metadata['source_commit'] != source or metadata['target'] != 'x86_64-unknown-linux-gnu':
        raise ValueError('Build source or architecture mismatch')
    if 'empty Cargo and target directories' not in metadata['build_policy']:
        raise ValueError('Clean build policy missing')
    if files['wpalt'][:6] != b'\x7fELF\x02\x01' or digest(files['wpalt']) != metadata['binary_sha256']:
        raise ValueError('Executable checksum or format mismatch')
    def source_bytes(path):
        return subprocess.check_output(['git', 'show', source + ':' + path], cwd=ROOT)
    if digest(source_bytes('Cargo.lock')) != metadata['lockfile_sha256']:
        raise ValueError('Dependency lock mismatch')
    frontend = {'lockfile_sha256':'frontend/package-lock.json', 'forms_sha256':'assets/generated/forms.js',
                'editor_sha256':'assets/generated/editor.js', 'studio_sha256':'assets/generated/builder.js',
                'admin_ui_sha256':'assets/generated/admin-ui.css', 'design_tokens_sha256':'frontend/design-tokens.json'}
    for key, path in frontend.items():
        if digest(source_bytes(path)) != metadata['frontend'][key]:
            raise ValueError('Frontend source mismatch: ' + path)
    for name in ['admin.js', 'app.css', 'engagement.js', 'engagement-review.js', 'form-embed.js']:
        if digest(source_bytes('assets/' + name)) != metadata['business_assets'][name]:
            raise ValueError('Business asset mismatch: ' + name)
    for name, path in [('wpalt.example.toml','wpalt.example.toml'),('operations.md','docs/operations.md'),
                       ('theme-authoring.md','docs/theme-authoring.md'),('membership-learning.md','docs/membership-learning.md'),
                       ('m5-contract.md','docs/m5-contract.md'),('commerce-reservations.md','docs/commerce-reservations.md'),('m6-contract.md','docs/m6-contract.md'),('resilient-operations.md','docs/resilient-operations.md'),('m7-contract.md','docs/m7-contract.md'),('migration-and-extensions.md','docs/migration-and-extensions.md'),('m8-contract.md','docs/m8-contract.md'),('THIRD_PARTY_NOTICES.md','THIRD_PARTY_NOTICES.md')]:
        if files[name] != source_bytes(path):
            raise ValueError('Packaged guide mismatch: ' + name)
    timestamp = int(command('git', 'show', '-s', '--format=%ct', source))
    tag = 'nightly-' + datetime.fromtimestamp(timestamp, timezone.utc).strftime('%Y%m%d%H%M') + '-' + source[:12]
    return archive, files, {'source_commit': source, 'tag': tag, 'archive': archive.name,
                           'archive_sha256': checksum, 'binary_sha256': metadata['binary_sha256'],
                           'binary_bytes': len(files['wpalt']), 'timestamp_basis': 'commit time, UTC'}


def publish(directory, archive, plan):
    # Never publish from PR execution, even if invoked with a write token by mistake.
    if os.environ.get('GITHUB_EVENT_NAME') != 'push' or os.environ.get('GITHUB_REF') != 'refs/heads/main':
        raise ValueError('Publication requires verified main push execution')
    if os.environ.get('GITHUB_SHA') != plan['source_commit']:
        raise ValueError('Workflow/source mismatch')
    tag = plan['tag']
    notes = directory / 'RELEASE.md'
    notes.write_text(f"# Development build {tag}\n\nSource `{plan['source_commit']}`.\n\n"
                     f"All application, compiler-floor, dependency-audit, frontend, native-PITR and clean-build gates passed. "
                     f"The Linux executable was built without restored dependency/build caches and tested through install/recovery and cumulative browser journeys.\n\n"
                     f"Archive SHA-256: `{plan['archive_sha256']}`. Binary SHA-256: `{plan['binary_sha256']}`.\n\n"
                     "Pre-adoption development prerelease; Linux x86_64/glibc (Ubuntu 24.04), not a production support promise. "
                     "See bundled operations and migration guides. No automatic database upgrade or deployment occurs.\n")
    listing = json.loads(command('gh', 'api', '--paginate', '--slurp', 'repos/{owner}/{repo}/releases'))
    existing = next((r for page in listing for r in page if r['tag_name'] == tag), None)
    remote_tag = command('git', 'ls-remote', 'origin', 'refs/tags/' + tag)
    if remote_tag and remote_tag.split()[0] != plan['source_commit']:
        raise ValueError('Existing release tag points to different source')
    if existing:
        if existing['draft'] and existing['target_commitish'] != plan['source_commit']:
            raise ValueError('Existing draft targets different source')
        if not existing['draft']:
            with tempfile.TemporaryDirectory() as temporary:
                subprocess.run(['gh', 'release', 'download', tag, '--dir', temporary], cwd=ROOT, check=True)
                try:
                    _, _, published = verify(Path(temporary), plan['source_commit'])
                except (ValueError, KeyError, OSError, tarfile.TarError) as error:
                    raise ValueError('Published release differs; refusing overwrite') from error
                if published['binary_sha256'] != plan['binary_sha256']:
                    raise ValueError('Published executable differs; refusing overwrite')
            print('Existing published release verified:', tag)
            return
    else:
        subprocess.run(['gh', 'release', 'create', tag, '--target', plan['source_commit'], '--draft',
                        '--prerelease', '--latest=false', '--title', tag, '--notes-file', str(notes)], cwd=ROOT, check=True)
    # Draft retries may replace incomplete uploads; published releases are immutable here.
    subprocess.run(['gh', 'release', 'upload', tag, str(archive), str(directory / 'SHA256SUMS'), '--clobber'], cwd=ROOT, check=True)
    subprocess.run(['gh', 'release', 'edit', tag, '--draft=false', '--prerelease', '--latest=false'], cwd=ROOT, check=True)
    print('Published:', tag)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--extract', type=Path)
    parser.add_argument('--publish', action='store_true')
    args = parser.parse_args()
    archive, files, plan = verify(args.directory, args.source)
    print(json.dumps(plan, indent=2))
    if args.extract:
        args.extract.mkdir(parents=True, exist_ok=False)
        for name, data in files.items():
            path = args.extract / name
            path.write_bytes(data)
            path.chmod(0o755 if name == 'wpalt' else 0o644)
    if args.publish:
        publish(args.directory, archive, plan)


if __name__ == '__main__':
    main()
