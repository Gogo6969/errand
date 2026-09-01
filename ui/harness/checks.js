// What somebody looking at the window would check, written down so nobody has
// to look. Each one names the thing that was actually found wrong.

// The same instance the page is using, not a second one.
//
// Everything here is loaded with a cache-busting query so the harness can never
// test yesterday's code. That query has to be carried across this import too:
// `./harness.js` and `./harness.js?at=1` are two different modules to a browser,
// with two different `asked` arrays and two different fixtures, and the checks
// would then be reading an empty log of things the page never asked *this* copy.
const { asked, FIXTURE, tell } = await import(
  `./harness.js${new URL(import.meta.url).search}`
);

/**
 * Open one of the fixture's conversations, the way somebody would.
 *
 * The agent that owns it first: the conversation picker only ever lists the
 * conversations of whoever is open, so setting it to somebody else's does
 * nothing at all and quietly leaves the wrong thread on screen.
 */
async function openTalk(id) {
  const owner = Object.entries(FIXTURE.conversations).find(([, talks]) =>
    talks.some((t) => t.id === id),
  )?.[0];
  const rows = [...document.querySelectorAll("#threads li")];
  const at = FIXTURE.agents.findIndex((a) => a.id === owner);
  rows[at]?.click();
  await new Promise((r) => setTimeout(r, 300));

  const picker = document.getElementById("talks");
  picker.value = id;
  picker.dispatchEvent(new Event("change"));
  await new Promise((r) => setTimeout(r, 400));
}

/**
 * Whether this window is being drawn at all.
 *
 * A layout check reads `getBoundingClientRect`, and a window with no size
 * answers every one of those with zero -- so twelve checks report the app
 * broken with tops of -113 and boxes "0px wide", when nothing is wrong except
 * that nobody is looking at it. The header check has always said "cannot be
 * judged" in that case; everything else failed instead, and I have now chased
 * it three times.
 */
const beingDrawn = () => window.innerWidth > 0 && window.innerHeight > 0;

/** A check that can only be judged when the window has a size. */
function whenDrawn(found, what, judge) {
  if (!beingDrawn()) {
    found.push({
      what,
      ok: true,
      saw: "cannot be judged: this window has no size, so every measurement is zero. Show the window and run again.",
    });
    return;
  }
  const { ok, saw } = judge();
  found.push({ what, ok: !!ok, saw });
}

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
  // Looking used to be an option inside the picker, which meant the picker
  // was a search. It is not any more, and the check that used to demand it
  // now demands the opposite: opening a picker must not be an invitation to
  // wait. Where looking lives instead is checked in `whichModels`.
  check(
    "the picker offers nothing that goes looking",
    !options("engine").some((o) => /network|look/i.test(o)),
    options("engine").join(" | "),
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

  // The one that cost real money before it was noticed. Resuming a session
  // does not only reload a transcript: a message an engine was sent and killed
  // before finishing is queued inside its own session and runs again on the
  // next resume. So merely opening the window ran an errand nobody had asked
  // for that minute, and ran it again on every restart.
  check(
    "looking at a conversation does not start an engine",
    !asked.some((a) => a.name === "open_thread"),
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
  typing.value = "up models";
  typing.dispatchEvent(new Event("input"));
  check(
    "typing words in any order finds the thing",
    list.children.length === 1 && list.children[0].textContent.includes("models"),
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
  // Three from the app and three the window answers for itself, and the count
  // has to be of all of them: a check that reported only half the setup would
  // be a check somebody trusted for the wrong half.
  check(
    "it counts everything it checked, including what only the window can answer",
    /2 of 6 things want attention/.test(panel.textContent),
    panel.textContent.slice(0, 70),
  );
  check(
    "it says what this window itself can and cannot do",
    ["Dictation", "Microphone", "Remembering"].every((w) => panel.textContent.includes(w)),
    panel.textContent.slice(0, 140),
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

/**
 * Seeing what is running somewhere nobody is looking.
 *
 * The whole reason this panel exists: the window knows only about
 * conversations somebody has opened, and a routine firing at seven on an agent
 * nobody has clicked is exactly the work worth being able to see.
 */
/**
 * Stopping says it stopped.
 *
 * A turn ends in the window when an ending arrives from the engine. Killing the
 * engine means no ending ever arrives, so the conversation went on saying
 * "Working" and offering to stop something that had stopped minutes ago. Seen
 * on screen, after the same fault had already been fixed twice behind it.
 */
export async function stopping() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  const open = () =>
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  const ask = (what) => {
    const typing = document.getElementById("palette-what");
    typing.value = what;
    typing.dispatchEvent(new Event("input"));
  };
  const offered = () => [...document.querySelectorAll("#palette-list li")].map((l) => l.textContent);

  // Driven the way somebody drives it: send something, which is what makes a
  // conversation working in the first place.
  const what = document.getElementById("what");
  what.value = "Count from 1 to 3000.";
  what.dispatchEvent(new Event("input"));
  document.getElementById("composer").dispatchEvent(new Event("submit", { cancelable: true }));
  await new Promise((r) => setTimeout(r, 300));
  check(
    "sending something makes it working",
    document.getElementById("threads").textContent.includes("Working"),
    document.getElementById("threads").textContent.includes("Working") ? "Working" : "not working",
  );

  open();
  ask("stop what");
  await new Promise((r) => setTimeout(r, 150));
  check(
    "a conversation that is working can be stopped",
    offered().some((l) => l.includes("Stop what it is doing")),
    offered().join(" / ") || "nothing offered",
  );

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise((r) => setTimeout(r, 300));

  const stillSaysWorking = document.getElementById("threads").textContent.includes("Working");
  const stillThinking = !!document.getElementById("messages").querySelector(".thinking");
  check(
    "it stops saying it is working",
    !stillSaysWorking && !stillThinking,
    [stillSaysWorking && "the list still says Working", stillThinking && "the dots are still there"]
      .filter(Boolean)
      .join(" and ") || "clear",
  );

  open();
  ask("stop what");
  await new Promise((r) => setTimeout(r, 150));
  check(
    "and stops offering to stop something that has stopped",
    !offered().some((l) => l.includes("Stop what it is doing")),
    offered().join(" / ") || "nothing offered",
  );
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  return found;
}

/**
 * What each posture actually does, said where it is chosen.
 *
 * "Never" is the one that has to say two things, not one. Switching the asking
 * off does not leave an agent with nothing in the way: it leaves it walled into
 * its own folder, because asking and a wall are the two mechanisms there are
 * and turning one off is exactly when the other goes up. Somebody choosing it
 * should know that before they choose, not meet it later as a refused write
 * they cannot explain.
 */
export async function postures() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 250));
  const asks = document.getElementById("asks");
  const means = document.getElementById("asks-means");

  const said = {};
  for (const how of ["plan", "ask", "edits", "auto"]) {
    asks.value = how;
    asks.dispatchEvent(new Event("change"));
    await new Promise((r) => setTimeout(r, 60));
    said[how] = means.textContent;
  }

  check(
    "every posture says what it does",
    Object.values(said).every((t) => t.length > 20),
    Object.entries(said)
      .map(([k, v]) => `${k}:${v.length}`)
      .join(" "),
  );
  check(
    "they do not all say the same thing",
    new Set(Object.values(said)).size === 4,
    `${new Set(Object.values(said)).size} distinct`,
  );
  check(
    "choosing never says the wall goes up, not just that it stops asking",
    said.auto.includes("walled") && said.auto.includes("own folder"),
    said.auto.slice(0, 90),
  );
  // What each granted rule covers, in words. Two rules that look alike on the
  // page are not alike at all: one covers every use of a program and the other
  // covers a single command line, and the rule text alone does not say which.
  const listed = document.getElementById("allowed").textContent;
  check(
    "what is already allowed says how much it covers",
    listed.includes("any top command") && listed.includes("only this exact command"),
    listed.slice(0, 110),
  );

  check(
    "and the postures that do ask do not claim to be walled",
    !said.ask.includes("walled") && !said.edits.includes("walled"),
    `${said.ask.slice(0, 40)} / ${said.edits.slice(0, 40)}`,
  );

  asks.value = "ask";
  asks.dispatchEvent(new Event("change"));
  document.getElementById("granted").click();
  return found;
}

/**
 * A goal, and where it has got to.
 *
 * The two things that have to be on screen are what it is aiming at and how
 * many turns it has spent, because between them they are the difference
 * between an agent working and an agent going round. Both were invisible in
 * every version of this before there was a panel.
 */
export async function aiming() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const panel = document.getElementById("aiming");

  check("it starts closed", panel.hidden, `hidden=${panel.hidden}`);
  document.getElementById("goal").click();
  await new Promise((r) => setTimeout(r, 250));
  check("the Goal button opens it", !panel.hidden, `hidden=${panel.hidden}`);

  check(
    "it shows what is being aimed at",
    document.getElementById("goal-what").value === "Get the tests passing",
    document.getElementById("goal-what").value,
  );

  const says = document.getElementById("goal-says").textContent;
  check(
    "it says how many turns have gone and how many there are, in numbers",
    says.includes("3 of 8 turns used"),
    says.slice(0, 60),
  );
  check(
    "it says what the agent itself said was left",
    says.includes("two of them still fail on a timeout"),
    says.slice(0, 120),
  );
  check(
    "and what it will do, before anybody agrees to it",
    says.includes("stops early if it says the same thing is left twice"),
    says.slice(-90),
  );

  // A goal that has ended says which of the four endings it was, because they
  // want different things done about them.
  FIXTURE.goal_of.over = "going round";
  await new Promise((r) => setTimeout(r, 4400));
  const ended = document.getElementById("goal-says").textContent;
  check(
    "when it ends it says which ending, not just that it stopped",
    ended.includes("same thing was left twice running"),
    ended.slice(0, 80),
  );
  FIXTURE.goal_of.over = null;

  document.getElementById("goal").click();
  check("clicking again closes it", panel.hidden, `hidden=${panel.hidden}`);

  // A goal ending is written by the app, not said by an agent and not typed by
  // anybody, and it is the one line that says why nothing more will happen.
  // Pushed in as its own kind it drew as nothing at all live and drew fine
  // after a reload, which is the worst way round: invisible exactly when it
  // matters and present when anybody goes looking for why.
  const before = document.querySelectorAll("#messages li").length;
  // Whichever conversation is actually on screen, since the point is that it
  // is drawn now rather than found later.
  const heard = tell("noted", {
    conversation: document.getElementById("talks").value,
    seq: 9990,
    kind: "goal",
    text: "Done: get the tests passing",
  });
  await new Promise((r) => setTimeout(r, 150));
  const shown = document.getElementById("messages").textContent;
  check(
    "the window is listening for a line the app writes itself",
    heard > 0,
    `${heard} listener(s)`,
  );
  check(
    "and draws it rather than silently dropping it",
    shown.includes("Done: get the tests passing") &&
      document.querySelectorAll("#messages li").length > before,
    shown.includes("Done: get the tests passing") ? "drawn" : "nothing appeared",
  );
  return found;
}

/**
 * Which models show up.
 *
 * Two things have to be true and both were false before there was a screen for
 * it. The picker must show exactly what somebody chose, and nothing may go
 * looking while it is open: the old one probed the machine every time it was
 * drawn, which is why it was slow and why what it offered changed under people.
 */
