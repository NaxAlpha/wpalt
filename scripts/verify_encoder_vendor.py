#!/usr/bin/env python3
"""Verify the narrow encoder patch against the checksummed registry source.
No archive is extracted or executed. Every vendored byte must equal the original
plus these reviewed maintenance transformations; extra/missing files fail.
"""
import hashlib, io, tarfile, urllib.request
from pathlib import Path

EXPECTED='43b6dd56e85d9483277cde964fd1bdb0428de4fec5ebba7540995639a21cb32b'
root=Path(__file__).resolve().parents[1]
with urllib.request.urlopen('https://static.crates.io/crates/rav1e/rav1e-0.8.1.crate',timeout=30) as response:
    archive=response.read(16*1024*1024+1)
assert len(archive)<=16*1024*1024 and hashlib.sha256(archive).hexdigest()==EXPECTED,'Unexpected encoder registry source'
expected={};total=0
with tarfile.open(fileobj=io.BytesIO(archive),mode='r:gz') as source:
    for member in source:
        assert member.isdir() or member.isfile(),'Nonregular source archive entry'
        if not member.isfile():continue
        prefix='rav1e-0.8.1/'
        assert member.name.startswith(prefix)
        relative=member.name[len(prefix):]
        assert relative and not Path(relative).is_absolute() and '..' not in Path(relative).parts
        total+=member.size
        assert total<=32*1024*1024 and len(expected)<2000,'Oversized source archive'
        data=source.extractfile(member).read()
        if relative.endswith('.rs'):
            data=data.replace(b'paste::',b'pastey::')
        if relative=='Cargo.toml':data=data.replace(b'[dependencies.paste]\nversion = "1.0"',b'[dependencies.pastey]\nversion = "0.1.0"')
        if relative=='Cargo.toml.orig':data=data.replace(b'paste = "1.0"',b'pastey = "0.1.0"')
        if relative=='src/predict.rs':data=data.replace(b') -> (i32, i32, PlaneSlice<T>)',b") -> (i32, i32, PlaneSlice<'_, T>)")
        if relative=='src/tiling/tile_restoration_state.rs':data=data.replace(b'pub const fn as_const(&self) -> TileRestorationState {',b"pub const fn as_const(&self) -> TileRestorationState<'_> {")
        assert relative not in expected
        expected[relative]=data
actual={str(path.relative_to(root/'vendor/rav1e')):path.read_bytes() for path in (root/'vendor/rav1e').rglob('*') if path.is_file()}
assert actual.keys()==expected.keys(),'Missing/extra encoder source files'
for name,original in expected.items():assert actual[name]==original,'Unreviewed encoder change: '+name
print('PASS: checksummed rav1e 0.8.1 source; upstream pastey migration and two explicit lifetime annotations only.')
