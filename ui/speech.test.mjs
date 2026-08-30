// Is what gets read aloud worth listening to?
//
//     node --test ui/speech.test.mjs
//
// Everything here was heard being read badly before it was written down. A
// voice saying "star star Price colon star star" is not a detail: it is the
// whole difference between a call and a party trick.

import { test } from "node:test";
import assert from "node:assert/strict";

import { worthSaying, enoughOfIt, toSay } from "./speech.js";

test("the marks that make text look right are not read out", () => {
  assert.equal(worthSaying("**Price:** it went up"), "Price: it went up");
  assert.equal(worthSaying("## What I found"), "What I found");
  assert.equal(worthSaying("- one\n- two"), "one two");
  assert.equal(worthSaying("1. first\n2. second"), "first second");
  assert.equal(worthSaying("> it said so"), "it said so");
  assert.equal(worthSaying("it is *really* gone"), "it is really gone");
});

test("a code block is named rather than read out one bracket at a time", () => {
  const said = worthSaying("Run this:\n```sh\nfor f in *; do rm -rf \"$f\"; done\n```\nThat is all.");
  assert.equal(said, "Run this: Code, on screen. That is all.");
  // A fence still being written holds the rest of the answer inside it, and
  // that is exactly when this runs: the answer arrives as it is typed.
  assert.match(worthSaying("Here:\n```sh\nls -l"), /Code, on screen/);
});

test("a filename keeps its words, because that is what was being said", () => {
  // Inline code is nearly always a name or a flag. Dropping it would drop the
  // answer; reading its backticks would be reading punctuation.
  assert.equal(worthSaying("it is in `app.js` now"), "it is in app.js now");
});

test("a link is heard by its words and never by its address", () => {
  assert.equal(worthSaying("see [the notes](https://example.com/a/b?c=1)"), "see the notes");
  assert.equal(worthSaying("see https://example.com/a/b?c=1 for it"), "see a link for it");
});

test("a table is named rather than read as a row of pipes", () => {
  const said = worthSaying("Here:\n| a | b |\n|---|---|\n| 1 | 2 |\n\nDone.");
  assert.match(said, /A table, on screen/);
  assert.doesNotMatch(said, /\|/);
});

test("including its last row, when the answer ends with the table", () => {
  // The ordinary shape, and the one that was left behind: a settled answer is
  // trimmed, so the final row has nothing after it. It was read out as pipes
  // immediately after announcing that the table was on screen.
  assert.doesNotMatch(worthSaying("Here:\n| a | b |\n|---|---|\n| 1 | 2 |"), /\|/);
  assert.doesNotMatch(worthSaying("| a | b |\n|---|---|"), /\|/);
});

test("emphasis with something starred inside it still loses its marks", () => {
  // A rule that cannot see past an asterisk left `**` in and the voice read it
  // out, which is the "star star Price colon star star" this module exists to
  // stop, arriving through the one case nobody writes a test for.
  assert.equal(worthSaying("**run `a*b`** now"), "run a*b now");
  assert.equal(worthSaying("- **Total: *about* five** pounds"), "Total: about five pounds");
  // And two bold spans in a sentence are still two, rather than everything
  // between the first and the last being swallowed.
  assert.equal(worthSaying("**one** and **two**"), "one and two");
});

test("an underscore inside a name is not emphasis", () => {
  // `my_var_name` is a name, and reading it as "my var name" would be reading
  // something that was never written.
  assert.equal(worthSaying("a file called my_var_name.txt"), "a file called my_var_name.txt");
});

test("a long answer stops on a sentence and says where the rest is", () => {
  const long = "This is a sentence about something. ".repeat(40);
  const said = enoughOfIt(long);
  assert.ok(said.length < 900, `${said.length} characters`);
  // Not cut mid-clause: a voice that stops halfway sounds like one that
  // crashed, and somebody waits for the rest that never comes.
  assert.match(said, /something\. The rest of it is on screen\.$/);
});

test("one very long sentence still stops at a word", () => {
  // Every word different, so where it stops can be seen. The old fixture
  // repeated one word, and cutting mid-word produced a fragment that also
  // appeared whole elsewhere in the string: the check passed with the
  // word-boundary cut deleted entirely.
  const words = Array.from({ length: 400 }, (_, i) => `word${i}`);
  const said = enoughOfIt(words.join(" "));
  assert.ok(said.length < 900, `${said.length} characters`);
  // Whatever it ends on has to be a whole one of those words.
  const last = said.replace(/ The rest of it is on screen\.$/, "").split(" ").pop();
  assert.ok(words.includes(last), `ends mid-word: ${JSON.stringify(last)}`);
});

test("a short answer is said exactly as it is", () => {
  assert.equal(toSay("It is Sunday."), "It is Sunday.");
  assert.equal(enoughOfIt("short"), "short");
});

test("nothing to say is nothing said, rather than the word undefined", () => {
  assert.equal(worthSaying(""), "");
  assert.equal(worthSaying(null), "");
  assert.equal(toSay(""), "");
});
