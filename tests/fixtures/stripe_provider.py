"""Disclosed local HTTPS Stripe protocol fixture, never a live payment processor."""
import http.server, json, ssl, sys, urllib.parse
from pathlib import Path
spec_file, certificate, key = map(Path, sys.argv[1:])
class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def reply(self, status, body):
        data=json.dumps(body).encode(); self.send_response(status)
        self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)))
        self.end_headers();self.wfile.write(data)
    def handle_method(self, method):
        spec=json.loads(spec_file.read_text())
        if self.headers.get('Authorization')!='Bearer sk_fixture' or self.headers.get('Stripe-Version')!='2026-09-30.endive':
            return self.reply(401,{})
        entry=spec.get(method,{}).get(self.path)
        if not entry: return self.reply(404,{})
        if method=='POST':
            length=int(self.headers.get('Content-Length','0'))
            if not 0<length<=16384:return self.reply(400,{})
            fields=urllib.parse.parse_qs(self.rfile.read(length).decode())
            if self.headers.get('Idempotency-Key')!=entry['key'] or any(fields.get(k)!=[v] for k,v in entry.get('fields',{}).items()):
                return self.reply(400,{'error':'invalid_fixture_request'})
        self.reply(entry.get('status',200),entry['body'])
    def do_GET(self):self.handle_method('GET')
    def do_POST(self):self.handle_method('POST')
server=http.server.HTTPServer(('127.0.0.1',0),Provider)
tls=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);tls.load_cert_chain(certificate,key)
server.socket=tls.wrap_socket(server.socket,server_side=True)
print(server.server_port,flush=True);server.serve_forever()