export async function whichModels() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const screen = document.getElementById("models");

  check("it starts closed", screen.hidden, `hidden=${screen.hidden}`);

  // The picker itself: what it shows and, more importantly, what it does not do.
  const askedBefore = asked.length;
  document.getElementById("engine").dispatchEvent(new Event("mousedown", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 200));
  const wentLooking = asked
    .slice(askedBefore)
    .some((a) => a.name === "look_for_models" || a.name === "engines");
  check(
    "opening the picker does not go looking for anything",
    !wentLooking,
    asked.slice(askedBefore).map((a) => a.name).join(",") || "asked nothing",
  );
  check(
    "and it offers no option that goes looking",
    ![...document.getElementById("engine").options].some((o) =>
      /network|look/i.test(o.textContent),
    ),
    [...document.getElementById("engine").options].map((o) => o.textContent).join(" / "),
  );
  check(
    "it shows what was chosen, and that is all",
    document.getElementById("engine").options.length === FIXTURE.offered.length,
    `${document.getElementById("engine").options.length} of ${FIXTURE.offered.length}`,
  );

  document.getElementById("setup").click();
  await new Promise((r) => setTimeout(r, 300));
  check("the gear opens it", !screen.hidden, `hidden=${screen.hidden}`);

  const chosen = document.getElementById("chosen").textContent;
  check(
    "it lists what is in the picker",
    chosen.includes("Claude - Opus") && chosen.includes("qwen2.5:7b"),
    chosen.slice(0, 80),
  );

  // The order is what the dropdown shows, top to bottom, so it has to be
  // somebody's to set. The ends have to be honest about being the ends: an up
  // arrow on the top line that looks pressable and does nothing is worse than
  // no arrow.
  const rows = [...document.querySelectorAll("#chosen li")];
  const nudges = (row) => [...row.querySelectorAll("button.nudge")];
  check(
    "every line can be moved up and down",
    rows.length > 1 && rows.every((r) => nudges(r).length === 2),
    `${rows.length} rows, ${nudges(rows[0] || document.createElement("li")).length} arrows on the first`,
  );
  check(
    "and the top and bottom say they are the top and bottom",
    nudges(rows[0])[0]?.disabled === true &&
      nudges(rows[rows.length - 1])[1]?.disabled === true &&
      nudges(rows[0])[1]?.disabled === false,
    `top up=${nudges(rows[0])[0]?.disabled} bottom down=${nudges(rows[rows.length - 1])[1]?.disabled}`,
  );

  const beforeMove = asked.length;
  nudges(rows[1])[0]?.click();
  await new Promise((r) => setTimeout(r, 200));
  const moved = asked.slice(beforeMove).find((a) => a.name === "move_it");
  check(
    "moving one asks the app to move that one",
    moved?.args?.id === FIXTURE.offered[1].id && moved?.args?.up === true,
    JSON.stringify(moved?.args) || "nothing was asked",
  );

  // Renaming, because the ones carried over from an agent already using them
  // were named after the agent, which is not what a model is called.
  const nowShowing = [...document.querySelectorAll("#chosen li")];
  const renameButton = [...nowShowing[0].querySelectorAll("button")].find(
    (b) => b.textContent === "Rename",
  );
  renameButton?.click();
  await new Promise((r) => setTimeout(r, 120));
  const box = document.querySelector("#chosen .renaming");
  check("a line can be renamed in place", box, box ? "a box appeared" : "no box");
  if (box) {
    const beforeName = asked.length;
    box.value = "The good one";
    box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await new Promise((r) => setTimeout(r, 250));
    const named = asked.slice(beforeName).find((a) => a.name === "call_it_something");
    check(
      "and the new name is sent",
      named?.args?.label === "The good one",
      JSON.stringify(named?.args) || "nothing was sent",
    );
  }
  check(
    "and everywhere models come from that was kept",
    document.getElementById("found").textContent.includes("DeepSeek"),
    document.getElementById("found").textContent.slice(0, 80),
  );
  check(
    "one that speaks the other protocol says so, since it changes what is sent",
    document.getElementById("found").textContent.includes("Anthropic protocol"),
    document.getElementById("found").textContent.slice(0, 130),
  );
  check(
    "a place with a key says so, and never shows it",
    document.getElementById("found").textContent.includes("key kept") &&
      !/sk-|Bearer/.test(document.getElementById("found").textContent),
    document.getElementById("found").textContent.slice(0, 110),
  );

  // Changing one that is already kept, rather than forgetting it and starting
  // again -- which is what rotating a key would otherwise mean, and it would
  // take every model chosen from it along with it.
  // The one that speaks the other protocol, so the check covers carrying that
  // back into the form as well as the address.
  const deepseek = [...document.querySelectorAll("#found li")].find((r) =>
    r.textContent.includes("DeepSeek"),
  );
  const change = [...(deepseek?.querySelectorAll("button") || [])].find(
    (b) => b.textContent === "Change",
  );
  check("a kept one can be changed", change, change ? "there is a way" : "no way to change one");
  change?.click();
  await new Promise((r) => setTimeout(r, 150));
  check(
    "changing one fills the form with what it already is",
    document.getElementById("hand-url").value === FIXTURE.backends[1].base_url &&
      document.getElementById("hand-wire").value === FIXTURE.backends[1].wire,
    `${document.getElementById("hand-url").value} as ${document.getElementById("hand-wire").value}`,
  );
  check(
    "and says it is changing rather than adding",
    document.getElementById("hand-save").textContent === "Save",
    document.getElementById("hand-save").textContent,
  );

  const beforeChange = asked.length;
  document.getElementById("by-hand").dispatchEvent(new Event("submit", { cancelable: true }));
  await new Promise((r) => setTimeout(r, 300));
  const changed = asked.slice(beforeChange).find((a) => a.name === "remember_backend");
  check(
    "saving a change names the one being changed, rather than making a second",
    changed?.args?.id === FIXTURE.backends[1].id,
    JSON.stringify(changed?.args?.id) || "no id was sent",
  );
  check(
    "and afterwards the form is back to adding",
    document.getElementById("hand-save").textContent === "Add",
    document.getElementById("hand-save").textContent,
  );


  // Looking is a thing you ask for, here, and it says what it found.
  const wasAsked = asked.length;
  document.getElementById("look-here").click();
  await new Promise((r) => setTimeout(r, 300));
  check(
    "looking happens here, when asked",
    asked.slice(wasAsked).some((a) => a.name === "look_for_models"),
    asked.slice(wasAsked).map((a) => a.name).join(",") || "asked nothing",
  );
  check(
    "and what it found is marked loaded or not, one by one",
    document.getElementById("found").querySelector(".lit:not(.cold)") &&
      document.getElementById("found").querySelector(".lit.cold"),
    `${document.getElementById("found").querySelectorAll(".lit").length} models`,
  );

  // Adding one by hand, which is the only way to reach a hosted provider.
  document.querySelectorAll("#presets button")[0]?.click();
  check(
    "picking a known provider fills the address in",
    document.getElementById("hand-url").value.startsWith("https://"),
    document.getElementById("hand-url").value,
  );
  check(
    "and says whether that address was actually checked",
    document.getElementById("hand-says").textContent.length > 20,
    document.getElementById("hand-says").textContent.slice(0, 70),
  );

  // The protocol travels with the address. The same host serves both at
  // different paths, so either one on its own is a setup that cannot work.
  const anth = [...document.querySelectorAll("#presets button")].find((b) =>
    b.textContent.includes("Anthropic"),
  );
  anth?.click();
  check(
    "an Anthropic-protocol address brings its protocol with it",
    document.getElementById("hand-url").value.endsWith("/anthropic") &&
      document.getElementById("hand-wire").value === "anthropic",
    `${document.getElementById("hand-url").value} as ${document.getElementById("hand-wire").value}`,
  );
  document.querySelectorAll("#presets button")[0]?.click();
  check(
    "and choosing an ordinary one puts the protocol back",
    document.getElementById("hand-wire").value === "openai",
    document.getElementById("hand-wire").value,
  );

  document.querySelectorAll("#presets button")[0]?.click();
  document.getElementById("hand-key").value = "sk-not-a-real-key";
  const beforeAdd = asked.length;
  document.getElementById("by-hand").dispatchEvent(new Event("submit", { cancelable: true }));
  await new Promise((r) => setTimeout(r, 300));
  const sent = asked.slice(beforeAdd).find((a) => a.name === "remember_backend");
  check("adding one by hand sends it to the app", sent, sent ? "sent" : "nothing was sent");
  check(
    "and sends which protocol it speaks",
    sent?.args?.wire === "openai" || sent?.args?.wire === "anthropic",
    JSON.stringify(sent?.args?.wire),
  );
  check(
    "the key is not left sitting in the page afterwards",
    document.getElementById("hand-key").value === "",
    `field holds ${document.getElementById("hand-key").value.length} characters`,
  );

  document.getElementById("models-done").click();
  check("back to chat closes it", screen.hidden, `hidden=${screen.hidden}`);

  // Taking away the very thing the open agent is set to, which is a thing
  // somebody will do, and the picker has to keep telling the truth about what
  // it is on afterwards -- and the true reason it is not there. The old words
  // said "not running", which would send somebody to go and check a server
  // that is perfectly well.
  //
  // Opened here rather than assumed. This used to read whichever agent an
  // earlier group happened to leave on screen, so adding a check anywhere
  // above it broke this one, which is a test failing at another test rather
  // than at the app.
  await openTalk("talk-1");
  document.getElementById("setup").click();
  await new Promise((r) => setTimeout(r, 250));
  const kept = FIXTURE.offered;
  const mine = [...document.querySelectorAll("#chosen li button")].find(
    (b) => b.textContent === "Remove",
  );
  FIXTURE.offered = kept.slice(1);
  mine?.click();
  await new Promise((r) => setTimeout(r, 300));

  const options = [...document.getElementById("engine").options].map((o) => o.textContent);
  check(
    "an agent on something no longer in the list still says what it is on",
    options.some((o) => o.includes("not in the list")),
    options.join(" / "),
  );
  check(
    "and says the true reason, rather than blaming the server",
    !options.some((o) => /not running/i.test(o)),
    options.join(" / "),
  );

  FIXTURE.offered = kept;
  document.getElementById("models-done").click();
  return found;
}

/**
 * A question that can still be answered, and one that cannot.
 *
 * On disk they are the same row: a question with nothing written against it.
 * Only whether the engine is still there tells them apart, and drawing the live
 * one as expired made an errand started from outside impossible to answer --
 * the card said the question had gone while the engine sat waiting for it.
 */
export async function questionsStillOpen() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  await openTalk("talk-3");
  const live = document.getElementById("messages").textContent;
  check(
    "a question whose engine is still there can still be answered",
    live.includes("Fetch BTC spot price") && !live.includes("expired"),
    live.includes("expired") ? "it says the question expired" : live.slice(0, 70),
  );

  await openTalk("talk-2");
  const gone = document.getElementById("messages").textContent;
  check(
    "and one whose engine has gone says so, rather than offering a button that does nothing",
    gone.includes("Delete the old backups") && gone.includes("expired"),
    gone.slice(0, 90),
  );
  // Where the way out of being asked belongs: in front of somebody who has
  // just answered three questions, not behind a button called Allowed that
  // nobody looks at while being interrupted.
  await openTalk("talk-4");
  // The shape the app actually sends: the event flattened onto the message,
  // with `kind` naming which one it is.
  tell("happened", {
    conversation: "talk-4",
    seq: 4,
    kind: "needs_you",
    asking: "Fetch the price",
    detail: "curl -s https://example.com/price",
    tool: "Bash",
    call: "a4",
    step: "a4",
    can_remember: true,
    rule: "curl",
    allows: "any curl command",
  });
  await new Promise((r) => setTimeout(r, 300));

  const card = document.querySelector("#messages .asking .choices");
  check("a live question offers a way to answer it", card, card ? "buttons" : "no buttons");
  const always = [...(card?.querySelectorAll("button") || [])].find((b) =>
    b.textContent.startsWith("Always"),
  );
  check(
    "and Always says how wide it is before it is pressed",
    always?.textContent === "Always · any curl command",
    always?.textContent || "no Always button",
  );
  check(
    "and after three of these there is a way to stop being asked at all",
    [...(card?.querySelectorAll("button") || [])].some((b) => b.textContent === "Stop asking me"),
    [...(card?.querySelectorAll("button") || [])].map((b) => b.textContent).join(" / "),
  );

  return found;
}

/**
 * Everything on a row is one height and one line.
 *
 * Not fussiness. The labels in these panels sit above their controls and are
 * bottom-aligned to them, so a control two pixels shorter than its neighbours
 * drops its own label below the rest of the row -- which is what a native
 * select does, and what an outlined button does beside a filled one. Two pixels
 * is enough to read as sloppy, and it read as sloppy.
 */
export async function everythingLinesUp() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  // Nothing here can be judged in a window with no size: every measurement
  // comes back zero and every check below reports the app broken, with tops of
  // -113 and boxes "0px wide". Said rather than failed, the way the header
  // check has always said it.
  if (!beingDrawn()) {
    found.push({
      what: "everything on a row lines up",
      ok: true,
      saw: "cannot be judged: this window has no size. Show the window and run again.",
    });
    return found;
  }

  const row = (within) =>
    [...document.querySelectorAll(`${within} input, ${within} select, ${within} button`)].filter(
      (e) => e.offsetParent,
    );
  const measure = (within) => {
    const all = row(within);
    return {
      count: all.length,
      heights: [...new Set(all.map((e) => Math.round(e.getBoundingClientRect().height)))],
      tops: [...new Set(all.map((e) => Math.round(e.getBoundingClientRect().top)))],
    };
  };

  // None of these is one row any more, and none of them should be. A goal is
  // one long sentence with its buttons under it; Repeat and Watch each ask
  // three questions and offer three answers, and Watch asks a third since
  // "every 10m" stopped being something somebody had to know how to type.
  //
  // What is worth holding is what somebody complained about: every control the
  // same height, and the ones sharing a line sharing it exactly. Asserting the
  // whole panel is one row would now be asserting something nobody wants.
  const panels = [
    ["repeat", "#routine", { oneLine: false }],
    ["watch", "#watching", { oneLine: false }],
    ["goal", "#aiming", { oneLine: false }],
  ];
  for (const [button, panel, how] of panels) {
    document.getElementById(button).click();
    await new Promise((r) => setTimeout(r, 350));
    const seen = measure(panel);
    check(
      `everything on one row in ${panel} is one height`,
      seen.count > 1 && seen.heights.length === 1,
      `${seen.count} controls, heights ${seen.heights.join("/")}`,
    );
    check(
      how.oneLine
        ? `and sits on one line in ${panel}`
        : `and what shares a line in ${panel} lines up`,
      how.oneLine
        ? seen.count > 1 && seen.tops.length === 1
        : seen.count > 1 && seen.tops.length <= 2,
      `tops ${seen.tops.join("/")}`,
    );
    document.getElementById(button).click();
    await new Promise((r) => setTimeout(r, 200));
  }

  // The one that was actually complained about, and the only row with a select
  // in it. A native select ignores the height it is given, so this is the row
  // where being one height had to be made to happen rather than assumed.
  document.getElementById("setup").click();
  await new Promise((r) => setTimeout(r, 450));
  const hand = measure("#by-hand");
  check(
    "every field in the add-by-hand row is one height",
    hand.heights.length === 1,
    `heights ${hand.heights.join("/")}`,
  );
  check(
    "and they all start on the same line",
    hand.tops.length === 1,
    `tops ${hand.tops.join("/")}`,
  );
  const labelTops = [
    ...new Set(
      [...document.querySelectorAll("#by-hand label")].map((l) =>
        Math.round(l.getBoundingClientRect().top),
      ),
    ),
  ];
  check(
    "so every label above them starts on the same line too",
    labelTops.length === 1,
    `tops ${labelTops.join("/")}`,
  );
  document.getElementById("models-done").click();
  await new Promise((r) => setTimeout(r, 200));
  return found;
}

/**
 * Whether something else already does this, said while it is being typed.
 *
 * Errand did not check at all: two agents could be given the same job every
 * morning and nothing anywhere said so. Two identical briefings at seven is how
 * somebody finds out, which is a week later and by accident.
 */
export async function alreadyRunning() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  document.getElementById("repeat").click();
  await new Promise((r) => setTimeout(r, 300));

  const at = document.getElementById("routine-at");
  const what = document.getElementById("routine-what");
  const type = async (when, doing) => {
    at.value = when;
    what.value = doing;
    what.dispatchEvent(new Event("input"));
    await new Promise((r) => setTimeout(r, 250));
    return document.getElementById("routine-says").textContent;
  };

  const clashing = await type("daily 07:00", "Brief me on bitcoin");
  check(
    "typing one that something else already does says so, and says who",
    clashing.includes("Bitcoin Desk already does almost exactly this"),
    clashing.slice(0, 90),
  );
  check(
    "and still says when this one would run, which is what the panel is for",
    /runs only when you ask|Next |nothing due/.test(clashing),
    clashing.slice(0, 90),
  );

  const fine = await type("daily 07:00", "Check whether the backups ran");
  check(
    "and says nothing when nothing else does it",
    !fine.includes("already does"),
    fine.slice(0, 80),
  );

  document.getElementById("repeat").click();
  await new Promise((r) => setTimeout(r, 150));
  return found;
}

/**
 * What it has cost.
 *
 * The engine says on every turn and this app threw it away, so there was no
 * answer at all to the one question anybody running errands has.
 */
