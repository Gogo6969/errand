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
