Errand, version by version.

Written for somebody who has just installed one of these over the top of
the last, which is the only way there is: there is no updater, no remote to
update from, and nothing that signs a build. Each line is something a person
would notice, not something a commit did.

## 0.3.0

- Your agents can read your Mail and your Calendar, once you switch each one
  on under Settings. Nothing signs in to anything: these read the apps already
  on this Mac, so there is no account, no token and nothing that expires. Both
  are read-only, and each says what it lets an agent see before you turn it on.
- New agents start at "never ask", which is what a standing job needs: an
  errand nobody is sitting in front of cannot answer a permission card. Agents
  you already have keep the posture they had.
- A conversation says what day it was. Yesterday's briefing and this morning's
  no longer sit one under the other with nothing between them.
- The list of agents says when one has said something you have not read, how
  much, and how long ago.
- Clicking a notification opens the conversation it was about, rather than
  only bringing the window forward. One about a routine says it was a routine.
  The dock icon counts the agents stopped waiting on you.
- A notification is now held back only for the conversation you are actually
  reading, rather than whenever the window happens to be in front.
- Half a typed message stays with the conversation you were typing it into.
  It used to follow you to the next agent, where Enter would send it.
- Right-click the conversation picker to name a conversation or delete just
  that one. The three names Errand invents are "First", "New conversation" and
  "{name}, again", which after a week is a list that repeats one word.
- Fixed: asking what was unread took eight minutes and answered with the
  number it had stopped at rather than the number there was. It now looks only
  where Mail says there is something, stops out loud, and always says what it
  did not reach.
- A briefing that meets a busy provider no longer simply does not happen. A
  request refused because the server was overloaded, still loading a model or
  briefly unreachable is tried once more, waiting as long as the server asked
  for. A wrong key is not tried again, because it would be wrong again.
- Repeat has a Pause. Going away for a week used to mean pressing the only stop
  there was, which threw the schedule and what it says away together.
- Repeat says how it has actually been going: every run, what started it, and
  whether it worked. Three failed mornings used to leave a conversation looking
  merely quiet.
- Searching now says which line it found, and takes you to it and marks it,
  rather than opening whichever conversation that agent spoke in most recently.
- Cmd-F finds words in the conversation on screen, counts them, and steps
  through with Enter and Shift-Enter.
- Fixed: searching your mail never worked. It reported that you had no
  mailboxes.

## 0.2.0

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

## 0.1.0

- The first one anybody ran. No notes were written for it, and this line
  exists so that a version somebody is still running does not answer "what
  changed" with nothing at all.
