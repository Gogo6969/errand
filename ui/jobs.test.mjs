// What the overview decides about jobs, checked without a window.
//
//     node --test jobs.test.mjs

import test from "node:test";
import assert from "node:assert/strict";
import { STATES, SHOWING, shown, stateOf, stillInTheList, inOrder, bySubject } from "./jobs.js";

const day = 86_400_000;
const job = (id, more = {}) => ({ id, name: id, title: "", priority: 2, spoke: 0, finished: null, paused: false, ...more });

test("a job somebody marked finished is finished, whatever else is going on", () => {
  const a = job("desk", { finished: 1000 });
  const running = [{ agent: "desk", waiting: true, what: "Waiting on you: sign in" }];
  assert.equal(stateOf(a, running, []).state, "finished");
});

test("waiting on somebody comes before working, and working before anything set up", () => {
  const a = job("desk");
  const standing = [{ agent: "desk", due: 5, what: "The briefing" }];
  assert.equal(stateOf(a, [{ agent: "desk", waiting: false, what: "Writing" }], standing).state, "working");
  const both = [
    { agent: "desk", waiting: false, what: "Writing" },
    { agent: "desk", waiting: true, what: "Waiting on you: yes or no" },
  ];
  const is = stateOf(a, both, standing);
  assert.equal(is.state, "waiting");
  assert.equal(is.waiting.what, "Waiting on you: yes or no");
});

test("a paused job is paused, and a stopped routine or watch needs a look", () => {
  assert.equal(stateOf(job("desk", { paused: true }), [], []).state, "paused");
  const off = [{ agent: "desk", off: true, at: "daily 07:00" }];
  assert.equal(stateOf(job("desk"), [], off).state, "stopped");
  const lost = [{ agent: "desk", stopped: "Stopped looking.", at: "https://example.com" }];
  assert.equal(stateOf(job("desk"), [], lost).stopped.stopped, "Stopped looking.");
});

test("a job that repeats says which run is next, and one that only watches says so", () => {
  const standing = [
    { agent: "desk", due: 300, what: "Later" },
    { agent: "desk", due: 100, what: "Sooner" },
  ];
  assert.equal(stateOf(job("desk"), [], standing).next.what, "Sooner");
  const watching = stateOf(job("desk"), [], [{ agent: "desk", due: null, at: "mail every 10m" }]);
  assert.equal(watching.state, "scheduled");
  assert.equal(watching.watch.at, "mail every 10m");
});

test("anything else is idle, and every state has a group to be drawn in", () => {
  assert.equal(stateOf(job("desk"), [], [{ agent: "other", due: 1 }]).state, "idle");
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

test("by subject jobs are grouped by the role each chose, with none last", () => {
  const groups = bySubject([
    job("a", { title: "Research" }),
    job("b"),
    job("c", { title: "Mail" }),
    job("d", { title: "Research" }),
  ]);
  assert.deepEqual(
    groups.map(([subject, list]) => [subject, list.map((a) => a.id)]),
    [["Mail", ["c"]], ["Research", ["a", "d"]], ["Other", ["b"]]],
  );
});

test("completed and finished are two groups: what the agent says, and what its person does", () => {
  const labels = Object.fromEntries(STATES);
  assert.match(labels.idle, /^Completed/);
  assert.equal(labels.finished, "Finished");
  assert.equal(labels.scheduled, "Repeating");
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
