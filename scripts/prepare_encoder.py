#!/usr/bin/env python3
"""Materialize a pinned, narrowly patched encoder before invoking Cargo.

Only build-time data is downloaded. Production never uses this script.
"""
import argparse
import errno
import hashlib
import io
import os
from pathlib import Path, PurePosixPath
import tarfile
import tempfile
import urllib.request

VERSION = '0.8.1'
SHA256 = '43b6dd56e85d9483277cde964fd1bdb0428de4fec5ebba7540995639a21cb32b'
URL = f'https://static.crates.io/crates/rav1e/rav1e-{VERSION}.crate'
MAX_ARCHIVE = 16 * 1024 * 1024
MAX_SOURCE = 32 * 1024 * 1024
ROOT = Path(__file__).resolve().parents[1]


def patched_source(archive, checksum=SHA256):
    if len(archive) > MAX_ARCHIVE or hashlib.sha256(archive).hexdigest() != checksum:
        raise ValueError('Encoder archive checksum or size mismatch')
    files = {}
    total = 0
    entries = 0
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:gz') as source:
        for member in source:
            entries += 1
            if entries > 2000:
                raise ValueError("Too many encoder archive entries")
            parts = PurePosixPath(member.name).parts
            if (not parts or parts[0] != f'rav1e-{VERSION}' or
                    any(p in ('', '.', '..') for p in member.name.rstrip('/').split('/')) or
                    '\\' in member.name or member.name.startswith('/') or
                    not (member.isfile() or member.isdir())):
                raise ValueError('Unsafe encoder archive entry')
            if member.isdir():
                continue
            relative = '/'.join(parts[1:])
            total += member.size
            if not relative or relative in files or total > MAX_SOURCE or len(files) >= 2000:
                raise ValueError('Duplicate or oversized encoder archive')
            data = source.extractfile(member).read()
            files[relative] = data.replace(b'paste::', b'pastey::') if relative.endswith('.rs') else data
    changes = {
        'Cargo.toml': (b'[dependencies.paste]\nversion = "1.0"', b'[dependencies.pastey]\nversion = "0.1.0"'),
        'Cargo.toml.orig': (b'paste = "1.0"', b'pastey = "0.1.0"'),
        'src/predict.rs': (b') -> (i32, i32, PlaneSlice<T>)', b") -> (i32, i32, PlaneSlice<'_, T>)"),
        'src/tiling/tile_restoration_state.rs': (b'pub const fn as_const(&self) -> TileRestorationState {', b"pub const fn as_const(&self) -> TileRestorationState<'_> {"),
    }
    for path, (before, after) in changes.items():
        if path not in files or files[path].count(before) != 1:
            raise ValueError('Upstream maintenance patch no longer applies: ' + path)
        files[path] = files[path].replace(before, after)
    return files


def verify_tree(destination, expected):
    if destination.is_symlink() or not destination.is_dir():
        raise ValueError('Encoder destination is not a regular directory')
    actual = {}
    for path in destination.rglob('*'):
        if path.is_symlink() or not (path.is_dir() or path.is_file()):
            raise ValueError('Unsafe generated encoder file')
        if path.is_file():
            name = path.relative_to(destination).as_posix()
            if name not in expected or path.stat().st_size != len(expected[name]):
                raise ValueError('Unexpected generated encoder file: ' + name)
            actual[name] = path.read_bytes()
    if actual != expected:
        raise ValueError('Generated encoder source changed; remove vendor/rav1e and rerun preparation')


def materialize(destination, expected):
    if destination.exists() or destination.is_symlink():
        verify_tree(destination, expected)
        return
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.encoder-', dir=destination.parent) as staging:
        tree = Path(staging) / 'source'
        tree.mkdir()
        for name, data in expected.items():
            path = tree / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        verify_tree(tree, expected)
        try:
            os.rename(tree, destination)
        except OSError as error:
            if error.errno not in (errno.EEXIST, errno.ENOTEMPTY):
                raise
            # Another preparation completed while this process staged its files.
            verify_tree(destination, expected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, help='Use a predownloaded checksummed archive (no network)')
    args = parser.parse_args()
    cache = args.archive or ROOT / 'work/build-dependencies' / f'rav1e-{VERSION}.crate'
    if cache.is_symlink():
        raise ValueError('Archive cache must not be a symlink')
    if cache.exists():
        with cache.open('rb') as stream:
            archive = stream.read(MAX_ARCHIVE + 1)
    elif args.archive:
        raise ValueError('Offline encoder archive does not exist')
    else:
        with urllib.request.urlopen(URL, timeout=30) as response:
            archive = response.read(MAX_ARCHIVE + 1)
    expected = patched_source(archive)
    if not cache.exists():
        cache.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(dir=cache.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(archive)
        try:
            os.replace(temporary, cache)
        finally:
            temporary.unlink(missing_ok=True)
    materialize(ROOT / 'vendor/rav1e', expected)
    print(f'PASS: prepared/verified rav1e {VERSION}; pinned archive SHA-256 {SHA256}; exact maintenance patch only.')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, tarfile.TarError) as error:
        raise SystemExit('Encoder preparation failed: ' + str(error))
