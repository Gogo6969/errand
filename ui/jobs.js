/**
 * What the overview decides about tasks, apart from how it draws them.
 *
 * A teammate is a job that goes on; a task is one piece of work given to it,
 * which is one of its conversations. What a task is doing, whether a finished
 * one is still in its teammate's task menu, and the order tasks are shown in
 * are here rather than in app.js so they can be tested without a window. Each
 * is a rule somebody will notice the moment it is wrong.
 */

/** The groups a job can be in by what it is doing, in the order they are looked at. */
export const STATES = [
  ["waiting", "Waiting on you"],
  ["working", "Working now"],
  ["stopped", "Stopped, needs a look"],
  ["scheduled", "Repeating"],
  // Its errand is done and nothing of it is running, waiting or due: done for
  // now, by its own account. Finished is the person's word, and is its own
  // group: completed is what the agent says, finished what they do.
  ["idle", "Completed, not marked finished"],
  ["paused", "Paused"],
  ["finished", "Finished"],
];

/** What "Show" can be set to, and what each keeps. */
export const SHOWING = [
  ["all", "All tasks"],
  ["waiting", "Waiting on you"],
  ["working", "Working now"],
  ["repeating", "Repeating"],
  ["idle", "Completed"],
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
  const stopped = theirs.find((s) => s.stopped || s.off);
  if (stopped) return { state: "stopped", stopped };
  if (theirs.length) {
    const next = theirs.filter((s) => s.due).sort((x, y) => x.due - y.due)[0];
    return next ? { state: "scheduled", next } : { state: "scheduled", watch: theirs[0] };
  }
  return { state: "idle" };
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
