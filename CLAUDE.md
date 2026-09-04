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

### Before any version goes out: the five errands

`./scripts/before-shipping.sh` runs five real errands against the installed
`/Applications/Errand.app` and its real store. **All five have to pass before a
build is pushed anywhere.** No exceptions, and passing unit tests is not a
substitute: for weeks every unit test was green while the app itself could not
finish a single errand, which is the only measure anybody outside this repo
cares about.

1. It answers a plain question.
2. It runs a command and shows what came back.
3. It writes a file and reads it back.
4. It reads something outside the app, through a connector.
5. A routine fires on its own, off the clock, and is written down.

Run it after installing the new bundle, not before:

```
cargo tauri build --bundles app
rm -rf /Applications/Errand.app && cp -R target/release/bundle/macos/Errand.app /Applications/
./scripts/before-shipping.sh
```

On any engine, not only Claude Code. An app that passes its own gate on one
engine has been tested on one, and a login that expires should not be able to
stop the gate running at all:

```
ENGINE=local MODEL='{"provider":"openai-compat","base_url":"https://api.deepseek.com/v1","model":"deepseek-v4-flash","wire":"openai"}' ./scripts/before-shipping.sh
```

Launch the app the way somebody actually launches it, from the Finder, and not
from a terminal. A terminal hands it the PATH from your shell profile and the
Finder hands it four system folders, which is the difference between finding
Claude Code and not:

```
env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin HOME="$HOME" USER="$USER" SHELL=/bin/zsh /usr/bin/open -a Errand
```

Two things the suite has already had to learn the hard way, both of which cost
an afternoon by looking like a dead clock:

- **A conversation's id is also the engine's session id, so it has to be a
  uuid.** A readable id like `shipping-routine` fails with "not a UUID", and
  fifteen characters of one panicked the socket name.
- **`opened` is what decides resume against start.** Setting it on a
  conversation that has no session behind it fails every single run.

When task 5 fails, read the `ended` line on the conversation before believing
the clock is broken. Three times now it has been something else.

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

**Run it in a window at least 700px wide.** A media query reads the viewport,
not any element, so a harness run in a narrow pane measures a header that is
wrapping exactly as it was told to and reports the app broken. The header check
now refuses to judge below that width rather than pass or fail on a test it did
not run. There is no way to widen the viewport from inside the page; widen the
actual window.

When something is found wrong in the window, **add a check for it** before
fixing it. The list in `checks.js` should be a list of things that were once
actually broken.

### Two things that only using it will find

Both of these passed every test and were wrong the moment they were on screen.

**macOS rewrites what somebody types.** Two hyphens become a dash, straight
quotes become curly ones. That is right for prose and wrong for every path and
every address, and a watch on a folder whose name contained `--` failed to find
a folder that was plainly there, with nothing on screen saying why. Anything
that reads a path or a URL from a text field puts that punctuation back first;
`watch::Watch::read` does.

**An agent's name is not a name.** It is whatever its first message was, cut
short, so it is usually a whole sentence. Dropping one into the middle of a
sentence produced "wakes Write a file called hello.txt containing when what is
there changes". `watch::reads_as_a_name` decides; anything that splices a name
into prose should ask it first.

### Where new work goes

There are four ways an errand can start, and each has a module holding the
judgement and an app-side part holding the plumbing:

- somebody types it
- the clock comes round (`routine`)
- something out in the world changes (`watch`)
- a goal is not finished yet (`goal`)

Each one says, in numbers and before anybody agrees to it, how often it can
possibly cost something. That is not decoration. It is the only reason any of
them is safe to leave switched on.

### Work that outlives the turn

`jobs` holds commands started rather than run. Two rules that took a test to
find: whatever waits on the child must not *own* it, or stopping a job finds
nothing there to stop; and a helper that builds a walled command sets the
working directory itself, or the wall is around somewhere the command is not
standing.

### Binding a socket needs a runtime

`tauri::async_runtime::spawn`, never `tokio::spawn`, is written down twice
already. This is the same rule from the other end: `UnixListener::bind`
registers with the reactor, so it too has to be called from inside a runtime,
and Tauri's `Ready` callback is not one.

