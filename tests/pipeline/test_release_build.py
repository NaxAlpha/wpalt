"""Delivery safety: real archive verification, tamper rejection and publish guards."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('release_build', ROOT / 'scripts/release_build.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class Delivery(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name)
        self.source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
        def data(path):
            return subprocess.check_output(['git', 'show', self.source + ':' + path], cwd=ROOT)
        digest = release.digest
        self.files = {name: data(path) for name, path in [
            ('wpalt.example.toml','wpalt.example.toml'), ('operations.md','docs/operations.md'),
            ('theme-authoring.md','docs/theme-authoring.md'), ('membership-learning.md','docs/membership-learning.md'),
            ('m5-contract.md','docs/m5-contract.md'),('commerce-reservations.md','docs/commerce-reservations.md'),('m6-contract.md','docs/m6-contract.md'),('resilient-operations.md','docs/resilient-operations.md'),('m7-contract.md','docs/m7-contract.md'),('migration-and-extensions.md','docs/migration-and-extensions.md'),('m8-contract.md','docs/m8-contract.md'), ('local_ai_worker.py','examples/integrations/local_ai_worker.py'), ('integration-example.md','examples/integrations/README.md'), ('THIRD_PARTY_NOTICES.md','THIRD_PARTY_NOTICES.md')]}
        self.files.update({'wpalt': b'\x7fELF\x02\x01disclosed-test-executable', 'INSTALL.md': b'Fixture guide'})
        metadata = {'source_commit': self.source, 'target':'x86_64-unknown-linux-gnu',
                    'build_policy':'empty Cargo and target directories', 'binary_sha256':digest(self.files['wpalt']),
                    'lockfile_sha256':digest(data('Cargo.lock')), 'frontend':{}, 'business_assets':{}}
        for key, path in {'lockfile_sha256':'frontend/package-lock.json', 'forms_sha256':'assets/generated/forms.js',
                          'editor_sha256':'assets/generated/editor.js', 'studio_sha256':'assets/generated/builder.js',
                          'admin_ui_sha256':'assets/generated/admin-ui.css', 'design_tokens_sha256':'frontend/design-tokens.json'}.items():
            metadata['frontend'][key] = digest(data(path))
        for name in ['admin.js','app.css','engagement.js','engagement-review.js','form-embed.js']:
            metadata['business_assets'][name] = digest(data('assets/' + name))
        self.metadata = metadata
        self.files['BUILD.json'] = json.dumps(metadata).encode()
        self.write_archive()

    def write_archive(self, extra=None):
        archive = self.path / 'fixture.tar.gz'
        with tarfile.open(archive, 'w:gz') as tar:
            for name, body in self.files.items():
                entry = tarfile.TarInfo(name); entry.size = len(body)
                tar.addfile(entry, io.BytesIO(body))
            if extra:
                entry = tarfile.TarInfo(extra); entry.size = 1
                tar.addfile(entry, io.BytesIO(b'x'))
        (self.path / 'SHA256SUMS').write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  fixture.tar.gz\n')

    def test_exact_source_package_and_retry_tag_are_stable(self):
        first = release.verify(self.path, self.source)[2]
        second = release.verify(self.path, self.source)[2]
        self.assertEqual(first, second)
        self.assertRegex(first['tag'], r'^nightly-\d{12}-[0-9a-f]{12}$')

    def test_corrupt_download_rejected_before_extraction(self):
        with (self.path / 'fixture.tar.gz').open('ab') as archive: archive.write(b'tamper')
        with self.assertRaisesRegex(ValueError, 'checksum'): release.verify(self.path, self.source)

    def test_valid_checksum_cannot_hide_wrong_source_or_modified_guide(self):
        self.metadata['source_commit'] = '0' * 40
        self.files['BUILD.json'] = json.dumps(self.metadata).encode(); self.write_archive()
        with self.assertRaisesRegex(ValueError, 'source'): release.verify(self.path, self.source)
        self.metadata['source_commit'] = self.source
        self.files['BUILD.json'] = json.dumps(self.metadata).encode()
        self.files['operations.md'] = b'Altered guide'; self.write_archive()
        with self.assertRaisesRegex(ValueError, 'guide'): release.verify(self.path, self.source)

    def test_traversal_member_rejected_before_writes(self):
        self.write_archive('../outside')
        with self.assertRaisesRegex(ValueError, 'members'): release.verify(self.path, self.source)
        self.assertFalse((self.path.parent / 'outside').exists())

    def test_pr_execution_cannot_publish_even_with_publish_flag(self):
        archive, _, plan = release.verify(self.path, self.source)
        with patch.dict('os.environ', {'GITHUB_EVENT_NAME':'pull_request','GITHUB_REF':'refs/pull/11/merge'}, clear=True):
            with self.assertRaisesRegex(ValueError, 'main push'): release.publish(self.path, archive, plan)

    def test_interrupted_draft_resumes_upload_then_publish_without_duplicate_creation(self):
        archive, _, plan = release.verify(self.path, self.source)
        existing = [[{'tag_name':plan['tag'], 'draft':True, 'target_commitish':self.source}]]
        with patch.dict('os.environ', {'GITHUB_EVENT_NAME':'push','GITHUB_REF':'refs/heads/main','GITHUB_SHA':self.source}, clear=True):
            with patch.object(release, 'command', side_effect=[json.dumps(existing), '']):
                with patch.object(release.subprocess, 'run') as calls:
                    release.publish(self.path, archive, plan)
                    self.assertEqual([c.args[0][2] for c in calls.call_args_list], ['upload','edit'])
                    self.assertIn('--draft=false', calls.call_args_list[-1].args[0])

    def test_published_asset_mismatch_fails_without_upload_or_edit(self):
        archive, _, plan = release.verify(self.path, self.source)
        existing = [[{'tag_name':plan['tag'], 'draft':False}]]
        def download(args, **kwargs):
            destination = Path(args[args.index('--dir') + 1])
            (destination / archive.name).write_bytes(b'mismatched published asset')
        with patch.dict('os.environ', {'GITHUB_EVENT_NAME':'push','GITHUB_REF':'refs/heads/main','GITHUB_SHA':self.source}, clear=True):
            with patch.object(release, 'command', side_effect=[json.dumps(existing), self.source + '\trefs/tags/' + plan['tag']]):
                with patch.object(release.subprocess, 'run', side_effect=download) as calls:
                    with self.assertRaisesRegex(ValueError, 'refusing overwrite'):
                        release.publish(self.path, archive, plan)
                    self.assertEqual(len(calls.call_args_list), 1)
                    self.assertEqual(calls.call_args.args[0][2], 'download')

    def test_valid_published_source_survives_repackaging_without_overwriting_assets(self):
        import shutil
        published = self.path / 'published'
        published.mkdir()
        for name in ['fixture.tar.gz', 'SHA256SUMS']:
            shutil.copy2(self.path / name, published / name)
        self.metadata['workflow_attempt'] = '2'
        self.files['BUILD.json'] = json.dumps(self.metadata).encode(); self.write_archive()
        self.assertNotEqual((published / 'fixture.tar.gz').read_bytes(), (self.path / 'fixture.tar.gz').read_bytes())
        archive, _, plan = release.verify(self.path, self.source)
        existing = [[{'tag_name':plan['tag'], 'draft':False}]]
        original_command = release.command
        def gh_command(*args):
            if args[0] == 'gh': return json.dumps(existing)
            if args[:2] == ('git', 'ls-remote'): return self.source + '\trefs/tags/' + plan['tag']
            return original_command(*args)
        original_run = release.subprocess.run
        # Mock only the external release download; git source inspection remains real.
        def run(args, **kwargs):
            if args[0] != 'gh': return original_run(args, **kwargs)
            destination = Path(args[args.index('--dir') + 1])
            for name in ['fixture.tar.gz', 'SHA256SUMS']:
                shutil.copy2(published / name, destination / name)
        with patch.dict('os.environ', {'GITHUB_EVENT_NAME':'push','GITHUB_REF':'refs/heads/main','GITHUB_SHA':self.source}, clear=True):
            with patch.object(release, 'command', side_effect=gh_command):
                with patch.object(release.subprocess, 'run', side_effect=run) as calls:
                    release.publish(self.path, archive, plan)
                    gh_calls = [c for c in calls.call_args_list if c.args[0][0] == 'gh']
                    self.assertEqual(len(gh_calls), 1)
                    self.assertEqual(gh_calls[0].args[0][2], 'download')
