import http.server
import time
import urllib.parse

PAYLOAD = b"NSH native WebView download fixture\n"
SLOW_PAYLOAD = b"nsh-slow-download-" * (4 * 1024 * 1024 // 18)

PAGE = b"""<!doctype html><meta charset="utf-8">
<title>NSH download fixture</title>
<p id="ready">ready</p>
"""


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        path = urllib.parse.urlsplit(self.path).path
        if path == "/download":
            data = PAYLOAD
            kind = "application/octet-stream"
        elif path == "/slow-download":
            data = SLOW_PAYLOAD
            kind = "application/octet-stream"
        else:
            data = PAGE
            kind = "text/html; charset=utf-8"

        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        if path in ("/download", "/slow-download"):
            self.send_header(
                "Content-Disposition", 'attachment; filename="fixture.txt"'
            )
        self.end_headers()
        try:
            if path == "/slow-download":
                # Keep writing long enough that an accepted transfer is still
                # active when the smoke cancels or tears its view down.
                for at in range(0, len(data), 4096):
                    self.wfile.write(data[at : at + 4096])
                    self.wfile.flush()
                    time.sleep(0.02)
            else:
                self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, fmt, *args):
        print(fmt % args, flush=True)


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
server.daemon_threads = True
print("FIXTURE_URL=http://127.0.0.1:" + str(server.server_port), flush=True)
server.serve_forever()