export async function whatItCost() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const panel = document.getElementById("costing");

  check("it starts closed", panel.hidden, `hidden=${panel.hidden}`);

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  const typing = document.getElementById("palette-what");
  typing.value = "what it has cost";
  typing.dispatchEvent(new Event("input"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise((r) => setTimeout(r, 300));

  check("it opens", !panel.hidden, `hidden=${panel.hidden}`);
  const said = panel.textContent;
  check(
    "it answers today and this month separately, since they are two questions",
    said.includes("Today: $0.19") && said.includes("This month: $4.70"),
    said.slice(0, 120),
  );
  check(
    "and says turns as well as money, since one errand going round thirty times is the one to look at",
    said.includes("12 errands, 30 turns"),
    said.slice(0, 160),
  );
  check(
    "spending outlives the agent that did it",
    said.includes("an agent that is gone"),
    said.slice(0, 160),
  );

  // Nothing paid for is not the same as nothing loaded: somebody running only
  // local models should be told why this is empty rather than left to wonder.
  const was = FIXTURE.what_it_cost;
  FIXTURE.what_it_cost = { today: [], this_month: [], nothing_yet: true };
  document.getElementById("costing").hidden = false;
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  typing.value = "what it has cost";
  typing.dispatchEvent(new Event("input"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise((r) => setTimeout(r, 250));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  typing.value = "what it has cost";
  typing.dispatchEvent(new Event("input"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise((r) => setTimeout(r, 300));
  check(
    "nothing paid for says why, rather than looking like nothing loaded",
    document.getElementById("costing").textContent.includes("costs no money"),
    document.getElementById("costing").textContent.slice(0, 110),
  );
  FIXTURE.what_it_cost = was;
  return found;
}

export async function running() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const panel = document.getElementById("working");

  check("it starts closed", panel.hidden, `hidden=${panel.hidden}`);

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  const typing = document.getElementById("palette-what");
  typing.value = "what is running";
  typing.dispatchEvent(new Event("input"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise((r) => setTimeout(r, 250));

  check("it opens", !panel.hidden, `hidden=${panel.hidden}`);
  check(
    "it counts what is waiting on somebody separately from what is merely busy",
    panel.textContent.includes("3 running, 1 stopped waiting"),
    panel.textContent.slice(0, 70),
  );
  check(
    "what is waiting on somebody is marked, since it is the only kind that will not finish",
    panel.querySelector('[data-waiting="true"]'),
    [...panel.querySelectorAll("[data-waiting]")].map((r) => r.dataset.waiting).join(","),
  );
  check(
    "it says which agent and which conversation, not just that something is happening",
    panel.textContent.includes("Bitcoin Desk") && panel.textContent.includes("Asked by Day Check"),
    panel.textContent.slice(0, 110),
  );

  // A command left running is the one kind of work that outlives the turn that
  // started it, so a list that leaves it out is wrong exactly when it matters.
  const command = panel.querySelector('[data-command="job-1"]');
  check("a command left running is listed too", command, command ? "listed" : "missing");
  check(
    "and it is the only row that offers to stop something",
    panel.querySelectorAll(".stop-command").length === 1 &&
      command?.querySelector(".stop-command"),
    `${panel.querySelectorAll(".stop-command").length} stop button(s)`,
  );

  // Clicking a command row must not navigate: there is no turn to open, and a
  // conversation that has moved on is the wrong place to send anybody.
  const wasShowing = document.getElementById("thread-name").textContent;
  command?.click();
  await new Promise((r) => setTimeout(r, 150));
  check(
    "clicking one does not pretend there is somewhere to go",
    !panel.hidden && document.getElementById("thread-name").textContent === wasShowing,
    panel.hidden ? "the panel closed" : "stayed put",
  );

  command?.querySelector(".stop-command")?.click();
  await new Promise((r) => setTimeout(r, 200));
  check(
    "stopping one asks the app to stop that command and nothing else",
    asked.some((a) => a.name === "stop_a_command" && a.args?.handle === "job-1") &&
      !asked.some((a) => a.name === "stop"),
    asked
      .filter((a) => a.name.startsWith("stop"))
      .map((a) => `${a.name}(${JSON.stringify(a.args)})`)
      .join(" ") || "nothing was asked",
  );
  return found;
}

/**
 * The Tools panel, which answers "what can this reach".
 *
 * Two sources and both belong there: the servers the app gives every engine,
 * and what the engine itself turned up with. The second was true and invisible
 * for as long as this app has existed.
 */
export async function whatItCanReach() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const panel = document.getElementById("reachable");

  document.getElementById("reach").click();
  await new Promise((r) => setTimeout(r, 300));

  check("it opens", !panel.hidden, `hidden=${panel.hidden}`);
  check(
    "the servers the app gives it are listed",
    panel.textContent.includes("peekaboo") && panel.textContent.includes("mempalace"),
    panel.textContent.slice(0, 80),
  );
  check(
    "a server that did not start says why rather than being left out",
    panel.querySelector(".server.broken") &&
      panel.textContent.includes("No such file or directory"),
    panel.querySelector(".server.broken") ? "shown" : "missing",
  );
  check(
    "what the engine itself brought is listed too",
    ["Skills", "Kinds of helper", "Plugins", "Commands"].every((h) =>
      panel.textContent.includes(h),
    ),
    panel.textContent.slice(0, 120),
  );
  check(
    "the things themselves are named, not just counted",
    panel.textContent.includes("artifact-design") && panel.textContent.includes("Explore"),
    panel.textContent.includes("artifact-design") ? "named" : panel.textContent.slice(0, 90),
  );

  document.getElementById("reach").click();
  check("clicking again closes it", panel.hidden, `hidden=${panel.hidden}`);
  return found;
}

/**
 * Dictating an errand instead of typing it.
 *
 * Exercised against a stand-in, so no microphone is ever turned on. What is
 * being checked is what the page does with what it hears, not whether the
 * machine can hear.
 */
export async function dictation() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const speak = document.getElementById("speak");
  const box = document.getElementById("what");

  check("there is a way to dictate where the window can hear", !speak.hidden, `hidden=${speak.hidden}`);
  check("it does not look like it is listening before it is", speak.getAttribute("aria-pressed") === "false", speak.getAttribute("aria-pressed"));

  // Half an errand typed, to be finished out loud.
  box.value = "Tomorrow morning,";
  speak.click();
  check("clicking it starts listening", window.__HEARD__.includes("start"), window.__HEARD__.join(","));
  check(
    "it looks unmistakably like it is listening",
    speak.getAttribute("aria-pressed") === "true",
    speak.getAttribute("aria-pressed"),
  );

  await new Promise((r) => setTimeout(r, 300));
  check(
    "what it heard is added to what was already typed, not put over it",
    box.value === "Tomorrow morning, check the invoices",
    box.value,
  );
  check(
    "nothing was sent",
    !document.getElementById("messages").textContent.includes("check the invoices"),
    "not sent",
  );

  speak.click();
  check("clicking again stops it", window.__HEARD__.includes("stop"), window.__HEARD__.join(","));
  check("it stops looking like it is listening", speak.getAttribute("aria-pressed") === "false", speak.getAttribute("aria-pressed"));

  box.value = "";
  return found;
}

/**
 * A conversation woken by something changing.
 *
 * The check that matters is the arithmetic sentence: the failure this feature
 * can cause is somebody agreeing to a rate they never pictured, and the only
 * defence is putting the real numbers in front of them before they agree.
 */
export async function watching() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const panel = document.getElementById("watching");

  check("it starts closed", panel.hidden, `hidden=${panel.hidden}`);
  document.getElementById("watch").click();
  await new Promise((r) => setTimeout(r, 200));
  check("the Watch button opens it", !panel.hidden, `hidden=${panel.hidden}`);

  check(
    "it shows what is watched and what it will say",
    document.getElementById("watch-at").value.includes("~/Downloads") &&
      document.getElementById("watch-what").value.length > 0,
    document.getElementById("watch-at").value,
  );

  const says = document.getElementById("watch-says").textContent;
  check(
    "it says how often it will wake somebody, in numbers, before they agree to it",
    says.includes("every 10 minutes") && says.includes("24 times a day"),
    says.slice(0, 90),
  );
  check(
    "it admits it only looks while the app is open",
    says.includes("only looks while Errand is open"),
    says.slice(-70),
  );
  check("it says when it last looked", says.includes("Last looked at"), says.slice(-70));
  check(
    "nothing offers to look again while nothing has stopped",
    document.getElementById("watch-again").hidden,
    String(document.getElementById("watch-again").hidden),
  );

  // Looking happens on a timer behind the panel, so a panel that draws once
  // goes on saying "it has not looked yet" while the agent it describes is
  // being woken. Seen on screen: the sentence was wrong the moment it fired.
  FIXTURE.watches.woke_at = 1788000900000;
  FIXTURE.watches.woke_today = 1;
  await new Promise((r) => setTimeout(r, 5400));
  check(
    "it notices by itself that somebody has been woken",
    document.getElementById("watch-says").textContent.includes("Last woke this at"),
    document.getElementById("watch-says").textContent.slice(-70),
  );

  // A watch failing four times in silence before it admits anything is the
  // quiet failure this whole app is written against.
  FIXTURE.watches.misses = 2;
  await new Promise((r) => setTimeout(r, 5400));
  const failing = document.getElementById("watch-says").textContent;
  check(
    "a look that failed is said straight away, not after the fifth one",
    failing.includes("The last 2 looks failed") && failing.includes("stops after five"),
    failing.slice(0, 70),
  );
  FIXTURE.watches.misses = 0;

  document.getElementById("watch").click();
  check("clicking again closes it", panel.hidden, `hidden=${panel.hidden}`);
  return found;
}

/** Everything in the header on one row, which is what a header is. */
/** The width below which the header is meant to wrap, from app.css. */
const WRAPS_BELOW = 700;

/**
 * A sentence being written looks like a sentence being written.
 *
 * The flag that asks Claude Code for its prose a word at a time has been passed
 * since the beginning, and every one of those was dropped twice over: the
 * engine had no handler for them, and the window threw away anything unsettled.
 * What somebody saw was dots for several seconds and then a wall of text, which
 * is the exact thing this app says reads as a hang rather than as thinking.
 */
export async function writingItOut() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const where = document.getElementById("talks").value;
  const live = () => document.querySelector("#messages .writing");

  // A word at a time, the way the engine sends them.
  for (const word of ["The ", "answer ", "is "]) {
    tell("happened", { conversation: where, seq: 7001, kind: "said", text: word, settled: false });
  }
  await new Promise((r) => setTimeout(r, 150));
  check(
    "words arriving one at a time are shown as they arrive",
    live(),
    live() ? "shown" : "nothing on screen while it was writing",
  );
  check(
    "and they gather into the sentence rather than replacing each other",
    live()?.textContent === "The answer is ",
    JSON.stringify(live()?.textContent ?? null),
  );
  // Dots and a half-written sentence are two answers to the same question.
  check(
    "the working dots give way to the words",
    !document.querySelector("#messages .thinking"),
    document.querySelector("#messages .thinking") ? "both at once" : "just the words",
  );

  // Then the whole line lands, and the pieces must not be left beside it.
  tell("happened", {
    conversation: where,
    seq: 7002,
    kind: "said",
    text: "The answer is four.",
    settled: true,
  });
  await new Promise((r) => setTimeout(r, 150));
  const shown = document.getElementById("messages").textContent;
  check(
    "the finished line replaces the half-written one instead of doubling it",
    !live() && shown.split("The answer is").length === 2,
    `${shown.split("The answer is").length - 1} copies, live=${!!live()}`,
  );

  // A turn stopped mid-word leaves nothing hanging.
  tell("happened", { conversation: where, seq: 7003, kind: "said", text: "And also", settled: false });
  await new Promise((r) => setTimeout(r, 100));
  tell("happened", { conversation: where, seq: 7004, kind: "done" });
  await new Promise((r) => setTimeout(r, 150));
  check(
    "a turn that stops mid-word leaves no half sentence behind",
    !live() && !document.getElementById("messages").textContent.includes("And also"),
    live() ? "still writing" : "cleared",
  );

  // And the ending nothing sends. Pressing Stop kills the engine, and a killed
  // engine says nothing about having stopped, so the window has to finish the
  // turn itself -- all of it. It used to set "not working" and leave the half
  // sentence on screen for good, with a caret blinking under the transcript
  // through every redraw, and the next turn's first word joined onto it.
  // Sent the way somebody sends one, because Stop is only offered while there
  // is something to stop and only saying something makes that true.
  document.getElementById("what").value = "Write me something long";
  document.getElementById("composer").requestSubmit();
  await new Promise((r) => setTimeout(r, 300));
  tell("happened", { conversation: where, seq: 7005, kind: "said", text: "Half a thou", settled: false });
  await new Promise((r) => setTimeout(r, 150));
  check("a stopped turn starts from something half written", live(), live() ? "writing" : "nothing being written");
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  await new Promise((r) => setTimeout(r, 150));
  const stop = [...document.querySelectorAll("#palette-list li")].find((li) =>
    /stop what it is doing/i.test(li.textContent),
  );
  check("there is a way to stop it", stop, "not in the palette");
  stop?.click();
  await new Promise((r) => setTimeout(r, 350));
  check(
    "and stopping it takes the half sentence with it",
    !live() && !document.getElementById("messages").textContent.includes("Half a thou"),
    live() ? `still writing: ${live().textContent}` : "cleared",
  );
  return found;
}

/**
 * A whole call: it hears, it sends, it reads the answer out, it listens again.
 *
 * Dictation is still typing. What somebody wants while cooking or driving is
 * the loop, and every join in that loop is a place it can quietly stop being a
 * call: sending that never happens, an answer read out as "star star Done star
 * star", ears that never come back after the answer, or worst, ears that come
 * back while it is still speaking and send it its own voice.
 */
export async function aCall() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const call = document.getElementById("call");
  const speak = document.getElementById("speak");
  const box = document.getElementById("what");
  const where = document.getElementById("talks").value;

  check("there is a way to talk to it hands free", !call.hidden, `hidden=${call.hidden}`);
  check(
    "it does not look like it is listening before it is",
    call.getAttribute("aria-pressed") === "false",
    call.getAttribute("aria-pressed"),
  );

  window.__HEARD__.length = 0;
  window.__SAID__.length = 0;
  box.value = "";
  call.click();
  check("starting a call starts listening", window.__HEARD__.includes("start"), window.__HEARD__.join(","));
  check(
    "it looks unmistakably like it is listening",
    call.getAttribute("aria-pressed") === "true",
    call.getAttribute("aria-pressed"),
  );
  // Sending on a pause is the one thing in this window that acts without being
  // told to, and somebody who does not know that is about to send half a
  // thought.
  check(
    "the box says what is about to happen to what it hears",
    /sends/i.test(box.placeholder),
    box.placeholder,
  );

  // Heard, and then nothing sent yet. The pause is the whole mechanism -- it is
  // what somebody finishing a sentence looks like from here -- and without this
  // the check passed for a version that sent the moment it heard anything.
  await new Promise((r) => setTimeout(r, 500));
  check(
    "it does not send the moment it hears something",
    !document.getElementById("messages").textContent.includes("check the invoices"),
    document.getElementById("what").value || "nothing in the box",
  );

  // Then the pause.
  //
  // Comfortably longer than the pause rather than a shade longer. A browser
  // throttles timers in a tab nobody is looking at, and at one a second the
  // wait and the pause it is waiting for landed in whichever order they felt
  // like, which is a check that fails for a reason that has nothing to do with
  // the thing it is checking.
  await new Promise((r) => setTimeout(r, 4000));
  check(
    "stopping talking sends it, without anybody pressing anything",
    document.getElementById("messages").textContent.includes("check the invoices"),
    box.value ? `still in the box: ${box.value}` : "sent",
  );
  check(
    "the ears stop while the answer is being worked on",
    window.__HEARD__.includes("stop"),
    window.__HEARD__.join(","),
  );

  // The answer, in the notation an agent actually writes in.
  window.__SAID__.length = 0;
  tell("happened", {
    conversation: where,
    seq: 9100,
    kind: "said",
    text: "**Done.** Three of them are in `notes.txt`. See [the list](https://example.com/x?y=1).",
    settled: true,
  });
  await new Promise((r) => setTimeout(r, 120));
  const aloud = window.__SAID__.join(" ");
  check("the answer is read out", aloud.length > 0, JSON.stringify(aloud));
  check(
    "the marks that make it look right are not read out as words",
    !aloud.includes("**") && !aloud.includes("`") && !aloud.includes("https://"),
    JSON.stringify(aloud),
  );
  check(
    "and what it actually said survives that",
    aloud.includes("Done.") && aloud.includes("notes.txt") && aloud.includes("the list"),
    JSON.stringify(aloud),
  );

  // While it is talking it must not be listening, or it hears itself and sends
  // its own answer back as the next thing said.
  const heardWhileSpeaking = window.__HEARD__.slice(window.__HEARD__.lastIndexOf("stop"));
  check(
    "it is not listening while it is speaking",
    !heardWhileSpeaking.includes("start"),
    heardWhileSpeaking.join(","),
  );

  // Then the turn ends, and the call has to pick the conversation back up.
  tell("happened", { conversation: where, seq: 9101, kind: "done" });
  await new Promise((r) => setTimeout(r, 300));
  check(
    "when it has finished speaking it listens again",
    window.__HEARD__.lastIndexOf("start") > window.__HEARD__.lastIndexOf("stop"),
    window.__HEARD__.join(","),
  );

  // Silence is what a call is mostly made of. Recognition raises `no-speech`
  // as a matter of course after a stretch of quiet, and ending the call on it
  // meant that pausing to think about what to ask hung up on you.
  window.__HEARD__.length = 0;
  window.__EARS__?.onerror?.({ error: "no-speech" });
  await new Promise((r) => setTimeout(r, 200));
  check(
    "a pause to think does not end the call",
    call.getAttribute("aria-pressed") === "true",
    call.getAttribute("aria-pressed"),
  );
  check(
    "and it goes back to listening by itself",
    window.__HEARD__.includes("start"),
    window.__HEARD__.join(","),
  );

  // A question is the common case, because the default is to ask before
  // touching anything. It is not an ending the call listens for, and a card
  // cannot be answered out loud, so it has to say what it is waiting for and
  // then wait rather than going deaf without a word.
  window.__SAID__.length = 0;
  window.__HEARD__.length = 0;
  tell("happened", {
    conversation: where,
    seq: 9200,
    kind: "needs_you",
    asking: "empty the downloads folder",
    detail: "rm -rf ~/Downloads/*",
    tool: "Bash",
    call: "q1",
    step: "q1",
    can_remember: true,
    rule: "rm",
    allows: "any rm command",
  });
  await new Promise((r) => setTimeout(r, 400));
  const aboutIt = window.__SAID__.join(" ");
  check(
    "a question in a call is read out rather than met with silence",
    /permission/i.test(aboutIt) && aboutIt.includes("empty the downloads folder"),
    JSON.stringify(aboutIt),
  );
  check(
    "and it says where to answer it, since nobody in a call is looking",
    /Errand window/i.test(aboutIt),
    JSON.stringify(aboutIt),
  );
  check(
    "and it does not listen through it, which would send the answer as an errand",
    !window.__HEARD__.includes("start"),
    window.__HEARD__.join(",") || "not listening",
  );

  // Answered, and the call picks the conversation back up.
  const card = document.querySelector("#messages .asking .choices");
  const yes = [...(card?.querySelectorAll("button") || [])].find((b) => /^yes/i.test(b.textContent));
  check("the question can still be answered in the window", yes, card ? "no yes button" : "no card");
  // From here on, so that what happens after it does not decide the answer.
  // Asserting that "start" was the *last* thing heard made this depend on
  // whether the stand-in's next result and its pause timer landed inside the
  // wait, which they do on a slow run: it listened again and then sent, and
  // the check called that a failure to listen.
  const beforeAnswering = window.__HEARD__.length;
  yes?.click();
  await new Promise((r) => setTimeout(r, 300));
  tell("happened", { conversation: where, seq: 9201, kind: "done" });
  await new Promise((r) => setTimeout(r, 400));
  check(
    "answering it puts the call back to listening",
    window.__HEARD__.slice(beforeAnswering).includes("start"),
    window.__HEARD__.slice(beforeAnswering).join(",") || "nothing happened",
  );

  // The way out somebody reaches for without looking, in the one state where
  // their hands are not on the keyboard.
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await new Promise((r) => setTimeout(r, 60));
  check(
    "pressing escape ends the call",
    call.getAttribute("aria-pressed") === "false",
    call.getAttribute("aria-pressed"),
  );
  check(
    "and the microphone is off, not merely unpressed",
    window.__HEARD__[window.__HEARD__.length - 1] === "stop",
    window.__HEARD__.slice(-3).join(","),
  );
  // Dictation is untouched by any of this: one set of ears, two things that
  // want them, and only one of them sends.
  check(
    "the dictate button is left as it was",
    speak.getAttribute("aria-pressed") === "false",
    speak.getAttribute("aria-pressed"),
  );

  box.value = "";
  return found;
}

/**
 * Nothing in a step sits on top of anything else in it.
 *
 * Measured rather than looked at. A hook is called `SessionStart:startup`, which
 * is one unbreakable word, and with nothing said about wrapping it ran straight
 * through the outcome text beside it: two pieces of writing in the same place,
 * both unreadable, and it looked fine in every screenshot where the names
 * happened to be short.
 */
export async function stepsDoNotOverlap() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  // Nothing here can be judged in a window with no size: every measurement
  // comes back zero and every check below reports the app broken, with tops of
  // -113 and boxes "0px wide". Said rather than failed, the way the header
  // check has always said it.
  if (!beingDrawn()) {
    found.push({
      what: "steps do not overlap",
      ok: true,
      saw: "cannot be judged: this window has no size. Show the window and run again.",
    });
    return found;
  }
  const where = document.getElementById("talks").value;

  // The real shape: a long unbroken name, and an outcome long enough to fill
  // the rest of the row.
  tell("happened", {
    conversation: where,
    seq: 9300,
    kind: "doing",
    // `what`, which is the field the app actually sends. Sending `text` here
    // drew an empty name, and every measurement below then compared an empty
    // box against a full one and called it a pass.
    // Long enough to be squeezed. `SessionStart:startup` is twenty characters
    // and a conversation column is a thousand pixels wide, so nothing was ever
    // squeezed and every check below passed with the fix taken back out. A step
    // name is a sentence with a path in it often enough, and a path is one
    // unbreakable word.
    what: "A hook of yours ran: /Users/me/.claude/hooks/SessionStart:startup-check-everything-before-it-runs.sh",
    tool: "Bash",
    call: "hook-1",
    step: "hook-1",
  });
  tell("happened", {
    conversation: where,
    seq: 9301,
    kind: "did",
    call: "hook-1",
    outcome:
      "/last30days: Ready to use. Run /last30days to get started. Reddit, Hacker News " +
      "and Polymarket work out of the box. The setup wizard can unlock more, and " +
      "it will say which of them are signed in already and which are not.",
  });
  await new Promise((r) => setTimeout(r, 150));

  // Measured in a column the width of the one this happened in, rather than
  // whatever width the harness happens to be opened at. A thousand pixels of
  // conversation hides the fault completely: the name is never squeezed, and
  // every check below passes with the fix taken back out.
  const list = document.getElementById("messages");
  const wasWide = list.style.maxWidth;
  list.style.maxWidth = "380px";

  const row = [...document.querySelectorAll("#messages .doing")].pop();
  const name = row?.querySelector(".what");
  const outcome = row?.querySelector(".outcome");
  check(
    "a step and what it produced are both drawn, with words in them",
    name && outcome && name.textContent.includes("SessionStart") && outcome.textContent.length > 40,
    row ? `${JSON.stringify(name?.textContent)} / ${outcome?.textContent?.length} characters` : "no row",
  );

  if (name && outcome) {
    const a = name.getBoundingClientRect();
    const b = outcome.getBoundingClientRect();
    const overlapping = a.right > b.left + 1 && a.left < b.right - 1 && a.bottom > b.top + 1 && a.top < b.bottom - 1;
    check(
      "the step's name does not run through the text beside it",
      !overlapping,
      `name ${Math.round(a.left)}-${Math.round(a.right)} x ${Math.round(a.top)}-${Math.round(a.bottom)}, ` +
        `outcome ${Math.round(b.left)}-${Math.round(b.right)} x ${Math.round(b.top)}-${Math.round(b.bottom)}`,
    );
    // Overflowing its own box is what caused that, and it is invisible until
    // some name happens to be long enough.
    check(
      "and stays inside its own box",
      name.scrollWidth <= name.clientWidth + 1,
      `${name.scrollWidth} wide in ${name.clientWidth}`,
    );
    check(
      "so does the text beside it",
      outcome.scrollWidth <= outcome.clientWidth + 1,
      `${outcome.scrollWidth} wide in ${outcome.clientWidth}`,
    );
    // A name with room beside it is not broken mid-word. Breaking anywhere is
    // the last resort that stops the overlap, not the first thing to reach for.
    check(
      "a step's name gets room before it is broken up",
      a.width >= 150,
      `${Math.round(a.width)}px wide`,
    );
    // The row is one line of writing across, whatever wraps inside it.
    const parent = row.getBoundingClientRect();
    check(
      "the whole step stays inside the conversation",
      a.right <= parent.right + 1 && b.right <= parent.right + 1,
      `row ends ${Math.round(parent.right)}, name ${Math.round(a.right)}, outcome ${Math.round(b.right)}`,
    );
  }

  list.style.maxWidth = wasWide;
  return found;
}

