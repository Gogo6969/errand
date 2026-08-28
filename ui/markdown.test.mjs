// Does the renderer read what the agent actually wrote?
//
//     node --test ui/markdown.test.mjs
//
// The window has no build step and no packages, and a test is not a good enough
// reason to acquire either, so the handful of DOM calls the renderer makes are
// stood up here in forty lines. It only has to be right about `append`,
// `textContent` and attributes, which is all the renderer touches.
//
// Two of these guard things that were wrong once and were invisible when they
// were: a back-reference pointing at the wrong group, which turned `**Price:**`
// into a bold "P" and a loose asterisk, and the rule that nothing a model wrote
// ever becomes markup.

import { test } from "node:test";
import assert from "node:assert/strict";

// ------------------------------------------------------------- a small DOM --

class Node {
  constructor(tag) {
    this.tag = tag;
    this.children = [];
    this.attributes = {};
    this.dataset = {};
  }
  append(...things) {
    for (const thing of things) {
      if (typeof thing === "string") this.children.push(thing);
      else if (thing.tag === "#fragment") this.children.push(...thing.children);
      else this.children.push(thing);
    }
  }
  set className(v) {
    this.attributes.class = v;
  }
  set href(v) {
    this.attributes.href = v;
  }
  set rel(v) {
    this.attributes.rel = v;
  }
  set textContent(v) {
    this.children = [v];
  }
  get textContent() {
    return this.children.map((c) => (typeof c === "string" ? c : c.textContent)).join("");
  }
  /** Every element of this kind, at any depth. */
  all(tag) {
    const found = [];
    for (const child of this.children) {
      if (typeof child === "string") continue;
      if (child.tag === tag) found.push(child);
      found.push(...child.all(tag));
    }
    return found;
  }
  /** The shape of it, as tag names, for asserting on structure. */
  get shape() {
    return this.children
      .filter((c) => typeof c !== "string")
      .map((c) => c.tag)
      .join(" ");
  }
}

globalThis.document = {
  createElement: (tag) => new Node(tag),
  createDocumentFragment: () => new Node("#fragment"),
  createTextNode: (text) => {
    const n = new Node("#text");
    n.children = [text];
    return n;
  },
};

const { render } = await import("./markdown.js");

/** Render, and hand back something to ask questions of. */
function drawn(text) {
  const holder = new Node("div");
  holder.append(render(text));
  return holder;
}

// ------------------------------------------------------------------ tests --

test("a bold run is bold, and does not leave a stray asterisk behind it", () => {
  const out = drawn("**Price:** about $79,900.");
  assert.equal(out.all("strong").length, 1);
  assert.equal(out.all("strong")[0].textContent, "Price:");
  assert.equal(out.textContent, "Price: about $79,900.");
});

test("a list item keeps its emphasis and swallows the line it wrapped onto", () => {
  const out = drawn("- **The rally was violent.** BTC gained 20%\n  and reclaimed $80K.");
  const items = out.all("li");
  assert.equal(items.length, 1, "a wrapped line is not a second bullet");
  assert.equal(items[0].textContent, "The rally was violent. BTC gained 20% and reclaimed $80K.");
});

test("a link becomes something to click and keeps the address it was given", () => {
  const out = drawn("Sources: [Yahoo Finance](https://finance.yahoo.com/x) and https://coindesk.com/p");
  const links = out.all("a");
  assert.equal(links.length, 2, "one written out, one bare");
  assert.equal(links[0].textContent, "Yahoo Finance");
  assert.equal(links[0].attributes.href, "https://finance.yahoo.com/x");
  assert.equal(links[1].attributes.href, "https://coindesk.com/p", "a bare address is still a link");
});

test("a link that is not one of the kinds worth following stays as the text it was", () => {
  // The one that matters. This text was written by a model, which read it off a
  // page an hour ago, which anybody can write.
  const out = drawn("[click me](javascript:alert(1))");
  assert.equal(out.all("a").length, 0);
  assert.match(out.textContent, /\[click me\]\(javascript:alert\(1\)\)/);
});

test("markup in what the model wrote never becomes markup", () => {
  const out = drawn('An <img src=x onerror="alert(2)"> and a <script>alert(3)</script>.');
  assert.equal(out.all("img").length, 0);
  assert.equal(out.all("script").length, 0);
  assert.match(out.textContent, /<script>alert\(3\)<\/script>/, "shown as the text it is");
});

test("a fenced block is kept exactly, including anything in it that looks like markdown", () => {
  const out = drawn("```bash\ncurl -s 'https://x/**y**'\n# not a heading\n```");
  const pre = out.all("pre");
  assert.equal(pre.length, 1);
  assert.equal(pre[0].dataset.language, "bash");
  assert.equal(pre[0].textContent, "curl -s 'https://x/**y**'\n# not a heading");
  assert.equal(out.all("strong").length, 0, "nothing inside a fence is read as markdown");
});

test("a table needs its separator, or it is a paragraph that happens to contain pipes", () => {
  const real = drawn("| Route | Result |\n|---|---|\n| `WebSearch` | worked |");
  assert.equal(real.all("table").length, 1);
  assert.deepEqual(
    real.all("tr").map((r) => r.children.map((c) => c.textContent)),
    [
      ["Route", "Result"],
      ["WebSearch", "worked"],
    ],
  );

  const not = drawn("I ran a | b | c and it worked.");
  assert.equal(not.all("table").length, 0);
});

test("the pieces of a real answer all arrive, in the order they were written", () => {
  const out = drawn(
    "# Bitcoin\n\nA line.\n\n- one\n- two\n\n> quoted\n\n---\n\nAfter the rule.",
  );
  assert.equal(out.shape, "h1 p ul blockquote hr p");
});

test("an empty answer renders nothing rather than an empty bubble full of nothing", () => {
  assert.equal(drawn("").children.length, 0);
  assert.equal(drawn("   \n\n  ").children.length, 0);
});
