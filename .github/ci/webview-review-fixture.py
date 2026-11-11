import http.server

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        color = "90,70,30" if self.path.startswith("/b") else "20,40,60"
        body = ('<!doctype html><meta charset="utf-8"><style>html,body{margin:0;width:100%;height:100%;background:rgb(' + color + ')}</style>').encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print("FIXTURE_URL=http://127.0.0.1:" + str(server.server_port), flush=True)
server.serve_forever()
