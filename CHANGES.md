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
- Your agents can read a web page in your own Chrome, once you switch that on
  too. Pages that need JavaScript, or that only make sense while you are signed
  in, now read as you see them rather than coming back as a consent screen. It
  opens a tab of its own behind the one you are on, reads it and closes it: it
  never clicks, types or fills anything in, and never touches a tab you already
  had open. It never asks for a file either, but a page it opens is a page, and
  a page can start a download the same as it would if you opened it yourself.
  Chrome needs "Allow JavaScript from Apple Events" switched on, under View,
  then Developer, in Chrome's own menu bar; it says so if it is off. Public
  addresses only: not localhost, not your router, not a .local name. And
  because the request goes out from your browser signed in as you, an agent
  that wants to read an address you did not give it yourself asks first,
  whatever posture it is on.
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
- A long command that overruns no longer throws away everything it did. A
  build, an install or a download that passes two minutes keeps running, and
  the agent is handed a way to watch it and stop it instead of being told to
  start over.
- What a long command is printing is now on screen in What Is Running, for the
  person who can decide to stop it. Only the agent could see it before.
- An agent can answer a running command that is waiting for a y/N. Nothing
  could type at one, which made the commonest prompt there is a dead end.
- Agents on a local model can now search inside files, find files by name, and
  change one piece of a file instead of rewriting the whole thing. They had
  none of these and had to shell out to grep, which stops to ask.
- Errand asks each model how much it can actually hold, instead of assuming
  32k for everything. A 200k model was losing its history four times sooner
  than it needed to, and an 8k model was refusing every request. The picker
  now says what each one holds.
- And it asks again as it goes, so a server you restart with a bigger window
  is noticed the same day rather than never. It asks behind whatever you are
  doing, never in front of it, and corrects every agent on that model at once
  rather than only the line in the picker.
- A write the wall stopped says it was the wall, instead of "operation not
  permitted", which sends people to change permissions on a folder that was
  never the problem.
- Opening a second copy of Errand says so rather than quietly sharing one set
  of conversations and running your routines twice.
- Pictures you send are shown in the thread, and stay there. They used to reach
  the agent and be thrown away, so the line said "(with a picture)" and you
  could never see the one you sent. Click one to see it larger.
- Handing over now works when there is no page to open. "Go into System
  Settings and switch this on" is the commonest thing to be asked, and it was
  being refused outright because it had no web address in it.
- A handover you answer late still counts. Granting an app Full Disk Access
  means quitting that app, so the one permission you are most likely to be
  asked for was the one that killed the agent waiting for your answer. The
  card keeps its buttons and picks the errand up where it left off.
- A picture you paste or drop is shown in the box, rather than named. A pasted
  screenshot is called "image.png" by the system, so two of them looked
  identical and there was no way to notice the wrong one until it had been sent.
- When something is stopping errands from working, Errand says so above the box
  before you type, and says what to do about it. A login that has expired now
  reads as a login that has expired, not as "401 OAuth access token has been
  revoked" under a message you had already spent ten minutes writing.
- Fixed: an agent on a model on this machine forgot everything the moment
  Errand was closed. Reopening one of its conversations handed it nothing,
  while the window went on showing the whole thread -- so a follow-up the next
  morning was answered by an agent that had never read what it followed up on,
  with nothing saying so. Agents on Claude were never affected.
- Allowing something for good is now the button that leads, once you have
  allowed the same thing before. It was the plain one beside the accented one
  people keep pressing, so you could say yes to curl four times and never find
  the way to stop being asked. The card also says how many times you have
  already allowed it.
- You can write a rule before being asked, under Allowed. Every rule used to
  cost an interruption to create.
- Pictures an agent makes or fetches are shown in its answer, instead of a path
  you cannot click. Agents had started apologising for this in prose: "both are
  downloaded locally if the images don't render for you".
- A file an answer points at can be clicked to show it in Finder. It is
  revealed rather than opened, so a path in an answer cannot run anything.
- When an agent needs a macOS permission it now opens that Settings pane for
  you, rather than describing where to find it. Automation is four levels down
  a screen most people have never opened.
- A task interrupted by Errand closing says so when you come back, and offers
  to run it again. It used to leave a question with no answer and nothing
  saying why, which looks exactly like an app still thinking about it.
- Fixed: the question before deleting an agent could be pushed off the bottom
  of the screen by its own length, so the one line you had to read was the one
  you could not.
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