/**
 * Coming back by itself after a restart.
 *
 * Everything this app does on its own it does while it is open, and the switch
 * that changes that is only worth having if it says what is true. The failure
 * to guard is a switch showing what it last remembered rather than what the
 * system will actually do, which is a thing somebody finds out about at a
 * login, days later, by a routine not running.
 */
export async function openingAtLogin() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  document.getElementById("setup").click();
  await new Promise((r) => setTimeout(r, 250));
  const box = document.getElementById("at-login");
  const says = document.getElementById("at-login-says");
  const card = box?.closest(".card");

  check("there is a way to have Errand open at login", box && !box.disabled, box ? `disabled=${box.disabled}` : "missing");
  check("it starts off, because nobody asked for it yet", box && !box.checked, `checked=${box?.checked}`);
  // The limitation this does not fix, said where the switch is rather than
  // discovered on the first morning the Mac was asleep.
  const explains = card ? card.textContent : "";
  check(
    "it says what this does not fix",
    /asleep|off/.test(explains) && /while it is open/.test(explains),
    explains.slice(0, 120),
  );

  box.checked = true;
  box.dispatchEvent(new Event("change"));
  await new Promise((r) => setTimeout(r, 200));
  check("turning it on asks the app to turn it on", asked.some((a) => a.name === "open_at_login" && a.args?.yes === true), JSON.stringify(asked.filter((a) => a.name === "open_at_login")));
  check("and it stays on, rather than snapping back", box.checked, `checked=${box.checked}`);
  // Nothing starts a second copy now, which is the part somebody would
  // otherwise discover by watching for a window that never appears.
  check("it says when this takes effect", /login/i.test(says.textContent), says.textContent);

  // Reopened from scratch: the switch has to be read back from the app, not
  // remembered by the page.
  //
  // The screen is hidden and unhidden rather than rebuilt, so the box keeps
  // whatever was last left on it and simply asserting it is still ticked is
  // true whether or not anything was ever asked. Put wrong first, on purpose:
  // only reading the answer back can put it right. Written the other way, the
  // window could stop asking the app at all and this would still pass, and the
  // switch would be showing what it last remembered rather than what the file
  // says.
  document.getElementById("models-done").click();
  document.getElementById("at-login").checked = false;
  document.getElementById("setup").click();
  await new Promise((r) => setTimeout(r, 250));
  check(
    "it reads what is actually set, rather than what the window remembers",
    document.getElementById("at-login").checked,
    `checked=${document.getElementById("at-login").checked}`,
  );

  box.checked = false;
  box.dispatchEvent(new Event("change"));
  await new Promise((r) => setTimeout(r, 200));
  check("turning it off asks the app to turn it off", asked.some((a) => a.name === "open_at_login" && a.args?.yes === false), JSON.stringify(asked.filter((a) => a.name === "open_at_login")));
  check("and says so", /not open at login/i.test(says.textContent), says.textContent);

  // The third answer, which no amount of pressing this switch can produce:
  // something starts at login and it is not this copy. Worth telling apart
  // from off, because turning it on is what fixes it and "it is already on"
  // is the one answer that would not.
  window.__AT_LOGIN__ = "something_else";
  document.getElementById("models-done").click();
  document.getElementById("setup").click();
  await new Promise((r) => setTimeout(r, 250));
  check(
    "another copy starting at login does not read as this one being on",
    !document.getElementById("at-login").checked,
    `checked=${document.getElementById("at-login").checked}`,
  );
  check(
    "and it says so, rather than leaving the switch to explain itself",
    /another copy/i.test(says.textContent),
    says.textContent,
  );
  window.__AT_LOGIN__ = undefined;
  document.getElementById("models-done").click();

  document.getElementById("models-done").click();
  return found;
}

/**
 * What this app is, for somebody who has just opened it.
 *
 * Two things have to be true and they pull against each other. It has to
 * appear on its own the first time, because an empty window that explains
 * itself beats an empty window. And it has to go away and stay away, because
 * the one thing worse than no explanation is one that will not leave.
 */
