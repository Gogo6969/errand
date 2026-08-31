Errand, version by version.

Written for somebody who has just installed one of these over the top of
the last, which is the only way there is: there is no updater, no remote to
update from, and nothing that signs a build. Each line is something a person
would notice, not something a commit did.

## 0.1.0

- Answers arrive as they are written, a word at a time, rather than after
  several seconds of nothing followed by a wall of text.
- You can talk to it with your hands somewhere else. The call button beside
  the microphone sends when you stop talking, reads the answer out, and
  listens again. Escape ends it.
- Repeat and Watch offer the errands you already asked in that conversation,
  so a routine is the thing that actually worked rather than a retyped
  approximation of it.
- Try it now, in both, so you can see what a routine does before a morning
  goes past. It changes nothing about when it next runs.
- Errand can open when you log in, under Settings, so standing jobs survive a
  restart. A Mac that is asleep or off is still asleep or off.
- What it may do without asking now includes the rules Claude Code allows on
  its own, with the file each one lives in. Errand cannot take those back, and
  says so.
- A new copy of Errand says what it is, in eight lines, once.
- From a terminal: the answer streams on stderr as it is written, `--json` and
  `--shape` ask for a reply a script can read, and prose where an object was
  asked for exits 3 rather than being piped on.
- Fixed: the first thing said to a new agent failed with a database error, and
  a routine, watch, goal, name or model chosen for one was accepted and
  quietly kept by nobody.
