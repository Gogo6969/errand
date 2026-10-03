// What the overview decides about jobs, checked without a window.
//
//     node --test jobs.test.mjs

import test from "node:test";
import assert from "node:assert/strict";
import { STATES, SHOWING, shown, stateOf, chipOf, byState, stillInTheList, inOrder, byTeammate } from "./jobs.js";

const day = 86_400_000;
const job = (id, more = {}) => ({ id, name: id, title: "", priority: 2, spoke: 0, finished: null, paused: false, ...more });

const teammate = (more = {}) => ({ id: "desk", name: "Bitcoin Desk", paused: false, ...more });

test("a task somebody marked finished is finished, whatever else is going on", () => {
  const t = job("talk-1", { finished: 1000 });
  const running = [{ conversation: "talk-1", agent: "desk", waiting: true, what: "Needs you: sign in" }];
  assert.equal(stateOf(t, teammate(), running, []).state, "finished");
});

test("waiting on somebody comes before working, and working before anything set up", () => {
  const t = job("talk-1");
  const standing = [{ conversation: "talk-1", agent: "desk", due: 5, what: "The briefing" }];
  assert.equal(stateOf(t, teammate(), [{ conversation: "talk-1", waiting: false, what: "Writing" }], standing).state, "working");
  const both = [
    { conversation: "talk-1", waiting: false, what: "Writing" },
    { conversation: "talk-1", waiting: true, what: "Needs you: yes or no" },
  ];
  const is = stateOf(t, teammate(), both, standing);
  assert.equal(is.state, "waiting");
  assert.equal(is.waiting.what, "Needs you: yes or no");
  // Another task of the same teammate being busy is not this one being busy.
  assert.equal(stateOf(job("talk-2"), teammate(), both, []).state, "idle");
});

test("a paused teammate or a routine switched off is paused, and a watch that gave up needs a look", () => {
  assert.equal(stateOf(job("talk-1"), teammate({ paused: true }), [], []).state, "paused");
  // Switched off by somebody, with Pause: paused, not something gone wrong.
  const off = [{ conversation: "talk-1", off: true, at: "daily 07:00" }];
  const paused = stateOf(job("talk-1"), teammate(), [], off);
  assert.equal(paused.state, "paused");
  assert.equal(paused.off.at, "daily 07:00");
  const lost = [{ conversation: "talk-1", stopped: "Stopped looking.", at: "https://example.com" }];
  assert.equal(stateOf(job("talk-1"), teammate(), [], lost).state, "stopped");
  assert.equal(stateOf(job("talk-1"), teammate(), [], lost).stopped.stopped, "Stopped looking.");
  // A routine switched off beside a watch still looking: it still runs.
  const both = [
    { conversation: "talk-1", off: true, at: "daily 07:00" },
    { conversation: "talk-1", due: null, at: "mail every 10m" },
  ];
  assert.equal(stateOf(job("talk-1"), teammate(), [], both).state, "scheduled");
});

test("a chip says a state in the same words wherever it is shown", () => {
  const when = () => "Fri 3 PM";
  const t = job("talk-1", { said: true });
  assert.deepEqual(chipOf({ state: "waiting" }, t, when), { kind: "needs-you", says: "Needs you" });
  assert.deepEqual(chipOf({ state: "working" }, t, when), { kind: "running", says: "Running now" });
  assert.deepEqual(chipOf({ state: "scheduled", next: { due: 1 } }, t, when), { kind: "scheduled", says: "Next Fri 3 PM" });
  assert.deepEqual(chipOf({ state: "scheduled", watch: {} }, t, when), { kind: "scheduled", says: "Watching" });
  assert.deepEqual(chipOf({ state: "paused" }, t, when), { kind: "paused", says: "Paused" });
  assert.deepEqual(chipOf({ state: "stopped" }, t, when), { kind: "stopped", says: "Stopped" });
  assert.deepEqual(chipOf({ state: "finished" }, t, when), { kind: "finished", says: "Finished" });
  assert.deepEqual(chipOf({ state: "idle" }, t, when), { kind: "idle", says: "Answered" });
  assert.deepEqual(chipOf({ state: "idle" }, job("talk-2", { said: false }), when), { kind: "idle", says: "New" });
  // And a group in Now says it the same way.
  const labels = Object.fromEntries(STATES);
  assert.equal(labels.waiting, "Needs you");
  assert.equal(labels.working, "Running now");
});

