#!/usr/bin/env python3
"""Fail release checks on missing/stale evidence or overdue compatibility bridges.
Does not fetch or claim semantic correctness of external guidelines.
"""
import argparse
import datetime as dt
import json
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('--as-of',type=dt.date.fromisoformat,default=dt.date.today())
args=parser.parse_args()
root=Path(__file__).resolve().parents[1]
records=json.loads((root/'docs/evidence/feature-guidance.json').read_text())['records']
required={'feature_id','area','scope','basis','sources','applicable_versions','last_checked','next_review','review_interval_days','maintenance_owner','requirements','tests','triggers','status'}
ids=set()
for r in records:
    assert required<=r.keys(),f"Missing evidence fields for {r.get('feature_id')}"
    assert r['feature_id'] not in ids,'Duplicate feature evidence ID'
    ids.add(r['feature_id'])
    checked=dt.date.fromisoformat(r['last_checked']);due=dt.date.fromisoformat(r['next_review'])
    assert checked<=due,f"Invalid review dates: {r['feature_id']}"
    assert args.as_of<=due,f"Evidence needs review: {r['feature_id']} (due {due})"
    assert r['sources'] and all(s.startswith('https://') for s in r['sources']),f"Missing authoritative links: {r['feature_id']}"
    assert r['requirements'] and r['tests'] and r['maintenance_owner'],f"Incomplete mappings: {r['feature_id']}"
for layer in json.loads((root/'docs/evidence/compatibility-register.json').read_text())['layers']:
    assert {'id','owner','reason','supported_versions','migration_path','removal_release','review_deadline'}<=layer.keys(),'Incomplete compatibility-layer record'
    assert args.as_of<=dt.date.fromisoformat(layer['review_deadline']),f"Compatibility layer needs removal/review: {layer['id']}"
print(f"PASS: {len(records)} feature evidence records current as of {args.as_of}; no overdue compatibility layers.")
