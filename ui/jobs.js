/**
 * What the overview decides about tasks, apart from how it draws them.
 *
 * A teammate is a job that goes on; a task is one piece of work given to it,
 * which is one of its conversations. What a task is doing, whether a finished
 * one is still in its teammate's task menu, and the order tasks are shown in
 * are here rather than in app.js so they can be tested without a window. Each
 * is a rule somebody will notice the moment it is wrong.
 */

/**
 * The groups a job can be in by what it is doing, in the order they are looked
 * at. The same words as the chip on a task and the row in the list down the
 * side: each place used to have words of its own for the same state, and
 * "Working now", "Running" and "Working" were one thing said three ways.
 */
export const STATES = [
  ["waiting", "Needs you"],
  ["working", "Running now"],
  ["stopped", "Stopped, needs a look"],
  ["scheduled", "Next up"],
  // Its errand is done and nothing of it is running, waiting or due: answered,
  // by its own account. Finished is the person's word, and is its own group:
  // answered is what the agent says, finished what they do.
  ["idle", "Answered, not marked finished"],
  ["paused", "Paused"],
  ["finished", "Finished"],
];

/** What "Show" can be set to, and what each keeps. */
export const SHOWING = [
  ["all", "All tasks"],
  ["waiting", "Needs you"],
  ["working", "Running now"],
  ["repeating", "Next up"],
  ["idle", "Answered"],
  ["finished", "Finished"],
  ["stopped", "Stopped"],
  ["paused", "Paused"],
];

/**
 * Whether a job is kept by what "Show" is set to.
 *
 * Repeating is any job with a routine or a watch, whatever it is doing this
 * minute: one that is working through its seven o'clock run is still a job
 * that repeats. Everything else is its state.
 */
export function shown(show, state, repeats) {
  if (show === "all") return true;
  if (show === "repeating") return repeats;
  return show === state;
}

/**
 * What a task is doing, from what the app says is running and what repeats.
 *
 * Finished first, because it is somebody's own word for it and outranks
 * anything the app can see. Then whatever needs them, then whatever is busy,
 * and only then what is merely set up to happen. A paused teammate pauses all
 * of its tasks.
 *
 * @param t the task, as the page holds it
 * @param a its teammate
 * @param running what the app says is running, every conversation's
 * @param standing every routine and watch
 * @returns {{state: string, waiting?: object, working?: object, stopped?: object, next?: object, watch?: object}}
 */
export function stateOf(t, a, running, standing) {
  const going = running.filter((w) => w.conversation === t.id);
  const theirs = standing.filter((s) => s.conversation === t.id);
  if (t.finished) return { state: "finished" };
  const waiting = going.find((w) => w.waiting);
  if (waiting) return { state: "waiting", waiting };
  if (going.length) return { state: "working", working: going[0] };
  if (a?.paused) return { state: "paused" };
  // A watch that gave up needs a look; a routine somebody switched off is
  // only paused, by their own hand, and Pause is what switched it off.
  const stopped = theirs.find((s) => s.stopped);
  if (stopped) return { state: "stopped", stopped };
  const live = theirs.filter((s) => !s.off);
  if (live.length) {
    const next = live.filter((s) => s.due).sort((x, y) => x.due - y.due)[0];
    return next ? { state: "scheduled", next } : { state: "scheduled", watch: live[0] };
  }
  if (theirs.length) return { state: "paused", off: theirs[0] };
  return { state: "idle" };
}

/**
 * A task's state as its chip says it: the kind it is drawn as, and the words.
 *
 * One place for the words, so the chip at the top of a task, the row in the
 * list down the side and the group in Now cannot drift apart again.
 *
 * @param is what stateOf said
 * @param t the task, for whether anything was said in it yet
 * @param when how a next run is said, short enough for a chip
 */
export function chipOf(is, t, when) {
  switch (is.state) {
    case "waiting":
      return { kind: "needs-you", says: "Needs you" };
    case "working":
      return { kind: "running", says: "Running now" };
    case "stopped":
      return { kind: "stopped", says: "Stopped" };
    case "scheduled":
      return { kind: "scheduled", says: is.next ? `Next ${when(is.next.due)}` : "Watching" };
    case "paused":
      return { kind: "paused", says: "Paused" };
    case "finished":
      return { kind: "finished", says: "Finished" };
    default:
      return { kind: "idle", says: t.said === false ? "New" : "Answered" };
  }
}

/** Where each state comes in a list of tasks: the order they are looked at. */
export function byState(list, stateFor) {
  const rank = new Map(STATES.map(([state], at) => [state, at]));
  return [...list].sort((x, y) => {
    const [sx, sy] = [stateFor(x), stateFor(y)];
    return (
      rank.get(sx.state) - rank.get(sy.state) ||
      // What is due soonest first, among what is due.
      (sx.next?.due ?? Infinity) - (sy.next?.due ?? Infinity) ||
      (y.spoke || 0) - (x.spoke || 0)
    );
  });
}

