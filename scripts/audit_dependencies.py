#!/usr/bin/env python3
"""Audit the lockfile and distinguish compiled dependencies from inactive optional entries.
Fail on advisories in the selected all-target normal/build/dev graph. No blanket ignore list.
"""
import argparse,json,re,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--audit-binary',default='cargo-audit');p.add_argument('--output',default='work/dependency-audit.json');args=p.parse_args()
root=Path(__file__).resolve().parents[1]
raw=subprocess.run([args.audit_binary,'audit','--db','work/advisory-db','--json'],cwd=root,capture_output=True,text=True)
try:report=json.loads(raw.stdout)
except json.JSONDecodeError:raise SystemExit('Advisory audit failed before producing a valid report: '+raw.stderr)
if 'vulnerabilities' not in report:raise SystemExit('Incomplete advisory report')
tree=subprocess.check_output(['cargo','tree','--locked','--target','all','--edges','normal,build,dev','--prefix','none','--format','{p}'],cwd=root,text=True)
selected=set()
for line in tree.splitlines():
    match=re.match(r'([^ ]+) v([^ ]+)',line)
    if match:selected.add(match.groups())
active=[];inactive=[]
for finding in report['vulnerabilities']['list']:
    package=finding['package'];entry={'advisory':finding['advisory']['id'],'package':package['name'],'version':package['version'],'title':finding['advisory']['title']}
    (active if (package['name'],package['version']) in selected else inactive).append(entry)
active_warnings=[]
for kind,findings in report.get('warnings',{}).items():
    for finding in findings:
        package=finding['package']
        if (package['name'],package['version']) in selected:active_warnings.append({'kind':kind,'package':package['name'],'version':package['version']})
output=root/args.output;output.parent.mkdir(parents=True,exist_ok=True)
output.write_text(json.dumps({'advisory_database':report.get('database'),'active_vulnerabilities':active,'inactive_optional_lockfile_findings':inactive,'active_warnings':active_warnings,'selection':'cargo tree --target all --edges normal,build,dev (current wpalt features)'},indent=2)+'\n')
print(f'Audit: {len(active)} selected-graph vulnerabilities, {len(active_warnings)} selected-graph warnings; {len(inactive)} inactive optional lockfile findings.')
if active or active_warnings:raise SystemExit('Selected dependencies need remediation/review')
if raw.returncode not in (0,1):raise SystemExit('Audit process failed unexpectedly')
