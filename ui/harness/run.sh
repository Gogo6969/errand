#!/bin/sh
# Serve the window and its stand-in, so a browser can run the real page.
#
# A test that needs a browser is not a test that runs in CI, and that is a real
# cost. It buys the only thing that catches a window which draws half of itself:
# actually drawing it.
#
# Nothing it serves may be yesterday's. The page busts its own imports by hand
# and that was never enough: `app.js` is fetched with a fresh query and then
# statically imports `./markdown.js`, `./icons.js` and `./speech.js` with none,
# so those came out of the browser's cache. `no-store` does not save it either
# -- the browser this is run in honours the header for `fetch` and ignores it
# for a module import, which was checked rather than assumed: the same URL gave
# one set of exports through `fetch` and an older set through `import`.
#
# So the query is put on for it, here, as the file goes out. A harness testing
# code from an edit ago is worse than no harness, and it does not fail as
# "stale" -- it failed as an export missing from a file that plainly had it.
set -e
cd "$(dirname "$0")/../.."
PORT="${PORT:-8792}"
python3 - "$PORT" <<'PY' >/dev/null 2>&1 &
import re
import sys
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

# One stamp for the life of the server, so every file served in a run agrees
# about which copy of a module it means. A per-request stamp would make each
# import a different module and the page would load four copies of everything.
STAMP = str(int(time.time()))
SIBLING = re.compile(rb'(from\s+")(\./[\w.\-/]+\.js)(")')


class Fresh(SimpleHTTPRequestHandler):
    def send_head(self):
        if not self.path.split("?")[0].endswith(".js"):
            return super().send_head()
        where = self.translate_path(self.path)
        try:
            with open(where, "rb") as f:
                body = f.read()
        except OSError:
            return super().send_head()
        body = SIBLING.sub(rb'\1\2?at=' + STAMP.encode() + rb'\3', body)
        self.send_response(200)
        self.send_header("Content-Type", "text/javascript")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store, no-cache, must-revalidate")
        self.end_headers()
        import io

        return io.BytesIO(body)

    def end_headers(self):
        if not self.path.split("?")[0].endswith(".js"):
            self.send_header("Cache-Control", "no-store, no-cache, must-revalidate")
        super().end_headers()

    def log_message(self, *args):
        pass


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Fresh).serve_forever()
PY
echo $! > /tmp/errand-harness.pid
sleep 1
echo "open http://127.0.0.1:$PORT/ui/harness/window.html?at=$(date +%s)"
