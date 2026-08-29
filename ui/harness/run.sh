#!/bin/sh
# Serve the window and its stand-in, so a browser can run the real page.
#
# A test that needs a browser is not a test that runs in CI, and that is a real
# cost. It buys the only thing that catches a window which draws half of itself:
# actually drawing it.
set -e
cd "$(dirname "$0")/../.."
PORT="${PORT:-8792}"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory . >/dev/null 2>&1 &
echo $! > /tmp/errand-harness.pid
sleep 1
echo "open http://127.0.0.1:$PORT/ui/harness/window.html"
