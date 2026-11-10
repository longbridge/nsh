import collections
import http.server
import json
import threading
import time
import urllib.parse

hits = collections.Counter()
lock = threading.Lock()
payload = b'NSH native WebView download fixture\n'
page = b'''<!doctype html><meta charset="utf-8"><style>
html,body{margin:0;background:rgb(20,40,60);height:100%;overflow:hidden}
#marker{position:absolute;left:20px;top:20px;width:80px;height:60px;background:rgb(0,200,80)}
.ad{position:absolute;left:140px;top:20px;width:80px;height:60px;background:rgb(240,20,20)}
</style><div id="marker"></div><div class="ad"></div><script>
addEventListener('load',()=>{window.nsh.postMessage('ready');});
</script>'''

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        path = urllib.parse.urlsplit(self.path).path
        with lock:
            hits[path] += 1
        if path == '/counts':
            with lock:
                data = json.dumps(dict(hits)).encode()
            kind = 'application/json'
        elif path in ('/download', '/slow-download'):
            data = payload if path == '/download' else payload * 100000
            kind = 'application/octet-stream'
        elif path == '/blocked' or path == '/allowed':
            data = b'fixture-resource'
            kind = 'text/plain'
        else:
            data = page
            kind = 'text/html; charset=utf-8'
        self.send_response(200)
        self.send_header('Content-Type', kind)
        self.send_header('Content-Length', str(len(data)))
        self.send_header('Cache-Control', 'no-store')
        if path in ('/download', '/slow-download'):
            self.send_header('Content-Disposition', 'attachment; filename="fixture.txt"')
        self.end_headers()
        try:
            if path == '/slow-download':
                for i in range(0, len(data), 4096):
                    self.wfile.write(data[i:i+4096])
                    self.wfile.flush()
                    time.sleep(0.03)
            else:
                self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, fmt, *args):
        print(fmt % args, flush=True)

server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
server.daemon_threads = True
print('FIXTURE_URL=http://127.0.0.1:' + str(server.server_port), flush=True)
server.serve_forever()
