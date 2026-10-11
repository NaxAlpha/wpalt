"""Bundled schema data is attributed and reproducible offline; altered input is refused."""
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('schema_vocabulary', ROOT / 'scripts/schema_vocabulary.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class SchemaVocabulary(unittest.TestCase):
    def test_offline_generation_preserves_ranges_status_and_refuses_changed_sources(self):
        raw = (ROOT / 'docs/research/schemaorg-30.1.jsonld').read_bytes()
        generated = MODULE.generate(raw)
        self.assertEqual(generated, (ROOT / 'assets/schemaorg-v30.1.json').read_bytes())
        data = json.loads(generated)
        self.assertEqual(data['license'], 'CC-BY-SA-3.0; Schema.org vocabulary data')
        self.assertIn('FoodEstablishment', data['types']['Restaurant']['parents'])
        self.assertIn('LocalBusiness', data['types']['FoodEstablishment']['parents'])
        self.assertIn('Person', data['properties']['author']['ranges'])
        self.assertIn('interactionStatistic', data['properties']['interactionCount']['superseded_by'])
        self.assertIn('DayOfWeek', data['enumerations']['Monday']['types'])
        for changed in [raw + b' ', b'{}', b'x' * (2 * 1024 * 1024 + 1)]:
            with self.assertRaises(ValueError):
                MODULE.generate(changed)
