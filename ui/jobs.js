/**
 * What the overview decides about jobs, apart from how it draws them.
 *
 * Here rather than in app.js so that it can be tested without a window: which
 * group a job is in, whether a finished one is still in the list down the
 * side, and the order jobs are shown in. Each is a rule somebody will notice
 * the moment it is wrong, and none of them needs a page to be checked.
 */

/** The groups a job can be in by what it is doing, in the order they are looked at. */
export const STATES = [
  ["waiting", "Waiting on you"],
  ["working", "Working now"],
  ["stopped", "Stopped, needs a look"],
  ["scheduled", "Runs on its own"],
  ["idle", "Idle"],
  ["paused", "Paused"],
  ["finished", "Finished"],
];

/**
 * What a job is doing, from what the app says is running and what repeats.
 *
 * Finished first, because it is somebody's own word for it and outranks
 * anything the app can see. Then whatever needs them, then whatever is busy,
 * and only then what is merely set up to happen.
 *
 * @param a the agent, as the page holds it
 * @param running what the app says is running, every conversation's
 * @param standing every routine and watch
 * @returns {{state: string, waiting?: object, working?: object, stopped?: object, next?: object, watch?: object}}
 */
export function stateOf(a, running, standing) {
  const going = running.filter((w) => w.agent === a.id);
  const theirs = standing.filter((s) => s.agent === a.id);
  if (a.finished) return { state: "finished" };
  const waiting = going.find((w) => w.waiting);
  if (waiting) return { state: "waiting", waiting };
  if (going.length) return { state: "working", working: going[0] };
  if (a.paused) return { state: "paused" };
  const stopped = theirs.find((s) => s.stopped || s.off);
  if (stopped) return { state: "stopped", stopped };
  if (theirs.length) {
    const next = theirs.filter((s) => s.due).sort((x, y) => x.due - y.due)[0];
    return next ? { state: "scheduled", next } : { state: "scheduled", watch: theirs[0] };
  }
  return { state: "idle" };
}

/**
 * Whether a job is in the list down the side.
 *
 * A finished one stays, at the bottom, for the days somebody chose, and then
 * lives only in the overview. A search finds it whatever its age, because
 * "where did that go" is exactly when somebody searches.
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
 * Jobs by what they are about: the role each settled on, with those that
 * have none last, as "Other".
 */
export function bySubject(list) {
  const subjects = new Map();
  for (const a of list) {
    const subject = a.title || "Other";
    if (!subjects.has(subject)) subjects.set(subject, []);
    subjects.get(subject).push(a);
  }
  return [...subjects.entries()].sort(
    ([x], [y]) => (x === "Other") - (y === "Other") || x.localeCompare(y),
  );
}
