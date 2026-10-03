"""Disclosed loopback HTTPS OIDC test adapter; never use this as a real provider."""
import base64, hashlib, http.server, json, ssl, sys, urllib.parse
from pathlib import Path
spec_file, certificate, key = map(Path, sys.argv[1:])
class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass  # Tokens, authorization codes and credentials never enter test logs.
    def reply(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def do_GET(self):
        spec = json.loads(spec_file.read_text())
        if self.path != '/keys':
            return self.reply(404, {})
        self.reply(200, spec['keys'])
    def do_POST(self):
        length = int(self.headers.get('Content-Length', '0'))
        if self.path != '/token' or not 0 < length <= 16384:
            return self.reply(400, {})
        fields = urllib.parse.parse_qs(self.rfile.read(length).decode())
        spec = json.loads(spec_file.read_text())
        verifier = fields.get('code_verifier', [''])[0]
        challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip('=')
        expected = {'grant_type':'authorization_code','client_id':spec['client_id'], 'code':'fixture-code', 'redirect_uri':spec['callback']}
        authorization = self.headers.get('Authorization', '')
        try:
            credentials = base64.b64decode(authorization.removeprefix('Basic '), validate=True).decode().split(':', 1)
            credentials = [urllib.parse.unquote_plus(v) for v in credentials]
        except (ValueError, UnicodeError):
            credentials = []
        if credentials != [spec['client_id'], spec['secret']] or any(fields.get(k) != [v] for k,v in expected.items()) or challenge != spec['challenge']:
            return self.reply(400, {'error':'invalid_fixture_exchange'})
        self.reply(200, {'id_token':spec['token'], 'access_token':'fixture-access'})
server = http.server.HTTPServer(('127.0.0.1', 0), Provider)
tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
tls.load_cert_chain(certificate, key)
server.socket = tls.wrap_socket(server.socket, server_side=True)
print(server.server_port, flush=True)
server.serve_forever()
