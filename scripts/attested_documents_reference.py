#!/usr/bin/env python3
"""Actual isolated scanner/browser/document journey, with controlled intelligence."""
import argparse,hashlib,json,os,socket,subprocess,tempfile,time,uuid
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary',required=True);p.add_argument('--clamd',required=True);p.add_argument('--renderer',required=True);p.add_argument('--certs');p.add_argument('--output',default='work/d11-browser')
a=p.parse_args();root=Path(__file__).resolve().parents[1];out=Path(a.output).resolve();out.mkdir(parents=True,exist_ok=True)
endpoint=Path('/private/tmp' if os.uname().sysname=='Darwin' else '/tmp')/('wpalt-d11-'+uuid.uuid4().hex+'.sock')
marker=b'WPALT_CONTROLLED_SCANNER_FIXTURE_20261012'
with tempfile.TemporaryDirectory(prefix='wpalt-d11-scanner-') as temporary:
 directory=Path(temporary);database=directory/'database';database.mkdir()
 (database/'controlled.ndb').write_text('Wpalt.Controlled.Fixture:0:*:'+marker.hex()+'\n')
 cfg=directory/'clamd.conf';cfg.write_text(f'DatabaseDirectory {database}\nLocalSocket {endpoint}\nForeground yes\nStreamMaxLength 2M\nMaxThreads 2\nMaxQueue 4\nReadTimeout 5\nCommandReadTimeout 5\n'+(f'CVDCertsDirectory {Path(a.certs).resolve()}\n' if a.certs else ''))
 log=(out/'scanner.log').open('w');scanner=subprocess.Popen([str(Path(a.clamd).resolve()),'--config-file',str(cfg)],stdout=log,stderr=log)
 try:
  for i in range(200):
   assert scanner.poll() is None,'Isolated scanner exited before readiness; inspect scanner.log'
   if endpoint.exists():break
   time.sleep(.05)
  assert endpoint.exists(),'Isolated scanner readiness deadline exceeded'
  with socket.socket(socket.AF_UNIX) as client:
   client.settimeout(5);client.connect(str(endpoint));client.sendall(b'zVERSION\0');version=b''
   while not version.endswith(b'\0'):
    chunk=client.recv(256);assert chunk,'Scanner version truncated';version+=chunk;assert len(version)<=1024
  env=dict(os.environ,WPALT_DOCUMENTS_ONLY='1',WPALT_BINARY=str(Path(a.binary).resolve()),WPALT_TEST_CLAMD_SOCKET=str(endpoint),WPALT_TEST_PDF_CHROMIUM=str(Path(a.renderer).resolve()),WPALT_BROWSER_OUTPUT=str(out))
  subprocess.run(['node','scripts/browser_acceptance.cjs'],cwd=root,env=env,check=True,timeout=240)
  renderer=Path(a.renderer).resolve(); renderer_version=subprocess.check_output([str(renderer),'--version'],text=True,timeout=10).strip()[:512]
  receipt={'renderer':renderer_version,'renderer_binary_sha256':hashlib.sha256(renderer.read_bytes()).hexdigest(),'scanner':version.decode().rstrip('\0'),'scanner_binary_sha256':hashlib.sha256(Path(a.clamd).read_bytes()).hexdigest(),'scope':'Actual local ClamAV and Chromium/application integration with one harmless controlled signature. No current official intelligence, complete malware coverage, certified identity, physical-host or universal PDF font claim.'}
  (out/'d11-scanner-reference.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt))
 finally:
  scanner.terminate();scanner.wait(timeout=10);log.close();endpoint.unlink(missing_ok=True)
