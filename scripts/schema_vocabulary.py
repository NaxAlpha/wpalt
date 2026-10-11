#!/usr/bin/env python3
"""Reproduce the attributed pinned offline vocabulary; never fetch at runtime."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE_SHA256 = '0a4b8c4c910fc831ec8695196510973390906a5ee34dc6b862dfec4d21a12f74'


def many(value):
    return value if isinstance(value, list) else ([] if value is None else [value])


def ids(value):
    return sorted({v['@id'][7:] for v in many(value)
                   if isinstance(v, dict) and v.get('@id', '').startswith('schema:')})


def generate(raw):
    if len(raw) > 2 * 1024 * 1024 or hashlib.sha256(raw).hexdigest() != SOURCE_SHA256:
        raise ValueError('Vocabulary source size/hash mismatch')
    graph = json.loads(raw)['@graph']
    registry = {'release': '30.1', 'source_sha256': SOURCE_SHA256,
                'license': 'CC-BY-SA-3.0; Schema.org vocabulary data',
                'types': {}, 'properties': {}, 'enumerations': {}}
    for node in graph:
        identity = node.get('@id', '')
        if not identity.startswith('schema:'):
            continue
        name = identity[7:]
        if not name.isascii() or not name.isalnum() or len(name) > 100:
            raise ValueError('Invalid pinned term identity')
        kind = many(node.get('@type'))
        info = {'pending': node.get('schema:isPartOf', {}).get('@id') == 'https://pending.schema.org',
                'superseded_by': ids(node.get('schema:supersededBy'))}
        if 'rdfs:Class' in kind:
            family = 'types'
            info.update(parents=ids(node.get('rdfs:subClassOf')), datatype='schema:DataType' in kind)
        elif 'rdf:Property' in kind:
            family = 'properties'
            info.update(domains=ids(node.get('schema:domainIncludes')),
                        ranges=ids(node.get('schema:rangeIncludes')),
                        parents=ids(node.get('rdfs:subPropertyOf')))
        else:
            family = 'enumerations'
            info['types'] = sorted({v[7:] for v in kind if isinstance(v, str) and v.startswith('schema:')})
            if not info['types']:
                raise ValueError('Unknown pinned enumeration type')
        if name in registry[family]:
            raise ValueError('Duplicate pinned vocabulary identity')
        registry[family][name] = info
    all_names = set().union(*(set(registry[k]) for k in ['types', 'properties', 'enumerations']))
    for family in ['types', 'properties', 'enumerations']:
        for term in registry[family].values():
            for field in ['parents', 'domains', 'ranges', 'types', 'superseded_by']:
                if not set(term.get(field, [])) <= all_names:
                    raise ValueError('Dangling pinned vocabulary reference')
    def ancestors(name, seen=frozenset()):
        if name in seen:
            raise ValueError('Pinned type hierarchy contains a cycle')
        answer = {name}
        for parent in registry['types'][name]['parents']:
            answer |= ancestors(parent, seen | {name})
        return answer
    for name in registry['types']:
        ancestors(name)
    return json.dumps(registry, sort_keys=True, separators=(',', ':')).encode() + b'\n'


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--check', action='store_true', help='Verify generated data without changing it')
    a = p.parse_args()
    data = generate((ROOT / 'docs/research/schemaorg-30.1.jsonld').read_bytes())
    target = ROOT / 'assets/schemaorg-v30.1.json'
    manifest = json.loads((ROOT / 'docs/research/d07-vocabulary-provenance.json').read_text())
    if hashlib.sha256(data).hexdigest() != manifest['generated_sha256']:
        raise ValueError('Pinned generation provenance mismatch')
    if a.check:
        if target.read_bytes() != data:
            raise ValueError('Committed vocabulary differs from pinned generation')
    else:
        target.write_bytes(data)
    print('Verified pinned offline vocabulary:', len(data), 'bytes')


if __name__ == '__main__':
    main()