export async function sayingWhatThisIs() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const tour = document.getElementById("tour");

  // Not in the way of somebody who has used this before.
  check("it is not sitting there for somebody who already has agents", tour.hidden, `hidden=${tour.hidden}`);

  // Reachable when it is wanted, from the one place everything is reachable.
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  await new Promise((r) => setTimeout(r, 120));
  const entry = [...document.querySelectorAll("#palette-list li")].find((li) =>
    /what errand is/i.test(li.textContent),
  );
  check("there is a way back to it", entry, "not in the palette");
  entry?.click();
  await new Promise((r) => setTimeout(r, 150));
  check("opening it shows it", !tour.hidden, `hidden=${tour.hidden}`);

  const said = tour.textContent;
  // Named by what they do. "Repeat" says nothing to somebody who has never
  // used it; "the same errand every morning" is the thing they came wanting.
  check(
    "it says what the things in the window are for, not what they are called",
    /every morning/.test(said) && /wake it when something changes/.test(said),
    said.slice(0, 90),
  );
  // A number written into a sentence beside a list is a number that goes
  // wrong the next time the list changes.
  const rows = tour.querySelectorAll(".tour-one").length;
  check(
    "the number it claims is the number of things it says",
    said.includes(`${rows} things`),
    `${rows} rows, and it says: ${said.slice(0, 45)}`,
  );
  check(
    "it covers the whole app rather than the chat box",
    ["agent", "Allowed", "gear", "terminal", "Goal"].every((word) => said.includes(word)),
    ["agent", "Allowed", "gear", "terminal", "Goal"].filter((w) => !said.includes(w)).join(",") || "all there",
  );
  // It must be possible to be done with it.
  const done = tour.querySelector(".tour-done");
  check("there is a way to be finished with it", done, "no button");
  // And it is where somebody can see it. These panels are taller than they
  // look: eight things ran past the bottom of one that scrolls, and the only
  // way out was below a fold that does not look like a fold.
  if (done) {
    // Measured in a panel the height of one in a real window. The harness is
    // opened taller than the app usually is, and at that size the notes fit and
    // the fault is invisible.
    const wasTall = tour.style.maxHeight;
    tour.style.maxHeight = "260px";
    const box = tour.getBoundingClientRect();
    const button = done.getBoundingClientRect();
    check(
      "and it is in view rather than below the fold",
      button.bottom <= box.bottom + 1 && button.top >= box.top - 1,
      `panel ${Math.round(box.top)}-${Math.round(box.bottom)}, button ${Math.round(button.top)}-${Math.round(button.bottom)}`,
    );
    tour.style.maxHeight = wasTall;
  }
  done?.click();
  await new Promise((r) => setTimeout(r, 100));
  check("and it goes away", tour.hidden, `hidden=${tour.hidden}`);

  return found;
}

/**
 * The very first time this app is opened.
 *
 * Its own mode (`?empty`), because a first run is a different window: nothing
 * has been done in it, the window makes an agent for itself, and every check
 * written against the fixture would fail for the right reason and bury the one
 * thing worth looking at. Which is this: somebody who has just installed this
 * gets an empty box and no idea what any of it is for, unless something says.
 */
export function firstRun() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const tour = document.getElementById("tour");

  check(
    "a copy nobody has used explains itself without being asked",
    tour && !tour.hidden,
    tour ? `hidden=${tour.hidden}` : "no panel at all",
  );
  check(
    "and there is still somewhere to type",
    document.getElementById("what") && !document.getElementById("composer").hidden,
    "the box is there",
  );
  // Explaining the app is not the same as being the app: the window behind it
  // has to be the real one, ready to be used the moment it is put away.
  check(
    "it explains the window rather than replacing it",
    document.querySelectorAll("#threads li").length >= 1,
    `${document.querySelectorAll("#threads li").length} agent(s) made for it`,
  );
  // And it is not told what changed as well. A copy installed today has
  // nothing to have changed from, and being shown notes for the only version
  // it has ever run is an answer to a question nobody could have asked.
  check(
    "a first run is not also told what changed",
    document.getElementById("changed").hidden,
    `hidden=${document.getElementById("changed").hidden}`,
  );
  check(
    "and will not be told at the next launch either",
    window.__TOLD__ === true,
    `told=${window.__TOLD__}`,
  );

  const done = tour?.querySelector(".tour-done");
  done?.click();
  check("and it can be put away on the first click", tour && tour.hidden, `hidden=${tour?.hidden}`);
  return found;
}

/**
 * Making a routine out of the errand you actually refined.
 *
 * This is the end of the loop the whole app is for: you say what you want, it
 * tries, you correct it, and the version that finally worked is the one worth
 * having every morning. Until this, that version lived only in the conversation
 * and setting it to repeat meant reading it off the screen and typing it out
 * again, which is how a routine ends up being a slightly different job from the
 * one that was tested.
 */
export async function repeatingWhatWasAsked() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  await openTalk("talk-1");
  const box = document.getElementById("routine-what");
  const offered = document.getElementById("routine-said");

  document.getElementById("repeat").click();
  await new Promise((r) => setTimeout(r, 250));

  check("what was asked here is offered", offered && !offered.hidden, offered ? `hidden=${offered.hidden}` : "missing");
  const chips = [...(offered?.querySelectorAll(".from-here-one") || [])];
  check("there is at least one to take", chips.length > 0, `${chips.length} offered`);
  // Enough to find the one you mean, few enough to read at a glance.
  check("and not the whole conversation", chips.length <= 4, `${chips.length} offered`);

  if (chips.length) {
    // The chip is cut to fit; what it puts in the box must not be.
    const whole = chips[0].title;
    chips[0].click();
    await new Promise((r) => setTimeout(r, 80));
    check(
      "taking one fills in what was actually asked, whole",
      box.value === whole && whole.length > 0,
      `${JSON.stringify(box.value)} from ${JSON.stringify(whole)}`,
    );
    check(
      "and nothing is saved by taking it",
      !asked.some((a) => a.name === "runs" && a.args?.what === whole),
      "not saved",
    );
  }

  // The panel is one row of boxes and buttons, all the same height, with this
  // underneath rather than in among them.
  const at = document.getElementById("routine-at").getBoundingClientRect();
  const save = document.getElementById("routine-save").getBoundingClientRect();
  const row = offered.getBoundingClientRect();
  // Height rather than position: whether the row wraps depends on how wide the
  // window is, and it is meant to. That they are all one height is the rule
  // this must not have broken, and it holds at every width.
  check(
    "the boxes above it are still all one height",
    Math.abs(at.height - save.height) <= 1 && Math.round(at.height) === 34,
    `when ${Math.round(at.height)}, save ${Math.round(save.height)}`,
  );
  check(
    "and it sits under them rather than among them",
    row.top >= at.bottom - 1,
    `offered at ${Math.round(row.top)}, boxes end ${Math.round(at.bottom)}`,
  );

  // And the other half of the loop: trying the thing before a morning goes
  // past. The first run of a routine was always in front of nobody, which is
  // the one run anybody would most want to watch.
  const says = document.getElementById("routine-says");
  const tryIt = document.getElementById("routine-try");
  check("there is a way to try it before it ever runs", tryIt, "no button");

  box.value = "";
  tryIt?.click();
  await new Promise((r) => setTimeout(r, 120));
  check(
    "trying nothing says so rather than sending an empty errand",
    /nothing to try/i.test(says.textContent) && !asked.some((a) => a.name === "say" && !a.args?.text),
    says.textContent,
  );

  const before = asked.filter((a) => a.name === "runs").length;
  box.value = "Give me the overnight numbers";
  tryIt.click();
  await new Promise((r) => setTimeout(r, 350));
  check(
    "trying it says the thing, now",
    asked.some((a) => a.name === "say" && a.args?.text === "Give me the overnight numbers"),
    JSON.stringify(asked.filter((a) => a.name === "say").slice(-1)),
  );
  // The two things trying must not do. Saving it would keep a version nobody
  // agreed to, and moving the schedule would take away the run this was
  // rehearsing for.
  check(
    "and saves nothing by trying",
    asked.filter((a) => a.name === "runs").length === before,
    `${asked.filter((a) => a.name === "runs").length - before} saves`,
  );
  check(
    "and puts the panel away so the errand can be watched",
    document.getElementById("routine").hidden,
    `hidden=${document.getElementById("routine").hidden}`,
  );

  box.value = "";
  return found;
}

/**
 * The other half of what an agent may do without asking.
 *
 * The panel that answers that question was answering half of it. Errand's own
 * list is what somebody agreed to here and can take back here; the engine reads
 * rules of its own out of its settings files, and on the machine this was
 * written on there were nineteen of them, none of them visible anywhere in this
 * app. An allowlist you cannot read is not a boundary, which is a thing this app
 * says out loud about somebody else's arrangement.
 */
export async function whatElseIsAllowed() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  // Put back afterwards. A check that leaves somebody else's conversation open
  // is a check that breaks the next one, which is how a green run turns red in
  // a place that has nothing to do with the change.
  const wasOn = document.getElementById("talks").value;
  await openTalk("talk-2");
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 300));

  const also = document.getElementById("also-allowed");
  check("what the engine allows on its own is shown too", also && !also.hidden, also ? `hidden=${also.hidden}` : "missing");
  const said = also?.textContent || "";
  check(
    "each rule is there, as the engine writes it",
    said.includes("Bash(awk *)") && said.includes("Bash(chmod +x:*)"),
    said.slice(0, 120),
  );
  // Which file, because that is the only way to go and change one.
  check(
    "and says which file it lives in",
    said.includes("~/.claude/settings.json") && said.includes("~/.claude/settings.local.json"),
    said.slice(0, 160),
  );
  // A refusal explains something that otherwise looks like a fault.
  check("what is refused outright is shown as refused", /Refused .*Read\(\/\/etc/.test(said), said.slice(0, 200));
  check(
    "it says plainly that Errand cannot take these back",
    /cannot take them back/i.test(said),
    said.slice(0, 90),
  );
  // No button, because a button here would be this app writing rules into the
  // one place it cannot show them.
  check(
    "and offers no button that would edit somebody else's settings file",
    !also.querySelector("button"),
    also.querySelector("button") ? also.querySelector("button").textContent : "no buttons",
  );
  // The half that is not a list at all, and the reason a command can run with
  // nothing on either list covering it.
  check(
    "it says the engine also judges some things harmless by itself",
    /judges harmless/i.test(said),
    said.slice(-90),
  );
  // Nineteen rules is an ordinary number to have, and this panel is a third of
  // a short window. What has to survive that is the prose: the list can be
  // scrolled to, the two sentences explaining it cannot be found by somebody
  // who does not know they are there.
  window.__ALSO_MODE__ = {
    allow: Array.from({ length: 20 }, (_, i) => ({
      rule: `Bash(thing${i} *)`,
      whose: "~/.claude/settings.json",
    })),
    deny: [],
    mode: null,
    mode_says: null,
  };
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 200));
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 300));
  {
    const box = document.getElementById("granting").getBoundingClientRect();
    const lines = [...document.querySelectorAll("#also-allowed .also-what")];
    // Measured, so only meaningful in a window that has a size.
    if (!beingDrawn()) {
      found.push({
        what: "a long list does not push the sentences explaining it out of the panel",
        ok: true,
        saw: "cannot be judged: this window has no size. Show the window and run again.",
      });
      return found;
    }
    check(
      "a long list does not push the sentences explaining it out of the panel",
      lines.length === 2 && lines.every((p) => p.getBoundingClientRect().bottom <= box.bottom + 1),
      lines
        .map((p) => `${Math.round(p.getBoundingClientRect().bottom)} of ${Math.round(box.bottom)}`)
        .join(", "),
    );
  }

  // A mode set for every session, said the way the app says it. The window
  // does not write this sentence: it does not know what Errand puts on the
  // command line, and when it did write it, it was the one line on the screen
  // that was not true.
  window.__ALSO_MODE__ = {
    allow: [{ rule: "Bash(awk *)", whose: "~/.claude/settings.json" }],
    deny: [],
    mode: { rule: "bypassPermissions", whose: "~/.claude/settings.json", managed: false },
    mode_says:
      "~/.claude/settings.json sets the engine to ask nothing at all: every tool runs. " +
      "That does not apply here: Errand starts this agent as default, on the command " +
      "line, which overrules it.",
  };
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 200));
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 300));
  const withMode = document.getElementById("also-allowed").textContent;
  check(
    "a mode Errand overrules is said as overruled, not as fact",
    /does not apply here/.test(withMode),
    withMode.slice(0, 140),
  );
  check(
    "and is not dressed as an alarm",
    !document.querySelector("#also-allowed .also-mode"),
    document.querySelector("#also-allowed .also-mode")?.textContent || "quiet",
  );

  // The one file Errand cannot overrule is the one that is loud.
  window.__ALSO_MODE__ = {
    allow: [],
    deny: [],
    mode: {
      rule: "bypassPermissions",
      whose: "/Library/Application Support/ClaudeCode/managed-settings.json",
      managed: true,
    },
    mode_says:
      "Whoever administers this Mac has set the engine to ask nothing at all: every tool " +
      "runs, in /Library/Application Support/ClaudeCode/managed-settings.json. That " +
      "outranks anything Errand asks for.",
  };
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 200));
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 300));
  check(
    "a mode Errand cannot overrule is said loudly",
    document.querySelector("#also-allowed .also-mode")?.textContent.includes("outranks"),
    document.querySelector("#also-allowed .also-mode")?.textContent || "nothing said",
  );
  window.__ALSO_MODE__ = undefined;
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 200));
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 300));

  // Errand's own list is still there and still revocable: this is beside it,
  // not instead of it.
  const ours = [...document.querySelectorAll("#allowed li button")];
  check("Errand's own list still offers to take its rules back", ours.length > 0, `${ours.length} buttons`);

  document.getElementById("granted").click();
  await openTalk(wasOn);
  return found;
}

/**
 * What changed in this one.
 *
 * Every copy of Errand is installed by hand over the top of the last, so the
 * only moment anybody knows the version changed is the moment they notice
 * something different and wonder whether they imagined it. The rule that
 * matters is the second one: once, and then only when asked, because a panel
 * that returns at every launch is a panel people learn to close unread.
 */
