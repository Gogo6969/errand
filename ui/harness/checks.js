// What somebody looking at the window would check, written down so nobody has
// to look. Each one names the thing that was actually found wrong.

import { asked } from "./harness.js";

const has = (id) => document.getElementById(id);
const text = (id) => (has(id) ? has(id).textContent.trim() : "<missing>");
const options = (id) => (has(id) ? [...has(id).options].map((o) => o.textContent) : []);

export function checks() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  // The regression that started all this: both pickers empty and the header
  // saying "Nothing open" while a conversation was plainly on screen.
  check(
    "the header names the agent that is open, rather than saying nothing is",
    text("thread-name") && text("thread-name") !== "Nothing open",
    text("thread-name"),
  );
  check(
    "the engine picker has something in it",
    options("engine").length > 0,
    `${options("engine").length} options`,
  );
  check(
    "the conversation picker has something in it",
    options("talks").length > 0,
    `${options("talks").length} options`,
  );

  // A picker that lists engines but cannot say which is chosen is the same
  // bug wearing a different hat.
  check(
    "one engine is marked as the one in use",
    has("engine") && has("engine").selectedIndex >= 0,
    `index ${has("engine")?.selectedIndex}`,
  );
  check(
    "the models are named rather than all being called Claude",
    options("engine").filter((o) => o.startsWith("Claude")).length > 1,
    options("engine").filter((o) => o.startsWith("Claude")).join(" | "),
  );
  check(
    "there is a way to look on the network",
    options("engine").some((o) => o.toLowerCase().includes("network")),
    options("engine").at(-1),
  );

  // What was said in the conversation is what the window is for.
  check(
    "the conversation on screen has the lines that were said in it",
    has("messages") && has("messages").children.length > 0,
    `${has("messages")?.children.length} lines`,
  );
  check(
    "the agents are listed down the side",
    has("threads") && has("threads").children.length > 0,
    `${has("threads")?.children.length} rows`,
  );

  // The window has to have asked for the things it shows.
  check(
    "it asked what could answer",
    asked.some((a) => a.name === "engines"),
    asked.map((a) => a.name).join(","),
  );

  return found;
}

/** The palette, which is where everything the header cannot hold now lives. */
export function palette() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const box = document.getElementById("palette");
  const list = document.getElementById("palette-list");
  const typing = document.getElementById("palette-what");

  check("it starts closed", box.hidden, `hidden=${box.hidden}`);

  // Command-K, the thing everybody tries first.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  check("cmd-K opens it", !box.hidden, `hidden=${box.hidden}`);
  check("it offers things to do", list.children.length > 0, `${list.children.length} rows`);
  check(
    "the first one is highlighted, so Enter does something predictable",
    list.children[0]?.getAttribute("aria-selected") === "true",
    list.children[0]?.getAttribute("aria-selected"),
  );
  check(
    "exporting the conversation is one of them",
    [...list.children].some((r) => r.textContent.includes("Export")),
    [...list.children].map((r) => r.textContent).join(" | ").slice(0, 120),
  );

  // Typing narrows it, in any word order.
  typing.value = "network models";
  typing.dispatchEvent(new Event("input"));
  check(
    "typing words in any order finds the thing",
    list.children.length === 1 && list.children[0].textContent.includes("network"),
    `${list.children.length}: ${list.children[0]?.textContent}`,
  );

  typing.value = "zzzz nothing like this";
  typing.dispatchEvent(new Event("input"));
  check(
    "something that matches nothing says so rather than showing an empty box",
    list.textContent.toLowerCase().includes("nothing"),
    list.textContent.slice(0, 60),
  );

  // Arrow keys move the highlight rather than the page.
  typing.value = "";
  typing.dispatchEvent(new Event("input"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  check(
    "the arrow keys move the highlight",
    list.children[1]?.getAttribute("aria-selected") === "true",
    [...list.children].map((r) => r.getAttribute("aria-selected")).join(","),
  );

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  check("escape closes it", box.hidden, `hidden=${box.hidden}`);
  return found;
}

/**
 * The setup check, which is only worth having if it says what to do.
 *
 * Returns a promise, because it asks the app rather than reading the page.
 */
export async function setupCheck() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const panel = document.getElementById("checkup");

  check("the setup check starts closed", panel.hidden, `hidden=${panel.hidden}`);

  // Opened the way somebody would: through the palette.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  const typing = document.getElementById("palette-what");
  typing.value = "check setup";
  typing.dispatchEvent(new Event("input"));
  const list = document.getElementById("palette-list");
  check(
    "the palette offers to check the setup",
    list.children.length === 1 && list.children[0].textContent.includes("Check"),
    list.children[0]?.textContent,
  );
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise((r) => setTimeout(r, 250));

  check("running it opens the panel", !panel.hidden, `hidden=${panel.hidden}`);
  check(
    "it counts what wants attention rather than only listing everything",
    panel.textContent.includes("2 of 3"),
    panel.textContent.slice(0, 60),
  );
  check(
    "something broken says what to do about it",
    panel.textContent.includes("~/.claude.json"),
    panel.textContent.includes("~/.claude.json") ? "yes" : panel.textContent.slice(0, 80),
  );
  check(
    "a broken thing and an odd thing are told apart",
    panel.querySelector('[data-how="broken"]') && panel.querySelector('[data-how="odd"]'),
    [...panel.querySelectorAll("[data-how]")].map((f) => f.dataset.how).join(","),
  );
  return found;
}

/** Everything in the header on one row, which is what a header is. */
export function headerFitsOnOneRow() {
  const title = document.getElementById("title");
  // Only what is on screen. A hidden child measures zero and would otherwise
  // count as a row of its own, which is a test failing at its own reflection.
  const showing = [...title.children].filter((c) => c.getBoundingClientRect().width > 0);
  const tops = new Set(showing.map((c) => Math.round(c.getBoundingClientRect().top / 10)));
  const tools = document.getElementById("reach").getBoundingClientRect();
  const bar = title.getBoundingClientRect();
  return {
    rows: tops.size,
    toolsInside: tools.width > 0 && tools.right <= bar.right - 17,
  };
}
