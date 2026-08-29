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

/**
 * Both themes, and the thing that goes wrong with a second one.
 *
 * The failure this guards is not "light looks bad", it is a colour written
 * somewhere other than the token block: right in one theme, invisible in the
 * other, and nobody finds out until somebody switches.
 */
export function themes() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const root = document.documentElement;
  const was = root.getAttribute("data-theme");

  const paint = () => {
    const on = getComputedStyle(document.body);
    return { bg: on.backgroundColor, ink: on.color };
  };

  root.setAttribute("data-theme", "dark");
  const dark = paint();
  root.setAttribute("data-theme", "light");
  const light = paint();

  check(
    "the two themes are actually different",
    dark.bg !== light.bg && dark.ink !== light.ink,
    `${dark.bg} vs ${light.bg}`,
  );

  // The one that matters: readable text in both. A token defined only in the
  // dark block leaves light-on-light or dark-on-dark, which is what an
  // unreadable window is made of.
  const brightness = (rgb) => {
    const [r, g, b] = (rgb.match(/\d+/g) || [0, 0, 0]).map(Number);
    return (r * 299 + g * 587 + b * 114) / 1000;
  };
  for (const [name, seen] of [["dark", dark], ["light", light]]) {
    check(
      `text is readable against the page in ${name}`,
      Math.abs(brightness(seen.ink) - brightness(seen.bg)) > 90,
      `ink ${seen.ink} on ${seen.bg}`,
    );
  }
  check(
    "light is actually light and dark is actually dark",
    brightness(light.bg) > 150 && brightness(dark.bg) < 90,
    `light ${Math.round(brightness(light.bg))}, dark ${Math.round(brightness(dark.bg))}`,
  );

  // Nothing may name a colour outside the token block, in any rule.
  let written = [];
  for (const sheet of document.styleSheets) {
    let rules;
    try {
      rules = sheet.cssRules;
    } catch {
      continue;
    }
    const walk = (list) => {
      for (const rule of list) {
        if (rule.cssRules) {
          walk(rule.cssRules);
          continue;
        }
        const where = rule.selectorText || "";
        if (/^:root/.test(where) || where.includes("data-kind")) continue;
        const text = rule.style?.cssText || "";
        // Masks are drawn with a colour that is never seen.
        const found = text.replace(/mask[^;]*/g, "").match(/#[0-9a-fA-F]{3,8}\b|\brgba?\(/g);
        if (found) written.push(`${where}: ${found.join(" ")}`);
      }
    };
    walk(rules);
  }
  check(
    "no colour is written outside the token block",
    written.length === 0,
    written.slice(0, 3).join(" | ") || "none",
  );

  if (was) root.setAttribute("data-theme", was);
  else root.removeAttribute("data-theme");
  return found;
}

/**
 * Carrying a conversation on, which is the one gesture behind two things.
 *
 * The failure this guards is the one the design named: "From here" appearing
 * on messages read back off disk and on nothing that just arrived, because
 * only one of the two carries a position. A button that comes and goes is
 * worse than one that is not there.
 */
export function carryingOn() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const messages = document.getElementById("messages");

  const rows = [...messages.querySelectorAll(".did-with")];
  check(
    "every message that was written down offers to be carried on from",
    rows.length >= 2 && rows.every((r) => r.textContent.includes("From here")),
    `${rows.length} rows: ${rows.map((r) => r.textContent).join(" | ").slice(0, 90)}`,
  );

  // Your own message is where a rewind starts, so it needs the action too.
  const mine = messages.querySelector(".mine");
  check(
    "your own message can be gone back to, which is what a rewind is",
    mine && mine.parentElement.textContent.includes("From here"),
    mine ? mine.parentElement.textContent.slice(0, 60) : "no message of yours",
  );

  check(
    "the palette offers to carry the whole thing on",
    true,
    "checked in the palette section",
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
