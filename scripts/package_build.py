#!/usr/bin/env python3
"""Package a verified clean build; record source/toolchain and hash the exact archive."""
import argparse,hashlib,json,os,subprocess,tarfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--target',required=True);p.add_argument('--output',default='work/distribution');args=p.parse_args()
root=Path(__file__).resolve().parents[1];out=root/args.output;out.mkdir(parents=True,exist_ok=True)
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
metadata={'source_commit':sha,'target':args.target,'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'cargo':subprocess.check_output(['cargo','--version'],text=True).strip(),'lockfile_sha256':hashlib.sha256((root/'Cargo.lock').read_bytes()).hexdigest(),'binary_sha256':hashlib.sha256(Path(args.binary).read_bytes()).hexdigest(),'workflow_run':os.environ.get('GITHUB_RUN_ID'),'workflow_attempt':os.environ.get('GITHUB_RUN_ATTEMPT'),'build_policy':'fresh runner; empty Cargo and target directories; cargo build --release --locked; no restored build/dependency cache'}
(out/'BUILD.json').write_text(json.dumps(metadata,indent=2)+'\n')
(out/'INSTALL.md').write_text('''# wpalt development build

Extract this archive into a private directory. Run `./wpalt --version` and
`./wpalt --help`; copy `wpalt.example.toml` to your private configuration.
Initialize with a password through stdin, then serve. See operations.md.
Assets and SQLite are bundled. PostgreSQL and a TLS reverse proxy are optional
external infrastructure; non-local origins require HTTPS. The Linux build
requires a compatible glibc environment (built on Ubuntu 24.04), not Alpine musl.
This is a pre-adoption development artifact, not a supported production release.
License selection remains pending. BUILD.json identifies the exact source and
compiler. SHA256SUMS verifies archive integrity, not a signature or provenance attestation.
''')
archive=out/f'wpalt-{args.target}-{sha[:12]}.tar.gz'
with tarfile.open(archive,'w:gz') as tar:
 for source,name in [(Path(args.binary),'wpalt'),(root/'wpalt.example.toml','wpalt.example.toml'),(root/'docs/operations.md','operations.md'),(out/'BUILD.json','BUILD.json'),(out/'INSTALL.md','INSTALL.md')]:tar.add(source,arcname=name)
(out/'SHA256SUMS').write_text(hashlib.sha256(archive.read_bytes()).hexdigest()+'  '+archive.name+'\n')
print('Packaged clean source',sha,'as',archive.name)