export async function whatChangedInThisOne() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const panel = document.getElementById("changed");

  // Shown by itself, without being asked for: this window was opened on a
  // version nobody had been told about, which is what happens every time a copy
  // is installed over the top of the last.
  check(
    "a version nobody has been told about says what changed, unasked",
    !panel.hidden,
    `hidden=${panel.hidden}`,
  );
  const said = panel.textContent;
  check("it says which version they are for", /0\.1\.0/.test(said), said.slice(0, 60));
  check(
    "and what a person would notice, rather than what a commit did",
    /Answers arrive as they are written/.test(said),
    said.slice(0, 120),
  );
  check(
    "each note is its own line",
    panel.querySelectorAll(".changed-list li").length === 10,
    `${panel.querySelectorAll(".changed-list li").length} lines`,
  );
  // Told, so it is not told again.
  check("looking at them counts as having been told", window.__TOLD__ === true, `told=${window.__TOLD__}`);

  const done = panel.querySelector(".tour-done");
  check("there is a way to be finished with them", done, "no button");
  if (done) {
    // The same, and for the same reason: this is the panel the fault was
    // actually seen in, in a window smaller than the harness runs at.
    const wasTall = panel.style.maxHeight;
    panel.style.maxHeight = "260px";
    const box = panel.getBoundingClientRect();
    const button = done.getBoundingClientRect();
    check(
      "and it is in view rather than below the fold",
      button.bottom <= box.bottom + 1 && button.top >= box.top - 1,
      `panel ${Math.round(box.top)}-${Math.round(box.bottom)}, button ${Math.round(button.top)}-${Math.round(button.bottom)}`,
    );
    panel.style.maxHeight = wasTall;
  }
  done?.click();
  await new Promise((r) => setTimeout(r, 80));
  check("and they go away", panel.hidden, `hidden=${panel.hidden}`);

  // And they stay away. A panel that comes back at every launch is one people
  // learn to close without reading, which is the whole reason for remembering
  // at all. Asked the same question the window asks on opening: the answer it
  // would get next time is what decides whether it shows itself.
  const again = await window.__TAURI__.core.invoke("what_changed");
  check(
    "and the next launch is told it has already said this",
    again && again.first_time === false,
    JSON.stringify(again?.first_time),
  );

  // But they can still be asked for.
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  await new Promise((r) => setTimeout(r, 150));
  const entry = [...document.querySelectorAll("#palette-list li")].find((li) =>
    /what changed/i.test(li.textContent),
  );
  check("there is still a way to ask for them", entry, "not in the palette");
  entry?.click();
  await new Promise((r) => setTimeout(r, 250));
  check(
    "and asking shows them again",
    !document.getElementById("changed").hidden,
    `hidden=${document.getElementById("changed").hidden}`,
  );
  document.getElementById("changed").querySelector(".tour-done")?.click();
  return found;
}

/**
 * What can be done to an agent without opening it.
 *
 * The list answered a right-click with nothing at all, which was not a missing
 * convenience: there was no way to delete an agent from the window whatsoever,
 * so a thread somebody made by mistake stayed in their list for good. Somebody
 * said so, having tried.
 */
export async function theMenuOnAnAgent() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const menu = document.getElementById("menu");
  // Whatever the window has open, read the way the window itself would.
  const showingAgentId = () =>
    document.querySelector('#threads li[aria-current="true"]')?.dataset.agent || "";
  const rowFor = (id) =>
    [...document.querySelectorAll("#threads li")].find((li) => li.dataset.agent === id);

  // One made for the purpose, which is also the case somebody complained
  // about: a thread made by mistake that could not be got rid of. Deleting a
  // fixture agent instead would take three later checks with it, since they
  // open its conversations.
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  await new Promise((r) => setTimeout(r, 150));
  [...document.querySelectorAll("#palette-list li")]
    .find((li) => /^New agent/.test(li.textContent))
    ?.click();
  await new Promise((r) => setTimeout(r, 400));

  const before = document.querySelectorAll("#threads li").length;
  const row = [...document.querySelectorAll("#threads li")].find((li) =>
    /New errand/.test(li.textContent),
  );
  check("the new agent is in the list to begin with", row, `${before} rows`);
  const doomed = { id: showingAgentId() };

  row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 120, clientY: 200 }));
  await new Promise((r) => setTimeout(r, 120));
  check("right-clicking an agent offers what can be done to it", !menu.hidden, `hidden=${menu.hidden}`);

  const labels = [...menu.querySelectorAll("button")].map((b) => b.textContent);
  check(
    "including the one somebody went looking for",
    labels.some((l) => /^Delete/.test(l)),
    labels.join(" | "),
  );
  check(
    "and the things only doable from outside an agent",
    labels.some((l) => /Pin/.test(l)) && labels.some((l) => /Hide/.test(l)) && labels.some((l) => /Who this is/.test(l)),
    labels.join(" | "),
  );
  // Asking about an agent is not asking to go and look at it: switching under
  // somebody loses whatever they were reading. Asked of a different agent than
  // the open one, since the one just made is open by definition.
  const openBefore = document.querySelector('#threads li[aria-current="true"]')?.dataset.agent;
  const another = [...document.querySelectorAll("#threads li")].find(
    (li) => li.dataset.agent && li.dataset.agent !== openBefore,
  );
  another?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 110, clientY: 180 }));
  await new Promise((r) => setTimeout(r, 120));
  check(
    "right-clicking does not open the agent",
    document.querySelector('#threads li[aria-current="true"]')?.dataset.agent === openBefore,
    `${openBefore} still open`,
  );
  // Back to the one being deleted.
  document.body.click();
  await new Promise((r) => setTimeout(r, 80));
  row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 120, clientY: 200 }));
  await new Promise((r) => setTimeout(r, 120));

  // Delete asks before it does it.
  const del = [...menu.querySelectorAll("button")].find((b) => /^Delete/.test(b.textContent));
  del.click();
  await new Promise((r) => setTimeout(r, 100));
  check(
    "the first press asks rather than deletes",
    !asked.some((a) => a.name === "forget") && /Delete .*\?/.test(del.textContent),
    del.textContent,
  );
  check("and says what goes with it", /said goes too/.test(del.textContent), del.textContent);

  // The second press is the answer.
  del.click();
  await new Promise((r) => setTimeout(r, 300));
  check(
    "the second press deletes it",
    asked.some((a) => a.name === "forget"),
    JSON.stringify(asked.filter((a) => a.name === "forget")),
  );
  check("the menu goes away with it", menu.hidden, `hidden=${menu.hidden}`);
  check(
    "and it leaves the list",
    document.querySelectorAll("#threads li").length === before - 1,
    `${document.querySelectorAll("#threads li").length} rows, was ${before}`,
  );
  check(
    "the window still has an agent open",
    document.getElementById("thread-name").textContent.trim().length > 0,
    document.getElementById("thread-name").textContent,
  );

  // And it closes the way everything else here closes.
  rowFor(FIXTURE.agents[0].id)?.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 100, clientY: 150 }),
  );
  await new Promise((r) => setTimeout(r, 100));
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await new Promise((r) => setTimeout(r, 100));
  check("escape closes it", menu.hidden, `hidden=${menu.hidden}`);

  rowFor(FIXTURE.agents[0].id)?.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 100, clientY: 150 }),
  );
  await new Promise((r) => setTimeout(r, 100));
  document.body.click();
  await new Promise((r) => setTimeout(r, 100));
  check("clicking anywhere else closes it", menu.hidden, `hidden=${menu.hidden}`);

  return found;
}

/**
 * Whether somebody could set a watch without being taught the app first.
 *
 * The old panel asked for `~/Downloads every 10m` in one box and "then say" in
 * another, and nothing on the screen said what either meant. Somebody who had
 * been shown it once still asked, reasonably: what is it watching, where, every
 * 10 what, and what does "then say" mean, can it speak? A form that has to be
 * explained in a message is a form that has not explained itself.
 */
export async function settingAWatchExplainsItself() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  await openTalk("talk-1");
  document.getElementById("watch").click();
  await new Promise((r) => setTimeout(r, 350));

  // From nothing being watched, whatever an earlier check left behind. The
  // panel describes a state, so a check about the empty state has to make it.
  const wasWatching = document.getElementById("watch-stop");
  if (!wasWatching.hidden) {
    wasWatching.click();
    await new Promise((r) => setTimeout(r, 300));
  }

  const at = document.getElementById("watch-at");
  const often = document.getElementById("watch-often");
  const what = document.getElementById("watch-what");
  const plain = document.getElementById("watch-plain");
  const stop = document.getElementById("watch-stop");

  // How often is a list of answers rather than a syntax to learn.
  check("how often is chosen rather than typed", often && often.tagName === "SELECT", often ? often.tagName : "missing");
  check(
    "and the choices are in words",
    [...(often?.options || [])].every((o) => /minute|hour|day/.test(o.textContent)),
    [...(often?.options || [])].map((o) => o.textContent).join(", "),
  );
  // Nothing is being watched yet, so there is nothing to stop.
  check("it does not offer to stop something that never started", stop.hidden, `hidden=${stop.hidden}`);
  at.value = "";
  at.dispatchEvent(new Event("input"));
  what.value = "";
  what.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 100));
  check(
    "an empty panel says what to do rather than nothing",
    /Name a folder/.test(plain.textContent),
    plain.textContent,
  );

  // Typed the way somebody would, and the sentence follows along.
  at.value = "~/Downloads";
  at.dispatchEvent(new Event("input"));
  often.value = "1h";
  often.dispatchEvent(new Event("change"));
  what.value = "tell me what is new and whether it matters";
  what.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 120));

  const said = plain.textContent;
  check("it says what it will look at", said.includes("~/Downloads"), said);
  check("how often, in the words that were chosen", /every hour/.test(said), said);
  check("what it will ask the agent to do", /tell me what is new/.test(said), said);
  // The limitation that decides whether it works at all, said where it is set
  // rather than found out on the first morning.
  check("and that it only looks while Errand is open", /while Errand is open/.test(said), said);

  // A web address reads as reading a page, not as looking in a folder.
  at.value = "https://example.com/prices";
  at.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 100));
  check(
    "a web address is described as a page rather than a folder",
    /read https:\/\/example\.com\/prices/.test(plain.textContent) && /page has changed/.test(plain.textContent),
    plain.textContent,
  );

  // Saving sends the two controls as the one line the app stores.
  at.value = "~/Downloads";
  at.dispatchEvent(new Event("input"));
  document.getElementById("watch-save").click();
  await new Promise((r) => setTimeout(r, 300));
  const sent = asked.filter((a) => a.name === "watch_it").pop();
  check(
    "saving puts the two controls back together the way the app stores them",
    sent?.args?.watches === "~/Downloads every 1h",
    JSON.stringify(sent?.args),
  );

  // Stopped again, so the next check meets the panel in the state this one
  // met it in.
  const stopAfter = document.getElementById("watch-stop");
  if (!stopAfter.hidden) {
    stopAfter.click();
    await new Promise((r) => setTimeout(r, 300));
  }
  document.getElementById("watch").click();
  return found;
}

/**
 * Being asked to come and do one thing, and handing it back.
 *
 * The end of the road that was not really the end of one. An agent meeting a
 * sign-in could only stop: it wrote a sentence about what somebody would have
 * to go and do, the errand ended, and whatever it had arranged half way through
 * stayed half arranged. What was missing was never the ability to sign in, and
 * must not be. It is the ability to stop, be helped, and carry on.
 */
export async function handingItOver() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const where = document.getElementById("talks").value;

  const heard = tell("handing_over", {
    conversation: where,
    seq: 9600,
    handover: "h-1",
    what: "Sign in to your Apple Account",
    why: "The order page will not show a guest order without the email on it.",
    where: "https://secure.store.apple.com/shop/order/list",
  });
  await new Promise((r) => setTimeout(r, 200));
  check("the window is listening for somebody being wanted", heard > 0, `${heard} listener(s)`);

  const card = document.querySelector("#messages .handover");
  check("it draws a card of its own rather than a line of text", card, "no card");
  const said = card?.textContent || "";
  check("it says what to do", said.includes("Sign in to your Apple Account"), said.slice(0, 60));
  check("and why it cannot be done for them", /will not show a guest order/.test(said), said.slice(0, 120));

  // The page itself, as a link rather than as a thing this app types into.
  const link = card?.querySelector("a.detail");
  check("the page is offered as a link they open", link && link.textContent.startsWith("https://"), link?.textContent);

  const buttons = [...(card?.querySelectorAll(".choices button") || [])].map((b) => b.textContent);
  check(
    "there is a way to say it is done and a way to refuse",
    buttons.length === 2 && /done/i.test(buttons[0]) && /skip/i.test(buttons[1]),
    buttons.join(" | "),
  );

  card.querySelector(".choices button").click();
  await new Promise((r) => setTimeout(r, 250));
  check(
    "saying it is done tells the agent that is waiting",
    asked.some((a) => a.name === "handed_back" && a.args?.handover === "h-1" && a.args?.how === "done"),
    JSON.stringify(asked.filter((a) => a.name === "handed_back")),
  );
  check(
    "and the card stops offering, since it has been answered",
    !document.querySelector("#messages .handover .choices"),
    document.querySelector("#messages .handover")?.textContent.slice(-40),
  );

  // Read back off disk while the agent is still sitting there. Without asking
  // the app, opening the conversation turned a question somebody was being
  // asked into a note about one, with the agent still waiting and nothing on
  // screen to answer it with.
  //
  // Two conversations rather than one opened twice: a conversation is read
  // back exactly once and kept, so opening the same one again proves nothing.
  await openTalk("talk-waiting");
  await new Promise((r) => setTimeout(r, 300));
  const again = document.querySelector("#messages .handover");
  check(
    "one still being waited on keeps its buttons when it is read back",
    again?.querySelector(".choices"),
    again ? again.textContent.slice(0, 60) : "no card",
  );

  // And one nobody is waiting on any more still offers a way out.
  //
  // This used to offer nothing, on the reasoning that a button answering a call
  // that has gone would tell nobody. True, and the wrong conclusion: granting
  // an app Full Disk Access means quitting that app, so the permission somebody
  // is most likely to be sent for is the one that takes the waiting agent down
  // with it -- and they come back to a card that cannot be answered and an
  // errand that has to be started again from nothing. The buttons say it into
  // the conversation instead.
  await openTalk("talk-over");
  await new Promise((r) => setTimeout(r, 300));
  const stale = document.querySelector("#messages .handover");
  check(
    "one nobody is waiting on says so plainly",
    stale && /stopped waiting while you were away/.test(stale.textContent),
    stale?.textContent.slice(-70) || "no card",
  );
  check(
    "and still offers a way to carry on",
    stale?.querySelector(".choices button"),
    [...(stale?.querySelectorAll(".choices button") || [])].map((b) => b.textContent).join(" | "),
  );
  check(
    "with a label that admits it is picking something back up",
    /carry on/i.test(stale?.querySelector(".choices button")?.textContent || ""),
    stale?.querySelector(".choices button")?.textContent,
  );

  await openTalk("talk-1");
  return found;
}

