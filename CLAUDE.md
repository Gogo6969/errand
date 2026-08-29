# Working on Errand

## Test what you deliver. Every single time.

**Nothing is delivered until it has been run.** Not read, not reasoned about,
not measured in a stylesheet: run, and looked at.

This rule exists because it was broken repeatedly and the same way. Three
changes in a row were reported as verified on the strength of reading the code
and measuring the CSS, and the third one left both pickers in the window empty
on startup. The person using it found it in ten seconds. It was reported to them
as "verified".

The reason reading is not enough is worth stating plainly, because it will feel
like enough again next time: **the bugs that survive review are not inside a
function, they are between them.** Every one of these was invisible in a diff
and obvious in a window:

- `show()` awaited `open_thread` before drawing the header. Harmless for months,
  because that call only read a row. The moment it grew to start an engine and
  bind a socket, the window sat blank for seconds saying "Nothing open".
- A wrapping flex row does not shrink its items to avoid breaking. Every
  give-way rule written to keep the header on one line was dead code.
- `main` was a grid of three rows with seven children, so opening a panel took
  the row meant for the conversation.
- A second `max-width` further down the file quietly beat the one that was meant
  to constrain the picker.

None of those is a mistake in the thing that was changed. All of them are a
mistake about what else was true.

### What counts as having tested it

| Kind of change | What has to be run |
|---|---|
| Anything in `ui/` | `ui/harness/run.sh`, then load the harness page and read the report. Every check green. |
| Anything in `core/` or `app/` | `cargo test --workspace`, and the ignored tests for whatever was touched |
| Anything a person will look at | A screenshot of the real window, taken and looked at |
| Anything that spawns a process | Run it end to end. Started, not just compiled. |

And before saying it works: **build it, install it, restart the app, and look at
it.** A stale bundle has cost an hour here more than once, and `cargo build`
succeeding says nothing about `/Applications/Errand.app`.

### The window harness

`ui/harness/` runs the real `index.html`, the real `app.js` and the real
stylesheet against a stand-in for `window.__TAURI__`. It is not a nicety. It is
the only thing that catches a window which draws half of itself, because that
failure lives in the ordering of awaits and not in any function's output.

    ./ui/harness/run.sh
    # then open the URL it prints, and read the report at the bottom

`?slow` makes opening a conversation take as long as it really does when an
engine has to start behind it. Use it. That is the switch that reproduced the
empty-pickers bug in one load.

When something is found wrong in the window, **add a check for it** before
fixing it. The list in `checks.js` should be a list of things that were once
actually broken.

### Screenshots

Screen capture works. Use `request_access` for the Errand app and take a real
screenshot. Do not claim the screen cannot be seen: that was said here, it was
wrong, and it was used to justify shipping something unlooked at.

## House style

- Comments explain **why**, not what. The reason a thing is the way it is,
  and the failure it prevents. A comment restating the code is worse than none.
- No em dashes.
- Test names are full sentences describing the behaviour, not labels:
  `a_routine_that_is_still_running_is_not_started_again`, never `test_routine`.
- Always, before saying anything is done:

      cargo fmt --all
      cargo clippy --all-targets -- -D warnings
      cargo test --workspace

## Things that are true about this codebase

- **Migrations are history.** Nothing already in `CHANGES` in `core/src/store.rs`
  may be edited, ever. A store that has applied change 1 will never apply it
  again, so an "improvement" to it changes only what a fresh install gets, and
  the two silently diverge. This was learnt the ordinary way.
- **`tauri::async_runtime::spawn`, never `tokio::spawn`,** from anywhere the
  Tauri event loop reaches. `tokio::spawn` outside a runtime does not start the
  task, and nothing says so: the clock task was simply never created and no
  routine ever ran.
- **`core/src/local/{talk,stream,find}.rs` are KinAI's**, kept close to their
  originals so re-syncing stays a `cp`. Put new work beside them, not in them.
- **The window is never told which engine answered.** That is the point of the
  seven events in `core/src/engine.rs`, and it stops being true the moment
  anything in `ui/` asks.
- **`umask` is process-wide.** Setting it around a `bind` corrupted the
  permissions of files other threads were creating at the same time.
