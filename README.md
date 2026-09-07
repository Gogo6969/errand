<div align="center">

# Errand

**Agents you come back to, rather than chats you start again.**

*Say what you want done. It tries, tells you what it did, and you correct it.
When it finally gets it right, that is the version you set to run every morning.*

[![License: MIT](https://img.shields.io/badge/License-MIT-00A97F.svg)](LICENSE)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-1E3A8A.svg)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-stable-CE422B.svg)](https://www.rust-lang.org/)
[![macOS](https://img.shields.io/badge/macOS-12%2B-F59E0B.svg)](#running-it)

</div>

> [!NOTE]
> **Version 0.2.0, signed but not yet notarised.** Built by hand on one Mac. There
> is no updater and nowhere to update from, so every copy arrives by hand over the
> top of the last one. The app says what changed in it for exactly that reason.

## What it is

An agent is somebody you come back to. It keeps what it learned, what it is
allowed to do and what it can reach, so starting again tomorrow is not starting
again from nothing.

Two engines answer, as peers: **Claude Code**, driven as a subprocess, and any
**local model** that speaks the OpenAI or Anthropic protocol, found on this Mac
or across the network. What you say to one means the same to the other, which is
the whole reason there is one implementation of asking, allowing and delegating
rather than one per engine.

An agent can hand part of a job to another, and several can work one problem in
a room. Pick "New room…" in the conversation picker and tick who is in it.
Everything you say there goes to each member in turn, never at once, so the
second hears what the first said; each answer is written into the room under
the name of the agent that gave it, and a line starting with @Name goes to that
one alone. Who is in it is decided when the room is made: no member can add
one, and there is no way to add one afterwards yet. The room takes one thing
round at a time; something said while a round is still going is kept in the
box and refused until it ends. The room is its own record: who said what, in
order. Each member takes part through a conversation of its own, "In the room:
<name>", under its own agent, which is where a question it stops to ask lands
and where you answer it.

## The four ways an errand starts

- **You type it.** You can talk to it while it works, and change your mind halfway.
- **The clock comes round.** A conversation with a schedule, so yesterday's
  briefing sits directly above today's. A run that arrives late says the time it
  was due, not the time it is now.
- **The world changes.** A folder gets a file, a page changes its mind. It says
  how often it will look and what that comes to before you agree to it.
- **A goal is not finished yet.** Something to get to rather than something to
  do: it keeps going, says where it has got to, and stops itself if it is going
  round in circles.

## Closing the window and quitting

Closing the window does not stop anything. Errand keeps running in the Dock,
and routines, watches, goals and started commands carry on; click the Dock icon
to get the window back. Quitting stops them all: Cmd-Q asks first when a routine
is due in the next fifteen minutes, Quit from the Dock menu does not. A Mac that
is asleep, has its lid down, or is off runs nothing; a routine whose time passed
runs the next time Errand is running and, when it is more than ten minutes
late, says so. Logging out quits Errand. With "Open Errand when I log in" on,
under Settings, a restart brings Errand back, and its routines and watches with
it; a goal that was part way through and a command that was running are not
picked up, and have to be started again.

An errand that finishes while you are reading something else says so with a
notification. If macOS has notifications switched off for Errand, Check this
setup says so and opens the right pane of System Settings for you, and the
first errand to finish unwatched says so in its conversation, once.

## Teaching it a task

Every errand is already written down step by step. A skill is one of those
errands kept by name. Say "save what we just did as a skill called tidy
downloads" and the agent keeps what you asked and the steps it took, from the
last turn that took any. Say "run the skill tidy downloads" and it does the
task again in a conversation of its own, "Skill: tidy downloads", handed the
original request and those steps as a plan: it follows them where things are
the same, changes them where they are not, and says what it did differently.
Nothing is replayed blind. Every step of the run goes through the same tools
and the same permission cards as any other errand, so a skill can do nothing
the agent could not do by hand. Saving under a name already taken replaces
that skill, and "what skills are there" lists them. A skill is one agent's,
like its notes: another agent has other folders and other tools.

## What it will and will not do without asking

It asks before anything that changes something, and your answer can become a
rule. Saying *always* to `top -l 1 -n 15 -o mem` allows `top`, not that exact
line, and the button says how wide that is **before** you press it. A command
that does more than one thing is never widened.

Every rule is listed where you can take it back, in words rather than in syntax.
Beside them are the rules Errand does **not** grant and cannot revoke: the ones
Claude Code allows out of its own settings files, shown with the file each lives
in. There is a third kind that is not a list at all, and it is said plainly:
some commands the engine judges harmless it runs without asking either list.

An agent set never to ask is not an agent with nothing between it and your
machine. It is walled into its own folder instead, because asking and a wall are
the two mechanisms there are, and turning one off is when the other has to be on.

What an agent says it did is checked where that is cheap. An answer that says
it wrote, saved, created or exported a file in its own folder, or in a folder
allowed to it, is looked up on the disk as it is written down; if the file is
not there, or was last changed before the errand began, a line of Errand's own
says so underneath, with the path and the times. The answer is left as it is,
nothing outside those folders is looked at, and a path inside a command the
agent ran is not a claim.

## From a terminal

```sh
Errand ask "Day Check" "what is the date?"     # prints the answer, exits 0
Errand ask --json "Day Check" "what day is it?" # JSON, or exit 3 if it is not
Errand ask --who                                # who you can hand work to
```

What it is doing goes to stderr as it happens, including the answer as it is
written, so the answer on stdout is still just the answer and arrives once.

## Running it

macOS 12 or later. [Claude Code](https://claude.com/claude-code) for the Claude
engine; nothing at all for a local model beyond something serving one.

```sh
cargo tauri build          # the .app and a .dmg in target/release/bundle
```

The build is signed with a Developer ID and is not yet notarised, so a copy from
someone else's Mac needs a right-click and Open the first time.

## Testing it

```sh
cargo test --workspace     # 296 tests
node --test ui/*.test.mjs  # the pure-JS parts
./ui/harness/run.sh        # then open the URL it prints: 234 checks in a real browser
```

The window harness runs the real `index.html`, the real `app.js` and the real
stylesheet against a stand-in for the app. It is not a nicety: it is the only
thing that catches a window which draws half of itself, because that failure
lives in the ordering of awaits and not in any function's output.

## Licence

MIT. See [LICENSE](LICENSE).