/**
 * What agents can be let at.
 *
 * Everywhere else a connector begins with an account and an OAuth screen. The
 * things people actually ask about on a Mac -- their mail, their diary -- are
 * already here, and the only permission needed is the one macOS asks for
 * itself. What that costs is honesty about the boundary: these run in the app
 * rather than in the walled engine, so confining an agent to its own folder
 * does not decide whether it can read somebody's mail. The switch does, which
 * makes the sentence beside the switch part of the feature.
 */
export async function whatAgentsCanReach() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  document.getElementById("setup").click();
  await new Promise((r) => setTimeout(r, 300));
  const list = document.getElementById("reachable-list");
  const rows = [...list.querySelectorAll("li")];
  check("the things it can be let at are listed", rows.length >= 2, `${rows.length} listed`);

  const boxes = [...list.querySelectorAll("input[type=checkbox]")];
  check("nothing is on until somebody turns it on", boxes.every((b) => !b.checked), boxes.map((b) => b.checked).join(","));
  // The switch is the whole of the permission, so it has to say what it lets in.
  const said = list.textContent;
  check(
    "each says what an agent would be able to see",
    /Reads your mail/.test(said) && /Reads what is in your calendars/.test(said),
    said.slice(0, 100),
  );
  check(
    "and what it will never do",
    /never sends anything/.test(said) && /never adds, moves or cancels/.test(said),
    said.slice(0, 140),
  );

  boxes[0].checked = true;
  boxes[0].dispatchEvent(new Event("change"));
  await new Promise((r) => setTimeout(r, 250));
  check(
    "turning one on tells the app which",
    asked.some((a) => a.name === "connect" && a.args?.id === "mail" && a.args?.on === true),
    JSON.stringify(asked.filter((a) => a.name === "connect")),
  );

  // Read back from the app rather than remembered by the page: a switch showing
  // what it last did rather than what is true is one somebody finds out about
  // when an agent cannot read the thing they connected.
  document.getElementById("models-done").click();
  document.getElementById("setup").click();
  await new Promise((r) => setTimeout(r, 300));
  const again = [...document.querySelectorAll("#reachable-list input[type=checkbox]")];
  check("and it is still on when the screen is opened again", again[0]?.checked, `checked=${again[0]?.checked}`);

  again[0].checked = false;
  again[0].dispatchEvent(new Event("change"));
  await new Promise((r) => setTimeout(r, 250));
  check(
    "turning it off tells the app that too",
    asked.some((a) => a.name === "connect" && a.args?.id === "mail" && a.args?.on === false),
    JSON.stringify(asked.filter((a) => a.name === "connect").slice(-1)),
  );

  document.getElementById("models-done").click();
  return found;
}

export function headerFitsOnOneRow() {
  // A media query reads the viewport, not the element, so this cannot be
  // judged by widening anything on the page: run narrow, it measures a header
  // that is wrapping exactly as it was told to, and reports the app broken.
  // Rather than pass on a test it did not run, it says it could not run.
  if (window.innerWidth <= WRAPS_BELOW) {
    return {
      rows: 0,
      at: window.innerWidth,
      tooNarrowToJudge: true,
      toolsInside: document.getElementById("reach").getBoundingClientRect().width > 0,
    };
  }
  const title = document.getElementById("title");
  // Only what is on screen. A hidden child measures zero and would otherwise
  // count as a row of its own, which is a test failing at its own reflection.
  const showing = [...title.children].filter((c) => c.getBoundingClientRect().width > 0);
  const tops = new Set(showing.map((c) => Math.round(c.getBoundingClientRect().top / 10)));
  const tools = document.getElementById("reach").getBoundingClientRect();
  const bar = title.getBoundingClientRect();
  return {
    rows: tops.size,
    // Said out loud, because "one row" is only a claim about a width.
    at: Math.round(bar.width),
    toolsInside: tools.width > 0 && tools.right <= bar.right - 17,
  };
}

/**
 * A thread that says when things happened.
 *
 * The four ways an errand starts -- you type it, the clock, a watch, a goal --
 * all write into a conversation nobody is looking at, and until now the window
 * had no way to say which lines were from this morning and which from a week
 * ago. Yesterday's briefing sat directly above today's with nothing between
 * them, which is the exact shape this app is for.
 */
export async function whenThingsHappened() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  await openTalk("talk-overnight");
  const days = [...document.querySelectorAll("#messages .day")].map((d) =>
    d.textContent.trim(),
  );
  // Three days of lines, so two seams: nothing above the first thing said, and
  // one wherever the day turns over. A separator above the first line would be
  // a label on a conversation rather than a break in one.
  check(
    "the day is said where it changes, and only there",
    days.length === 2,
    JSON.stringify(days),
  );
  check("today is called today rather than dated", days.includes("Today"), JSON.stringify(days));
  check(
    "and the day before it is called yesterday",
    days.includes("Yesterday"),
    JSON.stringify(days),
  );

  const first = document.querySelector("#messages > li");
  check(
    "nothing is announced above the first thing said",
    first && !first.classList.contains("day"),
    first?.className,
  );

  // The other half of the same rule: a separator on every message is noise, so
  // a conversation that happened in one sitting says nothing at all.
  await openTalk("talk-1");
  check(
    "a conversation that happened in one sitting says no dates",
    document.querySelectorAll("#messages .day").length === 0,
    String(document.querySelectorAll("#messages .day").length),
  );
  return found;
}

/**
 * An agent that says it has something you have not read.
 *
 * The reason to leave errands running is that they run while you are somewhere
 * else, and until now the list of agents could not say so. A row that had
 * produced a briefing at seven read exactly like one that had not run in a
 * month, because the line under the name is what the agent is for -- which is
 * the right line to have there, and not an answer to "did anything happen".
 */
export async function somethingYouHaveNotRead() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const rowFor = (agent) =>
    [...document.querySelectorAll("#threads li")].find((li) => li.dataset.agent === agent);

  // Put it back to unread first. The starting state cannot be relied on here,
  // because reading a conversation is what every group before this one does on
  // its way past, and reading is the thing being tested.
  window.__TAURI__.nowUnread("agent-bitcoin", 2);
  // Opening somebody else's conversation is what makes the window ask again,
  // without touching the agent under test.
  await openTalk("talk-waiting");

  const bitcoin = rowFor("agent-bitcoin");
  check(
    "the agent with something new is marked on the row",
    bitcoin?.classList.contains("has-new"),
    bitcoin?.className,
  );
  check(
    "and says how much and how long ago, rather than what it is for",
    /2 new · .+ago/.test(bitcoin?.querySelector(".last")?.textContent || ""),
    bitcoin?.querySelector(".last")?.textContent,
  );

  const quiet = rowFor("agent-unnamed");
  check(
    "an agent with nothing new still says what it is for",
    quiet && !quiet.classList.contains("has-new"),
    quiet?.querySelector(".last")?.textContent,
  );

  // And reading it clears it, which is the half a fixture answering the same
  // thing twice could never show.
  await openTalk("talk-2");
  const after = rowFor("agent-bitcoin");
  check(
    "opening it clears the mark",
    after && !after.classList.contains("has-new"),
    after?.className,
  );
  check(
    "and the line goes back to what the agent is for",
    !/new ·/.test(after?.querySelector(".last")?.textContent || ""),
    after?.querySelector(".last")?.textContent,
  );
  return found;
}

/**
 * What you typed stays with the thread you typed it in.
 *
 * This app encourages switching mid-thought: the sidebar row, the conversation
 * picker and "New conversation with this agent" are all one click. Every one of
 * them used to carry an unsent errand into a different agent's composer, where
 * Enter sends it. The only way the window could lose work rather than merely
 * fail to show something.
 */
export async function whatYouTypedStaysPut() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const box = document.getElementById("what");

  await openTalk("talk-1");
  box.value = "Sort the Downloads folder";
  box.dispatchEvent(new Event("input"));

  // Another conversation of the same agent, which is the one click away.
  await openTalk("talk-2");
  check("switching conversation does not carry it across", box.value === "", box.value);

  // And another agent entirely, which is where sending it would be worst.
  await openTalk("talk-waiting");
  check("nor does switching agent", box.value === "", box.value);

  await openTalk("talk-1");
  check(
    "and it is still there when you come back to where you wrote it",
    box.value === "Sort the Downloads folder",
    box.value,
  );

  // Sent is not half typed: without clearing it, the draft comes back next
  // time under the message it already became.
  document.getElementById("composer").dispatchEvent(new Event("submit"));
  await new Promise((r) => setTimeout(r, 300));
  await openTalk("talk-2");
  await openTalk("talk-1");
  check("sending it does not leave a copy behind", box.value === "", box.value);
  return found;
}

/**
 * Conversations you can name, and delete on their own.
 *
 * Errand shipped the better model, several conversations to an agent in a
 * picker, and then gave the picker nothing to tell its entries apart. The three
 * names the app invents are "First", "New conversation" and "{name}, again",
 * none of them chosen by a person, so after a week the list repeats one word
 * and the only way to find the right one is to open each of them.
 */
export async function namingAndDeletingAConversation() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const menu = document.getElementById("menu");
  const picker = document.getElementById("talks");

  await openTalk("talk-2");
  picker.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 300, clientY: 300 }));
  const labels = [...menu.querySelectorAll("button")].map((b) => b.textContent);
  check("right-clicking the picker offers what can be done to it", !menu.hidden, String(menu.hidden));
  check("renaming is one of them", labels.some((l) => /Rename/.test(l)), JSON.stringify(labels));
  check("and deleting just this conversation is another", labels.some((l) => /^Delete this/.test(l)), JSON.stringify(labels));

  // Naming it puts the name in the picker, so the entries can be told apart.
  const was = window.prompt;
  window.prompt = () => "Rent receipts";
  await menu.querySelector("button").click();
  await new Promise((r) => setTimeout(r, 300));
  window.prompt = was;
  check(
    "the name it was given is the one in the picker",
    [...picker.options].some((o) => o.textContent.includes("Rent receipts")),
    [...picker.options].map((o) => o.textContent).join(" | "),
  );
  check(
    "and the app was told, rather than only the window",
    asked.some((a) => a.name === "call_it" && a.args?.name === "Rent receipts"),
    JSON.stringify(asked.filter((a) => a.name === "call_it").slice(-1)),
  );

  // Deleting asks once, on the button, the same way deleting an agent does.
  picker.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 300, clientY: 300 }));
  const remove = [...menu.querySelectorAll("button")].find((b) => /^Delete this/.test(b.textContent));
  remove.click();
  await new Promise((r) => setTimeout(r, 120));
  check(
    "deleting asks first, in the button rather than over it",
    /Delete .*\?/.test(remove.textContent) && !asked.some((a) => a.name === "forget_conversation"),
    remove.textContent,
  );

  remove.click();
  await new Promise((r) => setTimeout(r, 350));
  check(
    "and the second press does it",
    asked.some((a) => a.name === "forget_conversation"),
    JSON.stringify(asked.filter((a) => a.name === "forget_conversation")),
  );
  check("the menu closes behind it", menu.hidden, String(menu.hidden));
  return found;
}

/**
 * A notification that finds its way back.
 *
 * The whole payoff of handing over something worth walking away from. Before
 * this a click only brought the app forward: you read the notification, and
 * then went and found the conversation yourself, which is the errand you were
 * trying not to run.
 */
export async function aNotificationThatLeadsSomewhere() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  // Start somewhere else, so arriving is a real move.
  await openTalk("talk-1");
  check("something else is open to begin with", document.getElementById("talks").value === "talk-1", document.getElementById("talks").value);

  // The app says which conversation somebody is reading, so a notification can
  // be held back for that one and shown for the rest.
  check(
    "the window says which conversation is on screen",
    asked.some((a) => a.name === "looking_at" && a.args?.id === "talk-1"),
    JSON.stringify(asked.filter((a) => a.name === "looking_at").slice(-1)),
  );

  // A click on one about a conversation of another agent entirely.
  await tell("go_to", "talk-waiting");
  await new Promise((r) => setTimeout(r, 500));
  check(
    "clicking a notification opens the conversation it was about",
    document.getElementById("talks").value === "talk-waiting",
    document.getElementById("talks").value,
  );

  // And one about something that no longer exists leaves the window alone
  // rather than jumping somewhere arbitrary.
  await tell("go_to", "talk-that-was-deleted");
  await new Promise((r) => setTimeout(r, 400));
  check(
    "one about a conversation that is gone leaves the window where it was",
    document.getElementById("talks").value === "talk-waiting",
    document.getElementById("talks").value,
  );
  return found;
}

/**
 * A routine you can switch off, and a list of what it actually did.
 *
 * The only stop there was cleared the schedule, what it says and when it last
 * ran, in one statement: going away for a week and coming back meant setting
 * the whole thing up again from memory. And the question people ask about a
 * standing job is not when it is next but whether it has been working, which
 * nothing in the app could answer -- three failed mornings left a conversation
 * looking merely quiet.
 */
export async function pausingARoutineAndSeeingHowItWent() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  await openTalk("talk-2");
  document.getElementById("repeat").click();
  await new Promise((r) => setTimeout(r, 350));
  const says = document.getElementById("routine-says");
  const pause = document.getElementById("routine-pause");

  check("a conversation with a routine offers to pause it", !pause.hidden, String(pause.hidden));
  check("and the button says pause while it is running", pause.textContent === "Pause", pause.textContent);
  check("the line says when it is next", /Next /.test(says.textContent), says.textContent);

  // What it actually did, told apart three ways.
  const went = [...document.querySelectorAll("#routine-went-list li")];
  check("what it did is listed", went.length === 3, String(went.length));
  check(
    "a failed run reads as failed rather than as quiet",
    went.some((li) => li.classList.contains("wrong") && /not answering/.test(li.textContent)),
    went.map((li) => li.className).join(" | "),
  );
  check(
    "and a run that never came back is neither done nor failed",
    went.some((li) => li.classList.contains("unfinished") && /did not finish/.test(li.textContent)),
    went.map((li) => li.textContent.slice(-30)).join(" | "),
  );

  // Pausing keeps the schedule, which is the whole point.
  pause.click();
  await new Promise((r) => setTimeout(r, 250));
  check(
    "pausing tells the app to switch it off rather than clear it",
    asked.some((a) => a.name === "routine_off" && a.args?.off === true) &&
      !asked.some((a) => a.name === "runs" && a.args?.at === null),
    JSON.stringify(asked.filter((a) => a.name === "routine_off" || a.name === "runs").slice(-2)),
  );
  check(
    "the line says it is paused, and does not promise a next run",
    /Paused/.test(says.textContent) && !/Next /.test(says.textContent),
    says.textContent,
  );
  check("and what it would run is still there", /daily 07:00/.test(says.textContent), says.textContent);
  check("the button now offers to start it again", pause.textContent === "Start again", pause.textContent);

  // And reopening reads it back from the app rather than from the page.
  document.getElementById("repeat").click();
  document.getElementById("repeat").click();
  await new Promise((r) => setTimeout(r, 350));
  check(
    "it is still paused when the panel is opened again",
    /Paused/.test(document.getElementById("routine-says").textContent),
    document.getElementById("routine-says").textContent,
  );

  document.getElementById("routine-pause").click();
  await new Promise((r) => setTimeout(r, 250));
  check(
    "and starting it again says so",
    asked.some((a) => a.name === "routine_off" && a.args?.off === false),
    JSON.stringify(asked.filter((a) => a.name === "routine_off").slice(-1)),
  );
  document.getElementById("repeat").click();
  return found;
}

