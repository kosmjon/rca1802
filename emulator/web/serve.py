"""Local dev server for the Elf II web front end.

Like `python -m http.server`, but sends `Cache-Control: no-store` so the
browser never serves a stale script, worker or WebAssembly build after a
rebuild. (GitHub Pages caching is unaffected; this is only for local use.)

Run from the `emulator` folder:
    python web/serve.py [PORT]      (default 1802, bound to 127.0.0.1)
"""
import sys
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer


class NoCacheHandler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store, must-revalidate")
        super().end_headers()


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 1802
    handler = partial(NoCacheHandler, directory="web")
    with ThreadingHTTPServer(("127.0.0.1", port), handler) as httpd:
        print(f"Serving web/ at http://127.0.0.1:{port}/ (no-store; Ctrl+C to stop)")
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    main()
