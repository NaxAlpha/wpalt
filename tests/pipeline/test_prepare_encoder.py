"""Build dependency admission, reproducibility and pipeline ordering contracts."""
import hashlib
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('prepare_encoder', ROOT / 'scripts/prepare_encoder.py')
encoder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(encoder)


def archive(entries):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode='w:gz') as stream:
        for name, value in entries.items():
            entry = tarfile.TarInfo('rav1e-0.8.1/' + name)
            if value is None:
                entry.type = tarfile.SYMTYPE
                entry.linkname = '/etc/passwd'
                stream.addfile(entry)
            else:
                entry.size = len(value)
                stream.addfile(entry, io.BytesIO(value))
    data = output.getvalue()
    return data, hashlib.sha256(data).hexdigest()


class EncoderPreparation(unittest.TestCase):
    def fixture(self):
        return {
            'Cargo.toml': b'[dependencies.paste]\nversion = "1.0"',
            'Cargo.toml.orig': b'paste = "1.0"',
            'src/predict.rs': b'paste::item! {}\nfn p() -> (i32, i32, PlaneSlice<T>) {}',
            'src/tiling/tile_restoration_state.rs': b'pub const fn as_const(&self) -> TileRestorationState {}',
            'LICENSE': b'Preserve upstream license verbatim',
            'src/x86/example.asm': b'unchanged assembly',
        }

    def test_checksum_and_archive_boundaries_reject_untrusted_input(self):
        data, checksum = archive(self.fixture())
        with self.assertRaises(ValueError):
            encoder.patched_source(data + b'changed', checksum)
        for unsafe in ['../escape', '/absolute', 'src/../../escape', 'src\\escape', 'link']:
            entries = self.fixture()
            entries[unsafe] = None if unsafe == 'link' else b'bad'
            data, checksum = archive(entries)
            with self.subTest(unsafe=unsafe), self.assertRaises(ValueError):
                encoder.patched_source(data, checksum)
        entries = self.fixture()
        entries['Cargo.toml'] = b'different upstream dependency'
        data, checksum = archive(entries)
        with self.assertRaises(ValueError):
            encoder.patched_source(data, checksum)

    def test_exact_patch_is_repeatable_and_cached_tampering_fails_closed(self):
        original = self.fixture()
        data, checksum = archive(original)
        expected = encoder.patched_source(data, checksum)
        self.assertEqual(expected['LICENSE'], original['LICENSE'])
        self.assertEqual(expected['src/x86/example.asm'], original['src/x86/example.asm'])
        self.assertIn(b'pastey::', expected['src/predict.rs'])
        self.assertIn(b"PlaneSlice<'_, T>", expected['src/predict.rs'])
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / 'vendor/rav1e'
            encoder.materialize(destination, expected)
            encoder.materialize(destination, expected)
            (destination / 'LICENSE').write_bytes(b'tampered')
            with self.assertRaises(ValueError):
                encoder.materialize(destination, expected)
            self.assertEqual((destination / 'LICENSE').read_bytes(), b'tampered')
            (destination / 'LICENSE').write_bytes(expected['LICENSE'])
            (destination / 'unexpected.rs').write_bytes(b'extra code')
            with self.assertRaises(ValueError):
                encoder.materialize(destination, expected)
            (destination / 'unexpected.rs').unlink()
            (destination / 'LICENSE').unlink()
            (destination / 'LICENSE').symlink_to('/etc/passwd')
            with self.assertRaises(ValueError):
                encoder.materialize(destination, expected)

    def test_each_rust_ci_job_prepares_source_before_cargo_or_cache(self):
        workflow = (ROOT / '.github/workflows/m1.yml').read_text()
        for job in ['application', 'clean-build', 'compiler-floor', 'dependency-audit']:
            section = workflow.split('\n  ' + job + ':', 1)[1]
            import re
            section = re.split(r'\n  [a-z][a-z-]*:', section, maxsplit=1)[0]
            prepare = section.index('python3 scripts/prepare_encoder.py')
            cargo = section.find('run: cargo ')
            if cargo >= 0:
                self.assertLess(prepare, cargo, job)
            cache = section.find('uses: Swatinem/rust-cache@')
            if cache >= 0:
                self.assertLess(prepare, cache, job)


if __name__ == '__main__':
    unittest.main()