test("tasks are listed in the order their states are looked at, what is due soonest first", () => {
  const states = {
    a: { state: "idle" },
    b: { state: "scheduled", next: { due: 300 } },
    c: { state: "waiting" },
    d: { state: "scheduled", next: { due: 100 } },
    e: { state: "finished" },
    f: { state: "working" },
  };
  const list = Object.keys(states).map((id) => job(id));
  assert.deepEqual(byState(list, (t) => states[t.id]).map((t) => t.id), ["c", "f", "d", "b", "a", "e"]);
});

test("a task that repeats says which run is next, and one that only watches says so", () => {
  const standing = [
    { conversation: "talk-1", due: 300, what: "Later" },
    { conversation: "talk-1", due: 100, what: "Sooner" },
  ];
  assert.equal(stateOf(job("talk-1"), teammate(), [], standing).next.what, "Sooner");
  const watching = stateOf(job("talk-1"), teammate(), [], [{ conversation: "talk-1", due: null, at: "mail every 10m" }]);
  assert.equal(watching.state, "scheduled");
  assert.equal(watching.watch.at, "mail every 10m");
});

test("anything else is idle, and every state has a group to be drawn in", () => {
  assert.equal(stateOf(job("talk-1"), teammate(), [], [{ conversation: "other", due: 1 }]).state, "idle");
  const groups = STATES.map(([state]) => state);
  for (const state of ["waiting", "working", "stopped", "scheduled", "idle", "paused", "finished"]) {
    assert.ok(groups.includes(state), state);
  }
});

test("a finished job stays in the list for the days chosen, and a search finds it after", () => {
  const now = 100 * day;
  assert.equal(stillInTheList(job("new"), now, 7), true);
  assert.equal(stillInTheList(job("recent", { finished: now - 6 * day }), now, 7), true);
  assert.equal(stillInTheList(job("old", { finished: now - 8 * day }), now, 7), false);
  assert.equal(stillInTheList(job("old", { finished: now - 8 * day }), now, 7, true), true);
});

test("by priority the most important come first, and the most recent within one", () => {
  const list = [
    job("low", { priority: 3, spoke: 50 }),
    job("normal-old", { priority: 2, spoke: 10 }),
    job("high", { priority: 1, spoke: 1 }),
    job("normal-new", { priority: 2, spoke: 20 }),
  ];
  assert.deepEqual(inOrder(list, true).map((a) => a.id), ["high", "normal-new", "normal-old", "low"]);
  assert.deepEqual(inOrder(list, false).map((a) => a.id), ["low", "normal-new", "normal-old", "high"]);
});

test("by teammate, each teammate's tasks are together, teammates by name", () => {
  const groups = byTeammate([
    job("a", { who: "Trend Scout" }),
    job("b", { who: "Disk Watch" }),
    job("c", { who: "Trend Scout" }),
  ]);
  assert.deepEqual(
    groups.map(([who, list]) => [who, list.map((t) => t.id)]),
    [["Disk Watch", ["b"]], ["Trend Scout", ["a", "c"]]],
  );
});

test("answered and finished are two groups: what the agent says, and what its person does", () => {
  const labels = Object.fromEntries(STATES);
  assert.match(labels.idle, /^Answered/);
  assert.equal(labels.finished, "Finished");
  assert.equal(labels.scheduled, "Next up");
});

test("showing repeating keeps every job that repeats, whatever it is doing now", () => {
  assert.equal(shown("repeating", "working", true), true);
  assert.equal(shown("repeating", "idle", false), false);
  assert.equal(shown("finished", "finished", false), true);
  assert.equal(shown("finished", "idle", true), false);
  assert.equal(shown("all", "paused", false), true);
  // Every choice offered is one this can answer.
  for (const [show] of SHOWING) assert.equal(typeof shown(show, "idle", false), "boolean");
});
