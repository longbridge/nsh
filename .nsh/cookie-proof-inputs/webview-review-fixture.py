"""Temporary HTTP fixture for the native WebView cookie smoke.

Serves the same small page on every path so three web views can load, and
lets the page post a message once ready. It holds no cookie logic: the
smoke exercises the native stores.
"""

import http.server
import urllib.parse

PAGE = b"""<!doctype html><meta charset="utf-8"><title>cookie fixture</title>
<style>html,body{margin:0;height:100%;background:rgb(24,48,72);color:#fff;font:16px sans-serif}</style>
<p>cookie fixture</p>
<script>
addEventListener('load', function () {
  if (window.nsh && window.nsh.postMessage) window.nsh.postMessage('ready');
});
</script>
"""


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        # Every fixture path loads the page; the smoke uses paths such as
        # /a, /b, /c, /deep/x and /other as cookie-filter URLs.
        path = urllib.parse.urlsplit(self.path).path
        body = PAGE
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, fmt, *args):
        print(fmt % args, flush=True)


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
server.daemon_threads = True
print("FIXTURE_URL=http://127.0.0.1:" + str(server.server_port), flush=True)
server.serve_forever()