It does not fail loudly. The socket file appears, because the system call that
creates it succeeds before the registration that does not, and what is left is
a door that is plainly there with nobody behind it. Two clues that it was this:
the file is mode 0755 rather than 0600, because the chmod after the bind never
ran, and `lsof -U` shows nothing holding it.

### Three providers, three shapes

There is no single OpenAI-compatible URL shape. Moonshot serves under `/v1`,
Z.ai under `/api/paas/v4`, DeepSeek off the bare root. A client that appends
`/v1` to all of them is wrong about two, and the failure is a 404 that reads
exactly like a bad key -- so somebody spends an evening on their key.

None of it can be settled from the documentation, because all three gateways
authenticate before they route: an unauthenticated probe answers the same 401
for a real path and for nonsense. So `find::settle` asks once, with the key, at
the moment somebody adds the backend, and the address that answered is what gets
stored. Three ambiguities become one fact.

Two more that will fail every request rather than some:

- **Send no `temperature` unless somebody chose one.** Kimi's models pin
  sampling and answer an error rather than clamping.
- **Hand back `reasoning_content` exactly as it arrived.** DeepSeek's reasoning
  models refuse a request with tools in it whose earlier assistant turns are
  missing it, and every turn here has tools in it: it would work once and fail
  on the second. Keep it apart from `content` -- joined on, the model's private
  working ends up in the answer, in its notes, and in anything it summarises.

Model ids are not to be compiled in. Two of the three the research turned up had
already been retired. Ask the endpoint.

### Before deciding the app is broken, check the screen is awake

A screenshot with no menu bar and no dock is a sleeping display or a lock
screen, not an app with no window. An hour went into "the window is gone",
including rebuilding the previous commit to bisect it, and the app had been
running correctly the whole time.

`osascript ... count of windows` is not the check either: it returned 0 for a
build that was known good.

### The harness will test yesterday's code if you let it

It reads the real files at run time because an embedded copy went stale within
the hour. The HTTP cache is that same failure in a different coat, and it
happened: three runs reported green while testing markup from two edits back,
including a form field that did not exist.

Everything it loads now carries a cache-busting query, `window.html` asks not to
be kept, and `run.sh` prints a URL with a timestamp on it. **Open the URL
`run.sh` prints.** A bare `location.reload()` does not re-fetch the modules.

When a check fails for a reason that makes no sense, check first that the page
is running the code you just wrote: `document.getElementById("the-new-thing")`
in the console settles it in one line.

### A replace that matches nothing says nothing

Editing these files with `str.replace` in a throwaway script is fine and fast,
and it fails silently: a pattern that does not match leaves the file alone and
the script reports success. That has now caused two bugs here, the second being
a URL built as `/v1/models/v1/models` because a signature change did not apply
while its call sites did. **Assert every pattern is present before replacing.**

### Asking and the wall are the same job done two ways

`wall` builds one sandbox profile for both engines. A local model always gets
it, because it has no asking of its own. Claude Code gets it exactly when the
asking is switched off, and never otherwise, because asking is the better
mechanism while it is on: it explains itself and a wall does not.

Every allowance in that profile is load-bearing. Without `~/.npm`, `npx` fails
with npm's own advice to `sudo chown` a directory that is fine, so a wall with
a gap in it does not merely block something, it sends somebody to break their
own machine. Anything added there needs a test naming what stopped working.

### A description of something live has to be live

A panel drawn once and never again goes on saying "it has not looked yet" while
the thing it describes is being woken behind it. That is not a stale view, it
is a false statement, and it is the same class of fault as a silent failure.
While a panel that describes background work is on screen, it re-reads. Redraw
the sentence always and the input boxes never, or a half-typed path gets taken
back while somebody is typing it.

Related: a watch that failed says so on the first failure, not after the fifth.

The harness can deliver an event (`tell` in `harness.js`). Before that its
`listen` returned a shrug, so everything the window only ever learns from an
event had no test at all, and a line pushed in with the wrong shape drew as
nothing live and drew fine after a reload -- invisible exactly when it mattered.

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