/**
 * Whether a task is in its teammate's task menu.
 *
 * A finished one stays, marked, for the days somebody chose, and then lives
 * only in the overview. A search finds it whatever its age, because "where
 * did that go" is exactly when somebody searches.
 */
export function stillInTheList(a, now, keptDays, searching = false) {
  if (searching || !a.finished) return true;
  return a.finished >= now - keptDays * 86_400_000;
}

/**
 * The order jobs are shown in: most important first when ordering by
 * priority, and within the same priority, or when ordering by recency, the
 * one spoken to most recently first.
 */
export function inOrder(list, byPriority) {
  return [...list].sort(
    (x, y) => (byPriority ? (x.priority || 2) - (y.priority || 2) : 0) || (y.spoke || 0) - (x.spoke || 0),
  );
}

/**
 * Tasks by teammate: each teammate's tasks together, teammates by name.
 *
 * @param list tasks, each carrying `who`, its teammate's name
 */
export function byTeammate(list) {
  const teams = new Map();
  for (const t of list) {
    const who = t.who || "Nobody";
    if (!teams.has(who)) teams.set(who, []);
    teams.get(who).push(t);
  }
  return [...teams.entries()].sort(([x], [y]) => x.localeCompare(y));
}

/**
 * The names the app gives a task before anybody has. They say nothing about
 * what it is for: "First" was every teammate's first task, and every task
 * another teammate started was "Asked by" whoever started it, so three of
 * them read the same down the side.
 */
const MADE_UP = [
  "First",
  "New task",
  "New conversation",
  "Asked by something outside",
  "Asked from the terminal",
];

/** Whether a task's name is one the app made up rather than a name. */
export function aMadeUpName(name) {
  return !name || MADE_UP.includes(name) || /^Asked by /.test(name) || /, again$/.test(name);
}

/**
 * Who asked for a task, when its name only says that: "Asked by Day Check"
 * came from Day Check. Nothing, for a task called anything else.
 */
export function askedBy(name) {
  if (name === "Asked from the terminal") return "the terminal";
  if (name === "Asked by something outside") return "something outside";
  const by = /^Asked by (.+)$/.exec(name || "");
  return by ? by[1] : null;
}

/**
 * A request, said as the name of a task: its first sentence, without the
 * words around it that only ask, short enough for the list down the side.
 *
 * "Please check the disk on the build server every day and tell me if it is low" is a
 * request; "Check the disk on the build server every day and tell me…" is what the task
 * is. Who asked comes off too: it is said on the task's card.
 *
 * @param {string} request the first thing asked in the task
 * @param {number} room how many characters a name may have
 */
export function headline(request, room = 52) {
  let said = String(request || "").trim().split("\n")[0].trim();
  said = said.replace(/^[^:]{1,40} asks: /, "");
  said = said.replace(
    /^(?:(?:hey|hi|hello|ok|okay|so)[,!.]?\s+)?(?:(?:please|can you|could you|would you|will you|i want you to|i'd like you to|i would like you to|i need you to)[,]?\s+)+/i,
    "",
  );
  const sentence = /^(.+?[.!?])(?:\s|$)/.exec(said);
  if (sentence) said = sentence[1];
  // A question keeps its mark; anything else ends where its words do, a dash
  // left hanging included.
  said = said.replace(/[.!,;:\s\-\u2013\u2014]+$/, "");
  if (!said) return "";
  // An address stays as it is written.
  if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(said)) said = said[0].toUpperCase() + said.slice(1);
  if (said.length <= room) return said;
  // Where its first part ends, when that part is a name on its own: "and tell
  // me if it is low" is how to report, not what the task is.
  const part = said
    .split(/,\s|;\s|\s[-\u2013\u2014]\s|\s(?:and|then|but)\s/i)[0]
    .replace(/[\s,;:\-\u2013\u2014]+$/, "");
  if (part.length >= room / 2 && part.length <= room) return part;
  const cut = said.slice(0, room);
  const space = cut.lastIndexOf(" ");
  const atAWord = space > room / 2 ? cut.slice(0, space) : cut;
  return `${atAWord.replace(/[\s,;:.]+$/, "")}…`;
}

/**
 * Whether what was first asked in a task was about setting a standing job up
 * rather than the job itself: "Set yourself a standing job: produce the Friday
 * report" names the setting up. A task like that is called by what its job
 * does each time, which is what it is.
 */
export function setsUpAJob(asked) {
  const first = String(asked || "").trim().split("\n")[0];
  // About the setting up, not about how often: "the weekly tally for Friday"
  // is a task, and "every morning, tell me what moved" says what it is too.
  return /\b(standing job|routine|on a schedule|schedule (it|this)|repeat (it|this))\b/i.test(first);
}

/**
 * What a task is called, when the app named it: what was asked in it, or for
 * a standing job set up in words about setting it up, what the job does.
 *
 * @param {string} first the first thing asked in it
 * @param {string} job what its routine or watch asks each time, if it has one
 */
export function aNameFrom(first, job) {
  return headline(job && setsUpAJob(first) ? job : first);
}
