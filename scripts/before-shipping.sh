#!/bin/sh
# Five errands, run against the installed app, before anything is pushed.
#
# This exists because the tests were green and the app did not work. 374 Rust
# tests and 363 window checks all pass while somebody trying to do a real thing
# gets stuck every time -- because none of them run an errand. A unit test knows
# a function returns what it should. It does not know that Claude Code is signed
# out, that a connector times out on a real mailbox, that a routine never fires,
# or that the answer arrives as a wall of nothing.
#
# So: five things somebody would actually ask for, end to end, against
# /Applications/Errand.app with its real store. Every one has to pass. If one
# does not, the build does not go out, and the failure is printed as what
# happened rather than as a number.
#
#     ./scripts/before-shipping.sh
#
# It makes its own agent and cleans up after itself.
set -u

APP="/Applications/Errand.app/Contents/MacOS/errand-app"
STORE="$HOME/Library/Application Support/Errand/errand.db"
WHO="Shipping Check"
PASSED=0
FAILED=0

say() { printf '\n\033[1m%s\033[0m\n' "$1"; }
won() { PASSED=$((PASSED + 1)); printf '  \033[32mworked\033[0m  %s\n' "$1"; }
lost() { FAILED=$((FAILED + 1)); printf '  \033[31mFAILED\033[0m  %s\n' "$1"; }

if [ ! -x "$APP" ]; then
  echo "There is no Errand in /Applications. Build and install it first."
  exit 2
fi
if ! pgrep -qf "Errand.app/Contents/MacOS/errand-app"; then
  echo "Errand is not running. Open it first: this talks to it."
  exit 2
fi

# ------------------------------------------------------------- set up --
# Its own agent, made the way the app makes one, so a run leaves nothing of
# somebody's behind and starts from the same place every time. Set to get on
# with things without asking, because there is nobody at a keyboard here to
# press a button -- the permission card itself is covered by the window
# harness, which can press one.
AGENT="shipping-check-agent"
# A conversation's id is also the engine's session id, and the engine requires
# that to be a uuid. Inventing a readable id here made the routine fail with
# "not a UUID" every time, which read as a broken clock for an afternoon.
FIRST=$(uuidgen | tr 'A-Z' 'a-z')
ROUTINE=$(uuidgen | tr 'A-Z' 'a-z')
HOME_DIR="$HOME/Library/Application Support/Errand/threads/$AGENT"
NOW=$(python3 -c 'import time;print(int(time.time()*1000))')
mkdir -p "$HOME_DIR"
# Everything a previous run may have left, including a run that was stopped
# part way through. Clearing only the agent left its conversation behind, and
# the next run then failed at the very first step with nothing to test.
sqlite3 "$STORE" "
  DELETE FROM runs WHERE conversation IN (SELECT id FROM conversations WHERE agent='$AGENT');
  DELETE FROM lines WHERE conversation IN (SELECT id FROM conversations WHERE agent='$AGENT');
  DELETE FROM conversations WHERE agent='$AGENT';
  DELETE FROM agents WHERE id='$AGENT';" || {
  echo "could not clear what an earlier run left behind"
  exit 2
}
sqlite3 "$STORE" "
  INSERT INTO agents (id, name, cwd, opened, started_at, spoke_at, engine, asks)
  VALUES ('$AGENT', '$WHO', '$HOME_DIR', 0, $NOW, $NOW, 'claude', 'auto');
  INSERT INTO conversations (id, agent, name, opened, started_at, spoke_at)
  VALUES ('$FIRST', '$AGENT', 'First', 0, $NOW, $NOW);" || {
  echo "could not make the agent to test with"
  exit 2
}

# ---------------------------------------------------------------- one --
# A turn that completes at all. Everything else is built on this, and it is
# what fails when the engine is signed out -- which looked like a working app
# until somebody typed a paragraph into it.
say "1. It answers a plain question"
ONE=$("$APP" ask "$WHO" "What is 17 times 23? Reply with just the number." 2>/dev/null | tail -1)
case "$ONE" in
  *391*) won "said 391" ;;
  *) lost "expected 391, got: ${ONE:-nothing at all}" ;;
esac

# ---------------------------------------------------------------- two --
# A tool, its permission, and its output reaching the person readably. The
# output half is the one that has broken: an outcome cut to 79 characters is
# an app that knows what happened and will not say.
say "2. It runs a command and shows what came back"
TWO=$("$APP" ask "$WHO" "Run the shell command: echo shipping-check-ok. Then reply with exactly what it printed." 2>/dev/null | tail -1)
case "$TWO" in
  *shipping-check-ok*) won "the command ran and its output came back" ;;
  *) lost "expected shipping-check-ok, got: ${TWO:-nothing at all}" ;;
