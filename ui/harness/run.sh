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
# With something different on the end every time.
#
# Everything the harness loads is already cache-busted from inside, but the page
# doing the busting is itself a file a browser will happily keep. It kept one:
# for three runs the harness reported green while testing markup from two edits
# earlier, which is precisely the failure it exists to prevent, arriving through
# the HTTP cache instead of through a copied file.
echo "open http://127.0.0.1:$PORT/ui/harness/window.html?at=$(date +%s)"
