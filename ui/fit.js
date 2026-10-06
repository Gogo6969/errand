// Who fits a task best, by the words it shares with what each teammate says
// it does. Apart from the window so it can be tried on its own.

/** Words too common to say anything about who should do a task. */
export const TOO_COMMON = new Set(
  ("that this with from have what which will would should could about into them they their there then than when where your make made need needs does done some more most very just also only each every please want like " +
    "the and for you are but not all any can has its our out new get now one two use way who why how see let put say too was his her him may own team")
    .split(" "),
);

/** The usual words for a kind of role, so a role of two letters still matches. */
export const ROLE_WORDS = [
  [/\b(qa|test\w*|review\w*|check\w*|verif\w*)\b/i, "test tests testing verify review bug bugs break", "check checks"],
  // The last list is everyday words that only lean towards the role: half a
  // word each, never enough alone. Counted whole, "app" and "Mac" made the
  // coder the best fit for any chore on a Mac.
  [/\b(code\w*|coder|dev|developer|engineer\w*|build\w*|program\w*|app)\b/i, "code build program script tool function implement python rust javascript swift fix bug", "app apps mac macos window windows desktop ios"],
  [/\b(writ\w*|copy\w*|editor|docs|text|content)\b/i, "write writing readme docs documentation text explain article"],
  [/\b(research\w*|analys\w*|scout|news|market\w*|finance)\b/i, "research find sources search news compare analyse report"],
  [/\b(design\w*|visual\w*|pixel)\b/i, "design layout visual image icon style colour"],
];

export function wordsOf(text) {
  return (String(text || "").toLowerCase().match(/[a-z0-9]+/g) || []).filter(
    (w) => w.length >= 3 && !TOO_COMMON.has(w),
  );
}

/** A word without its ending, so test, tests, tester and testing are one. */
export function stemOf(w) {
  for (const end of ["ing", "ers", "er", "ed", "es", "s"]) {
    if (w.length > end.length + 2 && w.endsWith(end)) {
      w = w.slice(0, -end.length);
      break;
    }
  }
  return w.length > 3 && w.endsWith("e") ? w.slice(0, -1) : w;
}

/**
 * How many words a task shares with a profile, by their starts: one for each
 * of its own words, half for each everyday word its role only leans towards.
 */
export function fitOf(text, profile, leaning = "") {
  const theirs = new Set(wordsOf(profile).map(stemOf));
  const half = new Set(wordsOf(leaning).map(stemOf).filter((w) => !theirs.has(w)));
  const asked = new Set(wordsOf(text).map(stemOf));
  let fit = 0;
  for (const w of asked) fit += theirs.has(w) ? 1 : half.has(w) ? 0.5 : 0;
  return fit;
}

/** The usual words for a role, as one line, matched as a whole word. */
export function usualWordsFor(title) {
  return ROLE_WORDS.filter(([asks]) => asks.test(title || ""))
    .map(([, words]) => words)
    .join(" ");
}

/** The everyday words a role only leans towards, as one line. */
export function leaningWordsFor(title) {
  return ROLE_WORDS.filter(([asks, , leaning]) => leaning && asks.test(title || ""))
    .map(([, , leaning]) => leaning)
    .join(" ");
}

/**
 * The one to mark as the best fit, from candidates closest first: a teammate
 * before a team that only ties with it, and nobody when several teammates,
 * or several teams, are equally close. A tie on one shared word picked a
 * calendar teammate for a Mac app because it was first in the list.
 */
export function theBestFit(ranked) {
  const top = ranked[0]?.fit || 0;
  // Everyday words alone are no reason to mark anybody.
  if (top < 1) return null;
  const tied = ranked.filter((x) => x.fit === top);
  const people = tied.filter((x) => !x.team);
  if (people.length === 1) return people[0].key;
  if (people.length === 0 && tied.length === 1) return tied[0].key;
  return null;
}