esac

# -------------------------------------------------------------- three --
# Writing and reading a file in the agent's own folder: the wall, and the two
# tools an errand uses most.
say "3. It writes a file and reads it back"
THREE=$("$APP" ask "$WHO" "Write a file called shipping.txt containing the word marmalade, then read it back and reply with just what it contains." 2>/dev/null | tail -1)
case "$THREE" in
  *marmalade*) won "the file was written and read back" ;;
  *) lost "expected marmalade, got: ${THREE:-nothing at all}" ;;
esac

# --------------------------------------------------------------- four --
# A connector. Reaching outside the app at all, and answering honestly about
# what it could not reach rather than hanging or lying.
say "4. It reads something outside the app"
FOUR=$("$APP" ask "$WHO" "Use your unread_mail tool once with at_most 1, and reply in one line with exactly what it returned." 2>/dev/null | tail -1)
case "$FOUR" in
  *nread*|*unread*|*Mail*) won "the connector answered" ;;
  *) lost "the connector said nothing usable: ${FOUR:-nothing at all}" ;;
esac

# --------------------------------------------------------------- five --
# The thing this app is for: a job that runs on its own and is written down.
# Tested through the clock rather than by calling the function, because the
# clock is the part that has failed.
say "5. A routine fires on its own and is recorded"
if [ -z "$AGENT" ]; then
  lost "no agent to hang a routine on"
else
  NOW=$(python3 -c 'import time;print(int(time.time()*1000))')
  DUE=$(date -v+1M +%H:%M)
  # Never opened, which is what a routine nobody has spoken to looks like. The
  # opened flag is what makes the app resume a session rather than start one,
  # and setting it on a conversation with no session behind it fails every run.
  sqlite3 "$STORE" "
    INSERT INTO conversations (id, agent, name, opened, started_at, spoke_at, runs_at, runs_what)
    VALUES ('$ROUTINE', '$AGENT', 'Shipping check', 0, $NOW, $NOW, 'daily $DUE',
            'Reply with just the word: fired');" 2>/dev/null
  printf '  waiting for the clock (due %s, checked every 30s)\n' "$DUE"
  UNTIL=$(( $(date +%s) + 240 ))
  GOT=""
  while [ "$(date +%s)" -lt "$UNTIL" ]; do
    GOT=$(sqlite3 "$STORE" "SELECT text FROM lines WHERE conversation='$ROUTINE' AND kind='said' LIMIT 1;" 2>/dev/null)
    [ -n "$GOT" ] && break
    sleep 10
  done
  RAN=$(sqlite3 "$STORE" "SELECT count(*) FROM runs WHERE conversation='$ROUTINE';" 2>/dev/null)
  # A turn that ends badly writes down why. Reading it back here is the
  # difference between "the clock is broken" and the actual reason, which
  # twice now has been something else entirely.
  ENDED=$(sqlite3 "$STORE" "SELECT substr(text,1,200) FROM lines WHERE conversation='$ROUTINE' AND kind='ended' LIMIT 1;" 2>/dev/null)
  case "$GOT" in
    *fired*) won "it fired on its own and answered" ;;
    "") if [ -n "$ENDED" ]; then
          lost "it fired but the turn ended: $ENDED"
        else
          lost "the clock never fired it within four minutes"
        fi ;;
    *) lost "it fired but said: $GOT" ;;
  esac
  case "$RAN" in
    0|"") lost "the run was not written down, so nothing can say whether it works" ;;
    *) won "the run is in its history ($RAN recorded)" ;;
  esac
  sqlite3 "$STORE" "DELETE FROM runs WHERE conversation='$ROUTINE';
                    DELETE FROM lines WHERE conversation='$ROUTINE';
                    DELETE FROM conversations WHERE id='$ROUTINE';" 2>/dev/null
fi

# ------------------------------------------------------------- tidy up --
sqlite3 "$STORE" "DELETE FROM agents WHERE id='$AGENT';" 2>/dev/null
rm -rf "$HOME_DIR" 2>/dev/null

say "$PASSED worked, $FAILED failed"
if [ "$FAILED" -gt 0 ]; then
  echo "Do not push. Fix these first."
  exit 1
fi
echo "All five worked. This build is worth pushing."