/**
 * A search that lands on the line, and a Cmd-F that does something.
 *
 * The expensive half of a search was already being done and thrown away: the
 * query finds the exact line, inside a subquery, and returned the agent. So
 * somebody searching for a phrase they remember was dropped into whichever of
 * that agent's conversations spoke most recently, with no highlight, and
 * scrolled for it by hand -- and it gets worse the more the app is used as
 * intended. Cmd-F was separately ignored altogether.
 */
export async function findingWhereTheWordsAre() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const box = document.getElementById("find");

  // Start somewhere the words are not, so arriving is a real move. The phrase
  // is in an older conversation of the same agent, which is the case the whole
  // thing exists for: the newest one is where you used to be dropped.
  await openTalk("talk-1");
  box.value = "Yesterday: BTC up";
  box.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 400));

  const rows = [...document.querySelectorAll("#threads li")];
  const row = rows.find((li) => li.dataset.agent === "agent-bitcoin");
  check("the agent that said it is in the narrowed list", row, rows.map((r) => r.dataset.agent).join(","));
  check(
    "and the row shows the line rather than what the agent is for",
    /Yesterday: BTC up/.test(row?.querySelector(".last")?.textContent || ""),
    row?.querySelector(".last")?.textContent,
  );

  row?.click();
  await new Promise((r) => setTimeout(r, 600));
  check(
    "clicking it opens the conversation the words are in",
    document.getElementById("talks").value === "talk-overnight",
    document.getElementById("talks").value,
  );
  const marked = document.querySelector("#messages li.found");
  check("and the line itself is marked", marked, String(!!marked));
  check(
    "the marked line is the one that matched, not the one beside it",
    /Yesterday: BTC up/.test(marked?.textContent || ""),
    marked?.textContent?.slice(0, 60),
  );

  box.value = "";
  box.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 300));

  // And finding words in what is already open, which is a different question.
  const bar = document.getElementById("finding");
  check("the find bar starts closed", bar.hidden, String(bar.hidden));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", metaKey: true, bubbles: true }));
  await new Promise((r) => setTimeout(r, 200));
  check("Cmd-F opens it", !bar.hidden, String(bar.hidden));

  const what = document.getElementById("finding-what");
  what.value = "BTC";
  what.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 200));
  check(
    "it says how many there are rather than leaving them to be counted",
    /\d+ of \d+/.test(document.getElementById("finding-count").textContent),
    document.getElementById("finding-count").textContent,
  );
  const firstOne = document.getElementById("finding-count").textContent;
  what.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise((r) => setTimeout(r, 150));
  check(
    "Enter steps to the next one",
    document.getElementById("finding-count").textContent !== firstOne,
    `${firstOne} then ${document.getElementById("finding-count").textContent}`,
  );

  what.value = "nothing like this is in here";
  what.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 150));
  check(
    "and words that are not there say so rather than nothing",
    document.getElementById("finding-count").textContent === "none",
    document.getElementById("finding-count").textContent,
  );

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", metaKey: true, bubbles: true }));
  await new Promise((r) => setTimeout(r, 150));
  check("Cmd-F again puts it away", bar.hidden, String(bar.hidden));
  check(
    "and nothing is left marked behind it",
    !document.querySelector("#messages li.found"),
    String(!!document.querySelector("#messages li.found")),
  );
  return found;
}

/**
 * What a long command is actually printing.
 *
 * Until now only the model could see this: it reaches the kept output through
 * check_command and nothing else did, which is the wrong way round for the one
 * person who can decide to stop it. A build that has been going for ten minutes
 * and a build that is stuck look identical from outside.
 */
export async function whatALongCommandIsPrinting() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  const panel = document.getElementById("working");
  // Closed first. The palette entry toggles, so a group before this one that
  // left it open would have this check close it and report the panel missing.
  panel.hidden = true;
  // Then opened the way somebody would: through the palette, which is where
  // everything the header cannot hold now lives.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  const typing = document.getElementById("palette-what");
  typing.value = "what is running";
  typing.dispatchEvent(new Event("input"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise((r) => setTimeout(r, 450));
  check("the panel opens", !panel.hidden, String(panel.hidden));

  const rows = [...panel.querySelectorAll(".one")];
  const command = rows.find((r) => r.dataset.command);
  check("the command left running is in it", command, rows.length + " rows");
  const printing = command?.querySelector(".tail");
  check("and it shows what that command is printing", printing, String(!!printing));
  check(
    "which is the end of the output rather than a summary of it",
    /Compiling errand-app/.test(printing?.textContent || ""),
    printing?.textContent,
  );

  // A turn is not a command and has nothing printing, so it shows nothing.
  const turn = rows.find((r) => !r.dataset.command);
  check(
    "a turn that is not a command shows no output",
    turn && !turn.querySelector(".tail"),
    String(!!turn?.querySelector(".tail")),
  );

  panel.hidden = true;
  return found;
}

/**
 * The one line in a menu somebody has to read all of.
 *
 * Deleting asks first, in the button. The menu was placed once, before the
 * label changed, so pressing Delete grew it downwards and pushed the sentence
 * saying what was about to be destroyed off the bottom of the window. And the
 * name it puts in that sentence is, for an agent that has not named itself,
 * the whole of the first thing anybody said to it.
 */
export async function theQuestionBeforeDeletingIsReadable() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const menu = document.getElementById("menu");

  // An agent whose name is a whole sentence, which is what an unnamed one is
  // called: after the first thing anybody said to it.
  const rows = [...document.querySelectorAll("#threads li")];
  const row = rows[rows.length - 1] || rows[0];
  // Opened low down, which is where the clipping happened: near the bottom
  // there is no room below for the label to grow into.
  row.dispatchEvent(
    new MouseEvent("contextmenu", {
      bubbles: true,
      clientX: 40,
      clientY: window.innerHeight - 40,
    }),
  );
  await new Promise((r) => setTimeout(r, 150));

  const remove = [...menu.querySelectorAll("button")].find((b) => /^Delete/.test(b.textContent));
  check("deleting is offered", remove, [...menu.querySelectorAll("button")].map((b) => b.textContent).join(" | "));
  if (!remove) return found;

  remove.click();
  await new Promise((r) => setTimeout(r, 200));
  check("it asks first", /\?/.test(remove.textContent), remove.textContent);

  const box = menu.getBoundingClientRect();
  whenDrawn(found, "and the whole menu is still on screen once it has asked", () => ({
    ok: box.bottom <= window.innerHeight && box.top >= 0,
    saw: `top ${Math.round(box.top)}, bottom ${Math.round(box.bottom)}, window ${window.innerHeight}`,
  }));
  const asking = remove.getBoundingClientRect();
  check(
    "the question itself is not cut off",
    asking.bottom <= box.bottom + 1 && asking.height > 0,
    `question ends ${Math.round(asking.bottom)}, menu ends ${Math.round(box.bottom)}`,
  );
  // A name that is a whole request is shortened rather than pasted in whole,
  // and the full stop on the end of it does not become "hello.?".
  check(
    "a long name is cut down rather than pasted in whole",
    remove.textContent.length < 70,
    `${remove.textContent.length} characters: ${remove.textContent}`,
  );
  check("and it does not read as a full stop followed by a question mark", !/\.\?/.test(remove.textContent), remove.textContent);

  closeTheMenuFromOutside();
  return found;
}

function closeTheMenuFromOutside() {
  document.body.click();
  const menu = document.getElementById("menu");
  if (menu) menu.hidden = true;
}

/**
 * Pictures you can actually see.
 *
 * A picture reached the engine and was thrown away, and the line said "(with a
 * picture)". So you could send a screenshot and never see the one you sent, and
 * a conversation that had been about a picture read afterwards as a
 * conversation about nothing.
 */
export async function picturesYouCanSee() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  await openTalk("talk-1");
  await new Promise((r) => setTimeout(r, 400));

  const withThem = [...document.querySelectorAll("#messages li.mine")].find((li) =>
    li.textContent.includes("What is wrong with this screen?"),
  );
  check("the message with pictures on it is drawn", withThem, String(!!withThem));

  const shown = withThem?.querySelectorAll(".pictures img") || [];
  check("the picture itself is shown, not a note about one", shown.length === 1, `${shown.length} images`);
  check(
    "and it is a real picture rather than an empty box",
    shown[0]?.src?.startsWith("data:image/"),
    String(shown[0]?.src || "").slice(0, 24),
  );
  // The old behaviour, which must not come back: the count glued onto the words.
  check(
    "the words are the words, with no count glued on the end",
    !withThem?.textContent.includes("with a picture"),
    withThem?.querySelector(".mine-words")?.textContent,
  );
  check(
    "a picture whose file has gone says so rather than leaving a gap",
    withThem?.querySelector(".picture-gone"),
    withThem?.querySelector(".picture-gone")?.textContent,
  );

  // Bigger, in the window. Not handed to the system: show_in_browser takes
  // http and https and is right to refuse a data URL.
  shown[0]?.click();
  await new Promise((r) => setTimeout(r, 150));
  const closer = document.querySelector(".closer");
  check("clicking one shows it larger", closer, String(!!closer));
  check(
    "and it did not try to hand a data URL to the system",
    !asked.some((a) => a.name === "show_in_browser" && String(a.args?.url).startsWith("data:")),
    JSON.stringify(asked.filter((a) => a.name === "show_in_browser").slice(-1)),
  );

  // Escape as well as a click, because an overlay with no visible way out is
  // the one kind people get stuck in.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await new Promise((r) => setTimeout(r, 150));
  check("Escape puts it away", !document.querySelector(".closer"), String(!!document.querySelector(".closer")));
  return found;
}

/**
 * A turn the app was closed during.
 *
 * A turn cannot outlive the process running it. Quitting Errand while one was
 * going killed the engine and left the transcript holding a question with no
 * answer and nothing at all saying why, which from the window is
 * indistinguishable from an app still thinking about it -- and stays that way
 * for ever. The ordinary "Ask again" is no help, because it hangs off the last
 * answer and never getting one is the whole of what happened.
 */
export async function aTurnTheAppWasClosedDuring() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  await openTalk("talk-cut-off");
  const ended = document.querySelector("#messages li.ended");
  check("it says what happened rather than showing nothing", ended, String(!!ended));
  check(
    "and says the words that were already written are still there",
    /closed while this was running/.test(ended?.textContent || ""),
    ended?.textContent?.slice(0, 80),
  );

  const again = ended?.querySelector("button.again");
  check("it offers to run it again", again, String(!!again));
  check("with a label that says what pressing it does", again?.textContent === "Run it again", again?.textContent);

  // And pressing it sends the question that was cut off, not the ending.
  const before = asked.filter((a) => a.name === "say").length;
  again?.click();
  await new Promise((r) => setTimeout(r, 400));
  const sent = asked.filter((a) => a.name === "say").slice(-1)[0];
  check(
    "pressing it asks the question again, not the apology",
    asked.filter((a) => a.name === "say").length > before &&
      sent?.args?.text === "Show me the most important news of today",
    JSON.stringify(sent?.args?.text),
  );

  // An ordinary failure is not offered a rerun: nothing about it says it would
  // go differently the second time.
  await openTalk("talk-4");
  const ordinary = [...document.querySelectorAll("#messages li.ended")].find(
    (li) => !/closed while this was running/.test(li.textContent),
  );
  check(
    "an ending that was not an interruption does not offer one",
    !ordinary || !ordinary.querySelector("button.again"),
    ordinary?.textContent?.slice(0, 50),
  );
  return found;
}

/**
 * A picture an agent made, shown rather than described.
 *
 * `![it](/some/path)` fell through to the link rule, which takes http and https
 * and hands everything else back as its own source text. So an answer said
 * "here is the picture" and showed a path, and agents learnt to apologise for
 * it in prose: "both are downloaded locally if the images don't render for
 * you". Which is the app failing and the model covering for it.
 */
export async function aPictureAnAgentMade() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const { render } = await import(`../markdown.js${new URL(import.meta.url).search}`);

  const drawn = render("Here he is:\n\n![John Ternus](/tmp/ternus.jpg)\n\nThat is him.");
  const img = drawn.querySelector("img.drawn");
  check("a picture in an answer becomes a picture", img, drawn.textContent.slice(0, 60));
  check("with the words around it kept", /Here he is/.test(drawn.textContent), drawn.textContent.slice(0, 40));
  check(
    "and the path is not left sitting in the text",
    !drawn.textContent.includes("/tmp/ternus.jpg"),
    drawn.textContent.slice(0, 80),
  );
  check("its alt text is what the agent called it", img?.alt === "John Ternus", img?.alt);

  // One on the web needs nothing from the app and is used directly.
  const web = render("![a chart](https://example.com/c.png)").querySelector("img.drawn");
  check("one on the web is used as it is", web?.src === "https://example.com/c.png", web?.src);

  // A file an answer points at is worth reaching, and revealing one in Finder
  // cannot run anything.
  const path = render("I saved it to [report.pdf](/Users/me/Desktop/report.pdf).");
  const on = path.querySelector("a.on-disk");
  check("a file an answer points at can be reached", on, path.textContent.slice(0, 60));
  check("and it says it will show it rather than open it", /Finder/.test(on?.title || ""), on?.title);

  // What must not change: a scheme that is not a scheme stays text.
  const nasty = render("[press me](javascript:alert(1))");
  check(
    "and a link that is not a link is still shown as its own text",
    !nasty.querySelector("a") && nasty.textContent.includes("javascript:"),
    nasty.textContent,
  );
  return found;
}
