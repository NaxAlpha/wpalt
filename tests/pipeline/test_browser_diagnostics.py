"""Browser failure artifacts preserve correlation without publishing raw server data."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class BrowserFailureEvidence(unittest.TestCase):
    def test_secrets_are_excluded_and_truncated_records_are_not_interpreted(self):
        secret = 'private-email@example.test password=NEVER-PUBLISH token=private-token'
        request = '12345678-1234-1234-1234-123456789abc'
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'server.log'
            # Deliberately put a confidential record across the tail boundary.
            log.write_text(json.dumps({'fields': {'event': 'request_completed', 'message': secret,
                                                 'padding': 'x' * (2 * 1024 * 1024)}}) + '\n')
            with log.open('a') as stream:
                for i in range(600):
                    stream.write(json.dumps({'fields': {'event': 'request_completed', 'status': 200,
                                                       'elapsed_us': i, 'body': secret}}) + '\n')
                stream.write(json.dumps({'fields': {'event': 'database_operation_failed',
                                                   'error_code': '517', 'message': secret,
                                                   'sql': secret},
                                         'spans': [{'request_id': request, 'route': '/admin/shop/products/{id}',
                                                    'method': 'POST', 'secret': secret}]}) + '\n')
                stream.write(json.dumps({'fields': {'event': 'user_supplied_event', 'request_id': secret}}) + '\n')
                stream.write('not JSON ' + secret + '\n')
            result = subprocess.run(['node', '-e',
                'process.stdout.write(JSON.stringify(require(process.argv[1]).collect(process.argv[2])))',
                str(ROOT / 'scripts/browser_diagnostics.cjs'), directory],
                check=True, capture_output=True, text=True)
            self.assertNotIn(secret, result.stdout)
            report = json.loads(result.stdout)
            source = report['sources'][0]
            self.assertTrue(source['truncated'])
            self.assertEqual(len(source['events']), 500)
            self.assertEqual(source['events'][-1], {'event': 'database_operation_failed',
                'request_id': request, 'route': '/admin/shop/products/{id}', 'method': 'POST', 'error_code': '517'})
