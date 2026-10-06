import { test } from "node:test";
import assert from "node:assert/strict";
import { wordsOf, stemOf, fitOf, usualWordsFor, leaningWordsFor, theBestFit } from "./fit.js";

test("a word is matched by its start, so tests and testing are test", () => {
  assert.equal(stemOf("testing"), "test");
  assert.equal(stemOf("windows"), "window");
  assert.deepEqual(wordsOf("Make a small Mac app"), ["small", "mac", "app"]);
});

test("a Mac app with a window fits a coder better than a calendar", () => {
  const asked = "Make a small Mac app with a window that converts between kilometres and miles, that I can start on this Mac.";
  const calendar = "Bell Ahead Calendar I watch your calendar and give you one clear sentence of warning before each thing starts";
  const coder = "Drill Coder Code Writes the code " + usualWordsFor("Code");
  const leaning = leaningWordsFor("Code");
  assert.ok(fitOf(asked, coder, leaning) > fitOf(asked, calendar), `${fitOf(asked, coder, leaning)} vs ${fitOf(asked, calendar)}`);
});

test("everyday Mac words only lean towards a coder, and alone mark nobody", () => {
  const coder = "Drill Coder Code Writes the code " + usualWordsFor("Code");
  const leaning = leaningWordsFor("Code");
  assert.equal(fitOf("My Mac is slow", coder, leaning), 0.5);
  assert.equal(fitOf("Check the Apple TV app news", coder, leaning), 0.5);
  assert.equal(theBestFit([{ key: "person:coder", fit: 0.5, team: 0 }]), null);
  // "Check" only leans towards a tester, so the inbox is the closer one.
  const tester = "Drill Tester QA Tests and breaks it " + usualWordsFor("QA");
  const inbox = "Inbox Watch Mail I check your mailboxes and tell you what came in";
  assert.ok(
    fitOf("Check my email for the invoice", inbox) > fitOf("Check my email for the invoice", tester, leaningWordsFor("QA")),
  );
  // A word of its own is still a whole word.
  assert.equal(fitOf("Fix the bug in the app", coder, leaning), 2.5);
});

test("nobody is the best fit when two teammates are equally close", () => {
  assert.equal(theBestFit([{ key: "person:a", fit: 1, team: 0 }, { key: "person:b", fit: 1, team: 0 }]), null);
  assert.equal(theBestFit([{ key: "team:t", fit: 2, team: 1 }, { key: "team:u", fit: 2, team: 1 }]), null);
});

test("one teammate ahead, or a team ahead of everybody, is the best fit", () => {
  assert.equal(theBestFit([{ key: "person:a", fit: 2, team: 0 }, { key: "person:b", fit: 1, team: 0 }]), "person:a");
  assert.equal(theBestFit([{ key: "team:t", fit: 4, team: 1 }, { key: "person:a", fit: 3, team: 0 }]), "team:t");
});

test("a teammate is marked before a team that only ties with it, and nothing for no words", () => {
  assert.equal(theBestFit([{ key: "person:a", fit: 2, team: 0 }, { key: "team:t", fit: 2, team: 1 }]), "person:a");
  assert.equal(theBestFit([{ key: "person:a", fit: 0, team: 0 }]), null);
  assert.equal(theBestFit([]), null);
});
