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
  // Four from the app and three the window answers for itself, and the count
  // has to be of all of them: a check that reported only half the setup would
  // be a check somebody trusted for the wrong half.
  check(
    "it counts everything it checked, including what only the window can answer",
    /3 of 7 things want attention/.test(panel.textContent),
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

  // Notifications off used to be a line on stderr. Here it is a finding that
  // says where to turn them on, with the pane opened for them: "Notifications,
  // then Errand" is four levels down a screen most people have never opened.
  const notifying = [...panel.querySelectorAll(".finding")].find((f) =>
    f.querySelector(".finding-what")?.textContent === "Notifications",
  );
  check(
    "notifications being off says where to turn them on",
    ["System Settings", "Notifications", "Errand"].every((w) =>
      notifying?.querySelector(".finding-fix")?.textContent.includes(w),
    ),
    notifying?.querySelector(".finding-fix")?.textContent,
  );
  const opens = notifying?.querySelector("button.finding-open");
  check("and offers to open that pane", !!opens, opens?.textContent || "no button");
  const before = asked.length;
  opens?.click();
  await new Promise((r) => setTimeout(r, 100));
  const opened = asked.slice(before).find((a) => a.name === "open_settings");
  check(
    "pressing it opens System Settings at Errand's own entry, through the app",
    opened?.args?.pane?.startsWith("x-apple.systempreferences:") && opened.args.pane.endsWith("?id=com.errandai.errand"),
    JSON.stringify(opened?.args),
  );
  check(
    "a finding with nowhere to open has no button",
    panel.querySelectorAll("button.finding-open").length === 1,
    `${panel.querySelectorAll("button.finding-open").length} buttons`,
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
    document.getElementById("threads").textContent.includes("Running now"),
    document.getElementById("threads").textContent.includes("Running now") ? "Running now" : "not running",
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

  const stillSaysWorking = document.getElementById("threads").textContent.includes("Running now");
  const stillThinking = !!document.getElementById("messages").querySelector(".thinking");
  check(
    "it stops saying it is working",
    !stillSaysWorking && !stillThinking,
    [stillSaysWorking && "the list still says Running now", stillThinking && "the dots are still there"]
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
  // The task that has one.
  await openTalk(FIXTURE.goalIn);

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
  const models = () => [...document.getElementById("engine").options].filter((o) => !o.value.startsWith("__"));
  check(
    "it shows what was chosen, and that is all",
    models().length === FIXTURE.offered.length,
    `${models().length} of ${FIXTURE.offered.length}`,
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
  // Errand's model is the first in the list, and then it is taken out.
  const menu = document.getElementById("engine");
  menu.value = kept[0].id;
  menu.dispatchEvent(new Event("change"));
  await new Promise((r) => setTimeout(r, 200));
  const mine = [...document.querySelectorAll("#chosen li button")].find(
    (b) => b.textContent === "Remove",
  );
  FIXTURE.offered = kept.slice(1);
  mine?.click();
  await new Promise((r) => setTimeout(r, 300));

  const options = [...document.getElementById("engine").options].map((o) => o.textContent);
  check(
    "Errand's model, once taken out of the list, is still shown as the one chosen",
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
  // And answering it reaches the question. Read back from the store it knows
  // only its step, and the answer went with no id at all, so pressing Yes on a
  // question opened from its notification failed.
  const before = asked.length;
  const yes = [...document.querySelectorAll("#messages .asking .choices button")].find((b) => b.textContent === "Yes");
  yes?.click();
  await new Promise((r) => setTimeout(r, 300));
  const answered = asked.slice(before).find((a) => a.name === "answer");
  check(
    "and pressing Yes on it answers with the step it is about",
    answered?.args?.call === "c2" && answered?.args?.step === "c2",
    JSON.stringify(answered?.args || "nothing was answered"),
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
  // A hosted model is paid for by the token, and what it used is shown as
  // tokens: the panel said only Claude was paid for while nearly every agent
  // ran on one.
  check(
    "what a hosted model used is there, in tokens and by model",
    said.includes("1.3M tokens on hosted models") && said.includes("deepseek-v4-flash at api.deepseek.com · 1.2M in, 45k out · 31 errands"),
    said.slice(said.indexOf("tokens") - 60, said.indexOf("tokens") + 160),
  );
  check(
    "and it says why tokens rather than dollars",
    said.includes("Errand does not guess"),
    said.slice(-160),
  );
  check("and no longer claims only Claude is paid for", !/Only Claude is paid for/.test(said), said.slice(0, 80));

  // Nothing paid for is not the same as nothing loaded: somebody running only
  // local models should be told why this is empty rather than left to wonder.
  const was = FIXTURE.what_it_cost;
  FIXTURE.what_it_cost = { today: [], this_month: [], used_today: [], used_this_month: [], nothing_yet: true };
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
    document.getElementById("costing").textContent.includes("costs nothing") &&
      document.getElementById("costing").textContent.includes("paid for by the token"),
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
    panel.textContent.includes("3 running, 1 needs you"),
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
    "and says what to do about it, under what went wrong",
    panel.querySelector(".server.broken .server-fix")?.textContent.includes("~/.claude.json"),
    panel.querySelector(".server.broken .server-fix")?.textContent || "no advice",
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
  // Not "while Errand is open": the window can be closed now, and a sentence
  // that said the watch needed the window would send people to keep one open.
  check(
    "it admits it only looks while Errand is running, window or no window",
    says.includes("only looks while Errand is running, window or no window"),
    says.slice(-110),
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

/**
 * The width from which the header has room for one row. Below it the window
 * wraps it when, and only when, it does not fit, and what is judged there is
 * that nothing is past the edge.
 */
const ONE_ROW_FROM = 1100;

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

  // An answer sent back before it was shown, for claiming work nothing did:
  // its words give way to the step that says so, and are not left on screen
  // to be read as said.
  tell("happened", {
    conversation: where,
    seq: 7101,
    kind: "said",
    text: "Done. It is in your Downloads, verified.",
    settled: false,
  });
  await new Promise((r) => setTimeout(r, 100));
  tell("happened", {
    conversation: where,
    seq: 7102,
    kind: "doing",
    what: "It said it had done this without running anything, so it was sent back to do it",
    tool: "errand",
    call: "sent-back-1",
  });
  await new Promise((r) => setTimeout(r, 150));
  const after = document.getElementById("messages").textContent;
  check(
    "an answer sent back before it was shown leaves none of its words behind",
    !live() && !after.includes("It is in your Downloads, verified."),
    live() ? "still writing" : "cleared",
  );
  check("and the step saying it was sent back is there", after.includes("sent back to do it"), after.slice(-120));
  tell("happened", { conversation: where, seq: 7103, kind: "done" });
  await new Promise((r) => setTimeout(r, 100));

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
  // By what the button does, not by what it says. It reads "Yes" the first
  // time and "Just this once" once there is an Always beside it worth telling
  // it apart from, and this check is about a question being answerable at all.
  const yes = card?.querySelector("button.yes");
  check(
    "the question can still be answered in the window",
    yes,
    card ? [...card.querySelectorAll("button")].map((b) => b.textContent).join(" | ") : "no card",
  );
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
 * Everything this app does on its own it does while the process is running,
 * window or no window, and the switch that brings it back after a restart is
 * only worth having if the card beside it says what is true. The failure to
 * guard is a switch showing what it last remembered rather than what the
 * system will actually do, which is a thing somebody finds out about at a
 * login, days later, by a routine not running. The card had its own version of
 * that: it went on saying the jobs stopped "while it is open" was over, after
 * closing the window had stopped meaning that.
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
  // What closing the window does and what it does not, said where the switch
  // is rather than discovered on the first morning the Mac was asleep.
  const explains = card ? card.textContent : "";
  check(
    "it says closing the window stops nothing, and where Errand stays",
    /Closing the window does not stop anything/.test(explains) && /Dock/.test(explains),
    explains.slice(0, 140),
  );
  check(
    "it says what does stop it: quitting, logging out, a Mac asleep or off",
    /Quit/.test(explains) && /[Ll]ogging out/.test(explains) && /asleep/.test(explains) && /off/.test(explains),
    explains.slice(140, 520),
  );
  check(
    "it no longer says the jobs only run while the window is open",
    !/while it is open/.test(explains),
    explains.slice(0, 140),
  );
  // What a restart brings back and what it does not. "All of it" included
  // started commands, which quitting kills and nothing restarts.
  check(
    "it says a restart brings routines and watches back, and a command has to be started again",
    /routines and watches with it/.test(explains) && /started again/.test(explains) && !/brings all of it back/.test(explains),
    explains.slice(400, 800),
  );
  // A run only calls itself late past ten minutes, so "says it is late" was
  // a promise the commonest case never kept.
  check(
    "it says when a late run says so: past ten minutes",
    /more than ten minutes late/.test(explains) && !/says it is late/.test(explains),
    explains.slice(300, 600),
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

  // The panel where a routine is set is the one of the four starts that said
  // nothing about what keeps it running; the person setting one is the one
  // about to rely on it.
  const keeps = document.getElementById("routine-keeps");
  check(
    "it says what keeps a routine running and what stops it, where the routine is set",
    keeps &&
      keeps.offsetParent !== null &&
      /window or no window/.test(keeps.textContent) &&
      /[Qq]uitting/.test(keeps.textContent) &&
      /asleep/.test(keeps.textContent),
    keeps ? keeps.textContent.trim() : "missing",
  );

  // Somebody who only wants this task again should not have to type it, or
  // know that the chips below are the way to get it.
  check(
    "with nothing repeating here yet, what to do is filled in with this conversation's task",
    box.value === "Show me the latest Bitcoin news",
    JSON.stringify(box.value),
  );
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
  check(
    "and nothing that would finish a teammate: what finishes is a task",
    !labels.some((l) => /finished/i.test(l)),
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
  check(
    "and that it only looks while Errand is running, window or no window",
    /while Errand is running, window or no window/.test(said),
    said,
  );

  // A web address reads as reading a page, not as looking in a folder.
  at.value = "https://example.com/prices";
  at.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 100));
  check(
    "a web address is described as a page rather than a folder",
    /read https:\/\/example\.com\/prices/.test(plain.textContent) && /page has changed/.test(plain.textContent),
    plain.textContent,
  );

  // Mail and the calendar are named in words, because nobody knows the
  // address of their own inbox, and each is described as what it is.
  at.value = "mail";
  at.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 100));
  check(
    "mail is described as counting unread mail, and only while Mail is open",
    /count your unread mail every hour/.test(plain.textContent) &&
      /only while Mail is open/.test(plain.textContent) &&
      /If there is more of it, it will ask this agent to tell me what is new/.test(plain.textContent),
    plain.textContent,
  );
  at.value = "calendar 30m before";
  at.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 100));
  check(
    "a calendar watch is described as waking before each event",
    /look at your calendars every hour/.test(plain.textContent) &&
      /At least 30 minutes before each event, it will ask this agent/.test(plain.textContent),
    plain.textContent,
  );
  at.value = "my calendar";
  at.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 100));
  check(
    "and a quarter of an hour when nobody said how long",
    /At least 15 minutes before each event/.test(plain.textContent),
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
  const title = document.getElementById("title");
  // Only what is on screen. A hidden child measures zero and would otherwise
  // count as a row of its own, which is a test failing at its own reflection.
  const showing = [...title.children].filter((c) => c.getBoundingClientRect().width > 0);
  // By each one's middle, which is the same for everything on a row because
  // the row centres them. By its top, a shorter one on the same row (a room's
  // names, fifteen pixels high) counted as a row of its own.
  const tops = new Set(
    showing.map((c) => {
      const box = c.getBoundingClientRect();
      return Math.round((box.top + box.height / 2) / 20);
    }),
  );
  const tools = document.getElementById("reach").getBoundingClientRect();
  const bar = title.getBoundingClientRect();
  return {
    rows: tops.size,
    // Said out loud, because "one row" is only a claim about a width.
    at: Math.round(bar.width),
    // Narrower than this it may wrap, and that is not a failure: a second
    // row is readable, and a button past the edge is not there at all.
    tooNarrowToJudge: window.innerWidth < ONE_ROW_FROM,
    toolsInside: tools.width > 0 && tools.right <= bar.right - 17,
    // Judged at every width.
    pastTheEdge: showing.filter((c) => c.getBoundingClientRect().right > bar.right + 1).map((c) => c.id || c.className),
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

  // Naming it happens on its card, where its name is: Rename in this menu
  // opens the name there for typing. It was a dialog, which a window on a Mac
  // does not always show.
  await menu.querySelector("button").click();
  await new Promise((r) => setTimeout(r, 150));
  const box = document.querySelector("#task-card .name-edit");
  check(
    "Rename opens the task's name for typing, on its card",
    box && document.activeElement === box,
    box ? `focused=${document.activeElement === box}` : "no box",
  );
  if (box) {
    box.value = "Rent receipts";
    box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  }
  await new Promise((r) => setTimeout(r, 300));
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
  const sideNames = () => [...document.querySelectorAll("#threads .task-name")].map((n) => n.textContent);
  check(
    "and the list down the side and the card say it too",
    sideNames().includes("Rent receipts") && document.querySelector("#task-card .name")?.textContent === "Rent receipts",
    `${sideNames().join(" | ")}; card: ${document.querySelector("#task-card .name")?.textContent}`,
  );
  // The name on the card is itself the way to rename it, and Escape changes
  // nothing.
  const calls = asked.filter((a) => a.name === "call_it").length;
  document.querySelector("#task-card .name")?.click();
  await new Promise((r) => setTimeout(r, 100));
  const again = document.querySelector("#task-card .name-edit");
  if (again) {
    again.value = "Something else";
    again.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  }
  await new Promise((r) => setTimeout(r, 200));
  check(
    "clicking a task's name on its card renames it, and Escape leaves it as it was",
    again &&
      asked.filter((a) => a.name === "call_it").length === calls &&
      document.querySelector("#task-card .name")?.textContent === "Rent receipts",
    `${again ? "box" : "no box"}; ${document.querySelector("#task-card .name")?.textContent}`,
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

  // Drawn like the buttons beside it, whatever it says. It was left out of
  // the rules that size and outline them, so it came out as the system's own
  // small button, lower than the rest of the row: "Start again" looked like a
  // label that had slipped, on the one routine somebody needed to start.
  const besideIt = (when) => {
    const tryIt = document.getElementById("routine-try");
    const [a, b] = [pause, tryIt].map((e) => e.getBoundingClientRect());
    const [p, t] = [pause, tryIt].map((e) => getComputedStyle(e));
    check(
      `${when}, it is the height of Try it now and sits on the same line`,
      Math.round(a.height) === Math.round(b.height) && Math.round(a.bottom) === Math.round(b.bottom),
      `height ${Math.round(a.height)}/${Math.round(b.height)}, bottom ${Math.round(a.bottom)}/${Math.round(b.bottom)}`,
    );
    check(
      `${when}, it is outlined in the same type as Try it now`,
      p.fontSize === t.fontSize && p.borderTopWidth === t.borderTopWidth && p.backgroundColor === t.backgroundColor && p.borderRadius === t.borderRadius,
      `font ${p.fontSize}/${t.fontSize}, border ${p.borderTopWidth}/${t.borderTopWidth}, background ${p.backgroundColor}/${t.backgroundColor}, radius ${p.borderRadius}/${t.borderRadius}`,
    );
  };
  if (beingDrawn()) besideIt("Pause");
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
  if (beingDrawn()) besideIt("Start again");

  // And reopening reads it back from the app rather than from the page.
  document.getElementById("repeat").click();
  document.getElementById("repeat").click();
  await new Promise((r) => setTimeout(r, 350));
  check(
    "it is still paused when the panel is opened again",
    /Paused/.test(document.getElementById("routine-says").textContent),
    document.getElementById("routine-says").textContent,
  );

  // Started again, it counts from now, so the next run is a new time and the
  // app is the one that knows it. Kept from when the panel opened, the line
  // said the next run was at 12:34 at a quarter past five.
  FIXTURE.routineDue = Date.now() + 5 * 3600000;
  document.getElementById("routine-pause").click();
  await new Promise((r) => setTimeout(r, 250));
  check(
    "and starting it again says so",
    asked.some((a) => a.name === "routine_off" && a.args?.off === false),
    JSON.stringify(asked.filter((a) => a.name === "routine_off").slice(-1)),
  );
  const fresh = new Date(FIXTURE.routineDue).toLocaleString();
  check(
    "and the line gives the next run the app has now, not the one from before it was paused",
    document.getElementById("routine-says").textContent.includes(fresh) &&
      document.getElementById("routine-pause").textContent === "Pause",
    `${document.getElementById("routine-says").textContent} (wanted ${fresh})`,
  );
  delete FIXTURE.routineDue;
  document.getElementById("repeat").click();

  // More than a page of runs: the ones before the newest twenty can be seen.
  await openTalk("talk-1");
  document.getElementById("repeat").click();
  await new Promise((r) => setTimeout(r, 350));
  const shown = () => document.querySelectorAll("#routine-went-list li").length;
  const more = document.getElementById("routine-went-more");
  check("a long history shows the newest twenty runs", shown() === 20, String(shown()));
  check("and offers the ones before them", !more.hidden, `hidden=${more.hidden}`);
  more.click();
  await new Promise((r) => setTimeout(r, 300));
  check("which are added below, oldest last", shown() === 23 && /ran/.test([...document.querySelectorAll("#routine-went-list li")].pop()?.textContent || ""), `${shown()} shown`);
  check("and with nothing older, it stops offering", more.hidden, `hidden=${more.hidden}`);
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
  const ended = [...document.querySelectorAll("#messages li.ended")].find((li) => li.querySelector("button.again"));
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
    "pressing it asks the question it cut off, not the apology and not the newest request",
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

  // One on the web waits to be asked for: drawing an address is loading it,
  // and the address is the model's to choose.
  const onTheWeb = render("![a chart](https://example.com/c.png)");
  const waiting = onTheWeb.querySelector("button.picture-elsewhere");
  check(
    "one on the web is not fetched until somebody asks, and says where it is from",
    !onTheWeb.querySelector("img.drawn") && /example\.com/.test(waiting?.textContent || ""),
    waiting?.textContent || "no button",
  );
  waiting?.click();
  const shown = onTheWeb.querySelector("img.drawn");
  check("and asking for it shows it", shown?.src === "https://example.com/c.png", shown?.src || "nothing shown");

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

/**
 * Saying yes once, instead of four times.
 *
 * The complaint that started this: allowing the same `curl` over and over and
 * never finding where to make it stop. The button that ends it was there all
 * along and was the plain one beside the accented one people keep pressing --
 * so the app watched somebody answer the same question three times and went on
 * pointing at the answer that brings it back.
 */
export async function allowingSomethingOnce() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });

  // Its own conversation: three of the same question already answered, so the
  // app has watched somebody say yes to curl three times. Borrowing talk-4
  // meant another group answered its open question first.
  await openTalk("talk-tired");
  // The fourth arrives live, which is the only way it carries what pressing
  // Always would allow: a question read back off disk has the words but not
  // the rule behind them.
  tell("happened", {
    conversation: "talk-tired",
    seq: 5,
    kind: "needs_you",
    asking: "Fetch the price once more",
    detail: "curl -s https://example.com/price",
    tool: "Bash",
    call: "t5",
    step: "t5",
    can_remember: true,
    rule: "curl",
    allows: "any curl command",
  });
  await new Promise((r) => setTimeout(r, 350));
  const card = [...document.querySelectorAll("#messages li.asking .choices")].pop();
  check("the open question still offers a way through", card, String(!!card));
  if (!card) return found;

  const labels = [...card.querySelectorAll("button")].map((b) => b.textContent);
  const always = [...card.querySelectorAll("button")].find((b) => /^Always/.test(b.textContent));
  check("it offers to allow this from now on", always, JSON.stringify(labels));
  check(
    "and says what that would allow before it is pressed",
    /Always · /.test(always?.textContent || ""),
    always?.textContent,
  );
  // The whole fix: after saying yes before, the standing answer leads and the
  // one-off is the plain one.
  check(
    "having said yes before, allowing it is the button that leads",
    always?.classList.contains("leading"),
    always?.className,
  );
  const once = [...card.querySelectorAll("button")].find((b) => /Just this once/.test(b.textContent));
  check("and saying yes once is still offered, plainly", once && once.classList.contains("plain"), once?.className);
  check(
    "the count is said rather than left to be felt",
    /allowed this \d+ times already|allowed this once already/.test(
      [...document.querySelectorAll("#messages li.asking")].pop()?.textContent || "",
    ),
    [...document.querySelectorAll("#messages li.asking .over-and-over")].pop()?.textContent,
  );

  // And a rule can be written without waiting to be interrupted at all.
  document.getElementById("allowed-open")?.click();
  const panel = document.getElementById("granting");
  panel.hidden = false;
  const what = document.getElementById("allow-what");
  const says = document.getElementById("allow-says");
  check("there is somewhere to say it before being asked", what, String(!!what));
  if (!what) return found;

  what.value = "git status";
  document.getElementById("allow-ahead").dispatchEvent(new Event("submit"));
  await new Promise((r) => setTimeout(r, 350));
  check(
    "writing one tells the app",
    asked.some((a) => a.name === "allow_in_advance" && a.args?.rule === "git status"),
    JSON.stringify(asked.filter((a) => a.name === "allow_in_advance").slice(-1)),
  );
  // What it actually allows, not what was typed: `git status` becomes any git
  // command, which is wider than it looks and has to be said.
  check(
    "and says what it really allows, which is wider than what was typed",
    /any git command/.test(says?.textContent || ""),
    says?.textContent,
  );
  check("the box is cleared, so it cannot be sent twice", what.value === "", what.value);
  panel.hidden = true;
  return found;
}

/**
 * A picture you can see before you send it, and a warning before you type.
 *
 * Two failures with the same shape: the app knew something and showed the least
 * useful version of it. A pasted screenshot is called `image.png` by the
 * system, so the composer said "image.png" and there was no way to tell two
 * apart or notice the wrong one had been pasted until it was sent. And a login
 * that had expired was reported after a paragraph, a screenshot and a request
 * for a daily errand had already been written.
 */
export async function seeingItBeforeYouSendIt() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const strip = document.getElementById("attached");

  await openTalk("talk-1");
  // A pasted picture arrives as a data URL and needs nothing from the app.
  const pasted = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
  window.__ATTACH__({ name: "image.png", url: pasted });
  await new Promise((r) => setTimeout(r, 250));
  const shown = strip.querySelector(".attached-one img");
  check("a pasted picture is shown, not named", shown, strip.textContent || "nothing");
  check("and it is the picture itself", shown?.src === pasted, String(shown?.src || "").slice(0, 22));
  check(
    "its name is still there for a screen reader",
    shown?.alt === "image.png",
    shown?.alt,
  );

  // Taking it off is its own control: clicking a picture means "show me it"
  // everywhere else, and removing something is not a thing to do by accident.
  const off = strip.querySelector(".attached-one .take-off");
  check("taking it off again is its own control", off, strip.innerHTML.slice(0, 80));
  off?.click();
  await new Promise((r) => setTimeout(r, 200));
  check("and pressing it takes it off", strip.hidden, `hidden=${strip.hidden}`);

  // A dropped one is a path, which the window cannot read: the app does.
  window.__ATTACH__({ name: "shot.png", url: "/tmp/shot.png" });
  await new Promise((r) => setTimeout(r, 350));
  check(
    "a dropped picture is read by the app and shown too",
    strip.querySelector(".attached-one img")?.src?.startsWith("data:image/"),
    String(strip.querySelector(".attached-one img")?.src || "").slice(0, 22),
  );
  strip.querySelector(".attached-one .take-off")?.click();
  await new Promise((r) => setTimeout(r, 150));

  // A picture belongs to the conversation it was pasted into. It went to
  // whichever agent was spoken to next.
  window.__ATTACH__({ name: "for-talk-1.png", url: pasted });
  await new Promise((r) => setTimeout(r, 150));
  // A conversation no check after this one opens, with nothing waiting in
  // it, since sending leaves a line there.
  await openTalk("talk-overnight");
  check("a picture waiting in one conversation is not waiting in the next", strip.hidden, `hidden=${strip.hidden}, ${strip.querySelectorAll(".attached-one").length} shown`);
  const says = asked.length;
  document.getElementById("what").value = "Anything new?";
  document.getElementById("composer").requestSubmit();
  await new Promise((r) => setTimeout(r, 300));
  const went = asked.slice(says).find((a) => a.name === "say");
  check("and sending there sends no picture", went && !went.args?.attached, JSON.stringify(went?.args || "nothing sent"));
  tell("happened", { conversation: "talk-overnight", seq: 9851, kind: "done" });
  await new Promise((r) => setTimeout(r, 150));
  await openTalk("talk-1");
  check(
    "coming back, it is waiting where it was pasted",
    !strip.hidden && strip.querySelector(".attached-one img")?.alt === "for-talk-1.png",
    strip.querySelector(".attached-one img")?.alt || "nothing waiting",
  );
  strip.querySelector(".attached-one .take-off")?.click();
  await new Promise((r) => setTimeout(r, 150));

  // And the warning, which has to be there before anything is typed.
  const trouble = document.getElementById("trouble");
  check("nothing is warned about when nothing is wrong", trouble.hidden, `hidden=${trouble.hidden}`);
  await tell("trouble", {
    said: "Claude Code is signed out.",
    fix: "Run `claude` in a terminal and sign in.",
    until_somebody_acts: true,
  });
  await new Promise((r) => setTimeout(r, 200));
  check("a login that has expired is said above the box", !trouble.hidden, `hidden=${trouble.hidden}`);
  check(
    "and it says what to do rather than only what went wrong",
    /sign in/i.test(trouble.querySelector(".fix")?.textContent || ""),
    trouble.querySelector(".fix")?.textContent,
  );

  // Something that clears on its own must not put a standing warning up.
  await tell("trouble_over", {});
  await new Promise((r) => setTimeout(r, 150));
  check("and it goes away once something gets through", trouble.hidden, `hidden=${trouble.hidden}`);
  await tell("trouble", { said: "The model server is busy.", fix: "It usually clears.", until_somebody_acts: false });
  await new Promise((r) => setTimeout(r, 200));
  check(
    "a busy server is not turned into a standing warning",
    trouble.hidden,
    `hidden=${trouble.hidden}`,
  );
  return found;
}

/**
 * A room: several agents in one conversation.
 *
 * Hand-off is one-to-one and lands in a conversation nobody was in. A room is
 * the other thing people do with a team, and the window has to say three
 * things it never had to before: who is in it, who said each line, and who is
 * answering this second.
 */
export async function aRoomOfSeveral() {
  const found = [];
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  await openTalk("talk-room");
  const members = document.getElementById("members");
  const everybody = "Room: Bitcoin Desk and Show me the latest Bitcoin news";
  found.push({
    what: "a room names its members in the header",
    ok: !members.hidden && members.textContent === everybody,
    saw: members.hidden ? "hidden" : members.textContent,
  });
  const whos = [...document.querySelectorAll("#messages .said .who")].map((w) => w.textContent);
  found.push({
    what: "every answer in a room says which member gave it",
    ok: whos.join("|") === "Bitcoin Desk|Show me the latest Bitcoin news",
    saw: whos.join("|") || "no names",
  });
  const onMine = document.querySelectorAll("#messages .mine .who").length;
  found.push({
    what: "what the person said carries no name",
    ok: onMine === 0,
    saw: `${onMine} labels on your own lines`,
  });
  const what = document.getElementById("what");
  found.push({
    what: "the composer says how to speak to one member",
    ok: /@Name/.test(what.placeholder),
    saw: what.placeholder,
  });

  tell("room_turn", { conversation: "talk-room", who: "Bitcoin Desk", over: false });
  const answering = document.querySelector("#messages .answering");
  found.push({
    what: "while a member is answering, the room says which one",
    ok:
      !!document.querySelector("#messages .thinking") &&
      answering?.textContent === "Bitcoin Desk is answering",
    saw: answering?.textContent || "nothing under the dots",
  });
  // A room takes one thing round at a time. Two rounds at once in one room
  // wrote a member down as stopped while it was still answering, and wrote
  // its answer to the first message down as the answer to the second.
  const saidBefore = asked.filter((a) => a.name === "say").length;
  what.value = "And the volume?";
  document.getElementById("composer").requestSubmit();
  await settle(100);
  const refused = [...document.querySelectorAll("#messages .ended")].pop();
  found.push({
    what: "something said mid-round is kept in the box and refused, without asking the app",
    ok:
      what.value === "And the volume?" &&
      asked.filter((a) => a.name === "say").length === saidBefore &&
      /still answering/.test(refused?.textContent || "") &&
      !refused.classList.contains("failed"),
    saw: `box: ${JSON.stringify(what.value)}; ${refused?.textContent || "nothing said"}`,
  });
  document.getElementById("composer").requestSubmit();
  await settle(100);
  found.push({
    what: "and pressing Return again does not say it twice",
    ok: document.querySelectorAll("#messages .ended").length === 1,
    saw: `${document.querySelectorAll("#messages .ended").length} lines`,
  });
  what.value = "";
  tell("noted", {
    conversation: "talk-room",
    seq: 9,
    kind: "said",
    text: "Up 2% since.",
    said_by: "agent-bitcoin",
  });
  const last = [...document.querySelectorAll("#messages .said")].pop();
  found.push({
    what: "an answer arriving live is labelled the same as one read back",
    ok: last?.querySelector(".who")?.textContent === "Bitcoin Desk" && /Up 2% since/.test(last.textContent),
    saw: last?.textContent.slice(0, 60) || "nothing drawn",
  });
  tell("room_turn", { conversation: "talk-room", over: true });
  found.push({
    what: "when the round is over the dots go",
    ok: !document.querySelector("#messages .thinking") && !document.querySelector("#messages .answering"),
    saw: document.querySelector("#messages .thinking") ? "still thinking" : "no dots",
  });

  await openTalk("talk-2");
  found.push({
    what: "an ordinary conversation names no members and asks plainly",
    ok: members.hidden && what.placeholder === "What would you like done?",
    saw: `${members.hidden ? "hidden" : members.textContent}; ${what.placeholder}`,
  });

  const picker = document.getElementById("talks");
  const offered = [...picker.options].some((o) => o.value === "room" && o.textContent === "New room…");
  found.push({
    what: "the picker offers a new room",
    ok: offered,
    saw: [...picker.options].map((o) => o.textContent).join(", "),
  });
  picker.value = "room";
  picker.dispatchEvent(new Event("change"));
  await settle(200);
  const rooming = document.getElementById("rooming");
  const boxes = [...rooming.querySelectorAll("#rooming-who input")];
  found.push({
    what: "choosing it lists every named agent to tick, with the open one ticked",
    ok:
      !rooming.hidden &&
      boxes.length === FIXTURE.agents.length &&
      boxes.find((b) => b.value === "agent-bitcoin")?.checked === true &&
      picker.value === "talk-2",
    saw: rooming.hidden
      ? "panel hidden"
      : `${boxes.length} to tick, ticked: ${boxes.filter((b) => b.checked).map((b) => b.value).join(",")}, picker on ${picker.value}`,
  });
  const before = asked.length;
  document.getElementById("rooming-start").click();
  await settle(200);
  const says = document.getElementById("rooming-says");
  found.push({
    what: "one agent ticked is not a room, and it says so rather than asking the app",
    ok: !asked.slice(before).some((a) => a.name === "make_room") && /at least two/.test(says.textContent),
    saw: says.textContent || "nothing said",
  });
  for (const b of boxes) b.checked = true;
  document.getElementById("rooming-name").value = "Desk and news";
  document.getElementById("rooming-start").click();
  await settle(400);
  const made = asked.slice(before).find((a) => a.name === "make_room");
  found.push({
    what: "starting it asks the app for a room of those agents, the open one first",
    ok:
      !!made &&
      made.args.agents.join(",") === "agent-bitcoin,agent-unnamed" &&
      made.args.name === "Desk and news",
    saw: made ? `${made.args.agents.join(",")} called ${made.args.name}` : "the app was not asked",
  });
  found.push({
    what: "and opens it, members in the header and the picker on it",
    ok:
      rooming.hidden &&
      members.textContent === everybody &&
      picker.selectedOptions[0]?.textContent === "Desk and news",
    saw: `${rooming.hidden ? "panel closed" : "panel open"}; ${members.textContent}; picker on ${picker.selectedOptions[0]?.textContent}`,
  });
  return found;
}

/**
 * An agent made outside the window.
 *
 * `errand-app ask` in a terminal, a script, another agent starting a room,
 * the clock: all of them make agents and conversations the window was never
 * told about, and the app then tells the window what happens in them the same
 * as any other. The window used to know only the agents it read when it
 * opened, and dropped the rest on the floor, so a new agent did not appear
 * down the side until Errand was quit and opened again.
 */
export async function madeOutsideTheWindow() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const rows = () => [...document.getElementById("threads").children].map((li) => li.textContent);
  const before = rows().length;

  // Made behind the window's back: in the store, not in the page. First in
  // the list the app hands back, because it has just spoken and that is the
  // app's order.
  FIXTURE.agents.unshift({
    ...FIXTURE.agents[0],
    id: "agent-outside",
    name: "Made in the terminal",
    started_at: 9,
    spoke_at: 9,
  });
  FIXTURE.conversations["agent-outside"] = [
    { id: "talk-outside", agent: "agent-outside", name: "First", opened: true },
  ];
  const asks = asked.length;
  // Two at once, the way a turn arrives: the first line, then the rest of it
  // before anybody could have answered the first.
  tell("happened", { conversation: "talk-outside", seq: 9001, kind: "said", text: "Hello ", settled: false });
  tell("happened", { conversation: "talk-outside", seq: 9002, kind: "said", text: "Hello from outside", settled: true });
  await new Promise((r) => setTimeout(r, 300));

  check(
    "an agent made outside the window appears down the side at its first word",
    rows().some((r) => r.includes("Made in the terminal")),
    `${rows().length} rows, was ${before}`,
  );
  check(
    "at the top, where a relaunch would put it, rather than below the fold",
    rows()[0]?.includes("Made in the terminal"),
    rows()[0]?.slice(0, 40) || "no rows",
  );
  // Its answer lands after the window met it, which is the order things
  // happen in: the app reports something unread only once the turn is done.
  window.__TAURI__.nowUnread("agent-outside", 1, Date.now());
  tell("happened", { conversation: "talk-outside", seq: 9003, kind: "done" });
  await new Promise((r) => setTimeout(r, 300));
  check(
    "and once its answer lands the row says what is new, not that nothing has been said",
    /1 new/.test(rows()[0] || "") && !/Nothing said yet/.test(rows()[0] || ""),
    rows()[0]?.slice(0, 60) || "no rows",
  );
  const since = asked.slice(asks).map((a) => a.name);
  check(
    "having asked the app whose conversation that is, and read the agents again",
    since.includes("conversation_agent") && since.includes("agents"),
    since.join(", ") || "the app was not asked",
  );
  check(
    "once, not once per line that arrived while it was asking",
    since.filter((n) => n === "conversation_agent").length === 1,
    `asked ${since.filter((n) => n === "conversation_agent").length} times`,
  );

  // The same for an agent that settles its name before the window has met it,
  // which is what the first answer of a new agent does.
  FIXTURE.agents.unshift({
    ...FIXTURE.agents[0],
    id: "agent-outside-named",
    name: "Made by a script",
    started_at: 10,
    spoke_at: 10,
  });
  tell("settled", [
    "agent-outside-named",
    { name: "Made by a script", title: null, about: null, mark: null, hue: null },
  ]);
  await new Promise((r) => setTimeout(r, 300));
  check(
    "and one that settles its name before the window has met it appears too",
    rows()[0]?.includes("Made by a script"),
    `${rows().length} rows`,
  );

  // One the app has never heard of either is left alone, and asked about once.
  const stray = asked.length;
  tell("happened", { conversation: "talk-nowhere", seq: 9004, kind: "said", text: "?", settled: true });
  await new Promise((r) => setTimeout(r, 300));
  check(
    "one the app does not have either is asked about and then left alone",
    rows().length === before + 2 &&
      asked.slice(stray).filter((a) => a.name === "conversation_agent").length === 1,
    `${rows().length} rows; asked ${asked.slice(stray).map((a) => a.name).join(", ") || "nothing"}`,
  );
  return found;
}

/**
 * Pausing an agent.
 *
 * One switch for everything it does on its own, in the header beside Pin and
 * Hide, in the menu on its row, and in the palette. And the state has to be
 * readable down the side without a click: which of these are actually
 * running is the question the list is for.
 */
export async function pausingAnAgent() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const rowOf = (id) => [...document.querySelectorAll("#threads li")].find((li) => li.dataset.agent === id);
  const button = document.getElementById("pause");

  // Whoever is open: the header's buttons act on the agent on screen.
  const open = document.querySelector('#threads li[aria-current="true"]')?.dataset.agent || "";
  const talk = document.getElementById("talks").value;
  check("the header offers Pause beside Pin and Hide", button && !button.hidden && button.textContent === "Pause", button?.textContent || "no button");

  // Working, so that pausing has something to stop.
  tell("happened", { conversation: talk, seq: 9101, kind: "started", session: talk, model: "test" });
  await settle(150);
  const working = rowOf(open)?.querySelector(".last")?.textContent || "";

  const asks = asked.length;
  button.click();
  await settle(300);
  const row = rowOf(open);
  const line = row?.querySelector(".last")?.textContent || "";
  check(
    "pressing it asks the app to pause that agent",
    asked.slice(asks).some((a) => a.name === "pause" && a.args.id === open && a.args.paused === true),
    asked.slice(asks).map((a) => a.name).join(", ") || "the app was not asked",
  );
  check("and the button says so, pressed", button.textContent === "Paused" && button.getAttribute("aria-pressed") === "true", `${button.textContent} pressed=${button.getAttribute("aria-pressed")}`);
  check("the row down the side says Paused, in place of Working", row?.classList.contains("paused") && line === "Paused", `was "${working}", now "${line}"`);
  check("and its conversation no longer shows as working", !document.querySelector("#composer.working, #messages .working"), "no working marker");

  // The menu on the row offers the way back.
  row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 120, clientY: 200 }));
  await settle(150);
  const menu = document.getElementById("menu");
  const items = [...menu.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent);
  check("the menu on a paused agent offers Start again", items.includes("Start again"), items.join(" | "));
  const again = [...menu.querySelectorAll('[role="menuitem"]')].find((b) => b.textContent === "Start again");
  const before = asked.length;
  again?.click();
  await settle(300);
  check(
    "and pressing it asks the app to start it again",
    asked.slice(before).some((a) => a.name === "pause" && a.args.id === open && a.args.paused === false),
    asked.slice(before).map((a) => a.name).join(", ") || "the app was not asked",
  );
  const back = rowOf(open);
  check(
    "after which the row and the button read as running again",
    !back?.classList.contains("paused") && back?.querySelector(".last")?.textContent !== "Paused" && button.textContent === "Pause",
    `${back?.querySelector(".last")?.textContent} / ${button.textContent}`,
  );

  // The menu on an agent that is not paused offers Pause.
  back.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 120, clientY: 200 }));
  await settle(150);
  const offered = [...menu.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent);
  check("and the menu on a running agent offers Pause", offered.includes("Pause"), offered.join(" | "));
  document.body.click();
  await settle(100);

  // The palette has it too, worded for the state it is in.
  // Opened the way somebody would: through the palette.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  await settle(200);
  const palette = [...document.querySelectorAll("#palette-list li")].map((li) => li.textContent);
  check("the palette offers to pause this agent", palette.some((p) => p.startsWith("Pause this agent")), palette.find((p) => /agent/.test(p)) || `${palette.length} entries`);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await settle(100);
  return found;
}

/**
 * A handover that stops waiting, and an agent paused from somewhere else.
 *
 * A handover told nobody: nothing down the side said an agent was waiting on
 * a sign-in, and a card whose agent had gone kept buttons that told a call
 * nobody was making. And an agent that paused because it was asked to never
 * showed it here.
 */
export async function aHandoverThatStopsWaiting() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  // In a conversation with nothing else waiting in it, so the side is saying
  // what this card is doing and not what another one is.
  await openTalk("talk-outside");
  const open = document.querySelector('#threads li[aria-current="true"]')?.dataset.agent || "";
  const where = document.getElementById("talks").value;
  const said = () =>
    [...document.querySelectorAll("#threads li")]
      .find((li) => li.dataset.agent === open)
      ?.querySelector(".last")?.textContent || "";

  tell("handing_over", {
    conversation: where,
    seq: 9700,
    handover: "h-ends",
    what: "Turn on Full Disk Access for Errand",
    why: "",
    where: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
  });
  await settle(200);
  check("a handover says Needs you down the side, as a question does", said() === "Needs you", said());
  const card = [...document.querySelectorAll("#messages .handover")].pop();
  const link = card?.querySelector("a.detail");
  check(
    "a pane of System Settings is offered as a link, the same as a page",
    link?.textContent.startsWith("x-apple.systempreferences:"),
    link?.textContent || "no link",
  );

  tell("handover_ended", { conversation: where, handover: "h-ends" });
  await settle(200);
  const after = [...document.querySelectorAll("#messages .handover")].pop();
  check(
    "when it stops waiting, the card says so",
    /stopped waiting/.test(after?.textContent || ""),
    (after?.textContent || "").slice(0, 90),
  );
  check("and the side stops saying Needs you", said() !== "Needs you", said());
  const before = asked.length;
  after?.querySelector(".choices button")?.click();
  await settle(300);
  check(
    "and pressing it says the answer into the conversation instead of into a call that has gone",
    asked.slice(before).some((a) => a.name === "say" && /Carry on/.test(a.args?.text || "")),
    asked.slice(before).map((a) => a.name).join(", ") || "nothing was asked",
  );

  tell("paused", { agent: open, paused: true });
  await settle(150);
  check("an agent paused from somewhere else reads Paused down the side", said() === "Paused", said());
  tell("paused", { agent: open, paused: false });
  await settle(150);
  check("and one started again from somewhere else reads as running", said() !== "Paused", said());
  return found;
}

/**
 * What an agent remembers, seen and corrected where somebody overrules it.
 *
 * Nineteen notes across ten agents were read into every conversation, and none
 * could be seen or fixed from the window.
 */
export async function whatAnAgentRemembers() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  await openTalk("talk-2");
  document.getElementById("thread-name").click();
  await settle(300);
  const rows = () => [...document.querySelectorAll("#notes-list li")];
  check("its notes are listed under who it is", rows().length === 2 && /Coinbase/.test(rows()[0].textContent), rows().map((li) => li.textContent).join(" | "));
  check("and the heading says how many", document.getElementById("notes-summary").textContent === "What it remembers (2)", document.getElementById("notes-summary").textContent);

  // Corrected in place.
  rows()[0].querySelector("button").click();
  await settle(100);
  const box = rows()[0].querySelector("input");
  check("changing one puts its words in a box", box?.value === "Prices from Coinbase, not Binance", box?.value || "no box");
  box.value = "Prices from Kraken";
  const before = asked.length;
  [...rows()[0].querySelectorAll("button")].find((b) => b.textContent === "Keep")?.click();
  await settle(250);
  const kept = asked.slice(before).find((a) => a.name === "note_down");
  check(
    "and keeping it writes the correction under the same handle",
    kept?.args?.agent === "agent-bitcoin" && kept?.args?.about === "exchange" && kept?.args?.note === "Prices from Kraken",
    JSON.stringify(kept?.args || "nothing written"),
  );

  // Taken back.
  const forgetting = asked.length;
  [...rows()[1].querySelectorAll("button")].find((b) => b.textContent === "Forget")?.click();
  await settle(250);
  const gone = asked.slice(forgetting).find((a) => a.name === "unnote");
  check("forgetting one asks the app to take it back", gone?.args?.about === "report_time", JSON.stringify(gone?.args || "nothing asked"));

  // Written by the person, under the same rules as the agent's own.
  document.getElementById("notes").open = true;
  document.getElementById("note-about").value = "Dentist";
  document.getElementById("note-text").value = "Dr Weber, only on Tuesdays";
  const adding = asked.length;
  document.getElementById("note-new").requestSubmit();
  await settle(250);
  const added = asked.slice(adding).find((a) => a.name === "note_down");
  check("a note can be written down from here", added?.args?.about === "Dentist" && /Tuesdays/.test(added?.args?.note || ""), JSON.stringify(added?.args || "nothing written"));
  document.getElementById("note-about").value = "deepseek";
  document.getElementById("note-text").value = "key sk-abcdefghijklmnopqrstuvwx";
  document.getElementById("note-new").requestSubmit();
  await settle(250);
  const says = document.getElementById("notes-says").textContent;
  check("and one holding a key is refused, saying why", /password or a key/.test(says) && document.getElementById("note-text").value.includes("sk-"), says || "nothing said");
  document.getElementById("thread-name").click();
  await settle(100);
  return found;
}

/**
 * A room whose members can change, whose members are offered behind @, and
 * whose round can be stopped.
 *
 * Members were fixed when a room was made, a name typed slightly wrong went
 * to nobody, and Stop left the member answering running and the round going.
 */
export async function changingARoom() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  await openTalk("talk-room");
  const header = document.getElementById("members");
  check("a room's members are a button in the header", header.tagName === "BUTTON" && !header.hidden, `${header.tagName} hidden=${header.hidden}`);
  header.click();
  await settle(200);
  const boxes = [...document.querySelectorAll("#rooming-who input")];
  const ticked = boxes.filter((b) => b.checked).map((b) => b.value).sort();
  check(
    "pressing it offers everybody, with the ones in the room ticked",
    !document.getElementById("rooming").hidden && ticked.join(",") === "agent-bitcoin,agent-unnamed" && document.getElementById("rooming-start").textContent === "Change who is in it",
    `${ticked.join(",")} / ${document.getElementById("rooming-start").textContent}`,
  );
  const another = boxes.find((b) => !b.checked);
  if (another) another.checked = true;
  const changing = asked.length;
  document.getElementById("rooming-start").click();
  await settle(300);
  const changed = asked.slice(changing).find((a) => a.name === "set_members");
  check("and changing it asks the app, with the new one in", changed?.args?.room === "talk-room" && changed?.args?.agents?.length === 3, JSON.stringify(changed?.args || "nothing asked"));
  check("and the header names three now", /,/.test(header.textContent) && document.getElementById("rooming").hidden, header.textContent);

  // Behind @, as it is typed.
  const box = document.getElementById("what");
  box.value = "@bit";
  box.dispatchEvent(new Event("input"));
  await settle(150);
  const offered = document.getElementById("slash");
  check("typing @ in a room offers its members", !offered.hidden && /@Bitcoin Desk/.test(offered.textContent), `hidden=${offered.hidden}: ${offered.textContent}`);
  offered.querySelector("li")?.click();
  await settle(100);
  check("and choosing one puts the name in the box", box.value === "@Bitcoin Desk ", JSON.stringify(box.value));
  box.value = "";
  box.dispatchEvent(new Event("input"));

  // Stopped from the palette while a member answers.
  tell("room_turn", { conversation: "talk-room", who: "Bitcoin Desk", over: false });
  await settle(150);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  await settle(200);
  const stopping = asked.length;
  [...document.querySelectorAll("#palette-list li")].find((li) => li.textContent.startsWith("Stop what it is doing"))?.click();
  await settle(300);
  check("a room answering can be stopped", asked.slice(stopping).some((a) => a.name === "stop" && a.args?.id === "talk-room"), JSON.stringify(asked.slice(stopping).map((a) => a.name)));
  tell("room_turn", { conversation: "talk-room", who: null, over: true });
  await settle(150);
  return found;
}

/**
 * A monthly limit, in dollars for Claude and in tokens for a hosted model.
 *
 * Nothing stopped an agent spending: twelve of thirteen ran on paid models and
 * nothing was even counted.
 */
export async function aMonthlyLimit() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const openWho = async (id) => {
    [...document.querySelectorAll("#threads li")].find((li) => li.dataset.agent === id)?.click();
    await settle(500);
    document.getElementById("thread-name").click();
    await settle(300);
  };

  await openWho("agent-bitcoin");
  const summary = () => document.getElementById("limit-summary").textContent;
  const used = () => document.getElementById("limit-used").textContent;
  check("a Claude agent's limit is in dollars, beside what it spent this month", summary() === "Monthly limit: $20" && used() === "Spent $4.70 this month." && document.getElementById("limit-unit").textContent === "Dollars a month", `${summary()} / ${used()}`);
  document.getElementById("limit-value").value = "30";
  const setting = asked.length;
  document.getElementById("limit-form").requestSubmit();
  await settle(250);
  const set = asked.slice(setting).find((a) => a.name === "set_limits");
  check("and changing it sets dollars, not tokens", set?.args?.dollars === 30 && set?.args?.tokens === null, JSON.stringify(set?.args || "nothing asked"));
  document.getElementById("thread-name").click();
  await settle(100);

  // A hosted one, counted in tokens.
  FIXTURE.agents.push({
    ...FIXTURE.agents[0],
    id: "agent-hosted",
    name: "Hosted Scout",
    engine: "local",
    pinned: false,
    hidden: false,
    paused_at: null,
    spoke_at: Date.now(),
  });
  FIXTURE.conversations["agent-hosted"] = [{ id: "talk-hosted", agent: "agent-hosted", name: "First", opened: true }];
  tell("happened", { conversation: "talk-hosted", seq: 9991, kind: "said", text: "Hello", settled: true });
  tell("happened", { conversation: "talk-hosted", seq: 9992, kind: "done" });
  await settle(400);
  await openWho("agent-hosted");
  check("a hosted model's limit is in tokens, beside what it used this month", used() === "Used 1.2M tokens this month." && document.getElementById("limit-unit").textContent === "Tokens a month", `${summary()} / ${used()}`);
  document.getElementById("limit-value").value = "5M";
  const tokensAsked = asked.length;
  document.getElementById("limit-form").requestSubmit();
  await settle(250);
  const tokens = asked.slice(tokensAsked).find((a) => a.name === "set_limits");
  check("and 5M is read as five million tokens", tokens?.args?.tokens === 5000000 && tokens?.args?.dollars === null, JSON.stringify(tokens?.args || "nothing asked"));
  document.getElementById("limit-value").value = "plenty";
  document.getElementById("limit-form").requestSubmit();
  await settle(150);
  check("and something that is not an amount is refused, saying how to write one", /like 5M or 500k/.test(document.getElementById("limit-says").textContent), document.getElementById("limit-says").textContent);
  document.getElementById("thread-name").click();
  await settle(100);
  return found;
}

/**
 * An agent copied, saved to a file, and started again from one.
 *
 * Every new agent started from nothing, and one set up with care had to be set
 * up with care again for the next.
 */
export async function copyingAnAgent() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const rowOf = (id) => [...document.querySelectorAll("#threads li")].find((li) => li.dataset.agent === id);
  const menuItems = () => [...document.querySelectorAll('#menu [role="menuitem"]')];
  const openMenu = (row) => row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 120, clientY: 200 }));

  openMenu(rowOf("agent-bitcoin"));
  await settle(150);
  const labels = menuItems().map((b) => b.textContent);
  check("the menu on an agent offers to duplicate it and to save it to a file", labels.includes("Duplicate") && labels.includes("Save to a file"), labels.join(" | "));

  const copying = asked.length;
  menuItems().find((b) => b.textContent === "Duplicate")?.click();
  await settle(700);
  const copied = asked.slice(copying).find((a) => a.name === "duplicate");
  check("duplicating asks the app for a copy of that agent", copied?.args?.id === "agent-bitcoin", JSON.stringify(copied?.args || "nothing asked"));
  check("and the copy is opened, under its own name", document.getElementById("thread-name").textContent === "Bitcoin Desk copy", document.getElementById("thread-name").textContent);

  openMenu(rowOf("agent-bitcoin"));
  await settle(150);
  menuItems().find((b) => b.textContent === "Save to a file")?.click();
  await settle(400);
  const saidWhere = [...document.querySelectorAll("#messages li.ended")].pop()?.textContent || "";
  check("saving one says where the file went", /Saved Bitcoin Desk to \/Users\/you\/Desktop\/Bitcoin Desk\.errand\.json/.test(saidWhere), saidWhere.slice(0, 120));

  const loading = asked.length;
  await window.__LOAD_AGENT__("/Users/you/Downloads/Scout.errand.json");
  await settle(500);
  const loaded = asked.slice(loading).find((a) => a.name === "load_agent");
  check("a file dropped on the window starts an agent from it", loaded?.args?.path === "/Users/you/Downloads/Scout.errand.json" && document.getElementById("thread-name").textContent === "Loaded Scout", `${JSON.stringify(loaded?.args || "nothing asked")} / ${document.getElementById("thread-name").textContent}`);
  document.body.click();
  await settle(100);
  return found;
}

/**
 * A long conversation: drawn from its newest lines, and nobody's place taken.
 *
 * Every line was drawn again on every few words of an answer, and the view
 * jumped to the bottom each time, so a 4,775-line conversation froze the
 * window for minutes and a line being read further up was taken away.
 */
/**
 * Whether the box says so when the agent is waiting on an answer.
 *
 * An agent ended on "Have you granted Errand Full Access to Calendars yet? A
 * yes and I'll set the watch", and the box under it said "What would you like
 * done?": nothing on screen said an answer was what it was waiting for.
 */
export async function anOpenQuestion() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const line = (seq, kind, text) => ({ seq, at: Date.now() - (10 - seq) * 60000, kind, text, call: null, tool: null, outcome: null });
  FIXTURE.lines["talk-asks"] = [
    line(1, "mine", "Tell me fifteen minutes before each meeting"),
    line(2, "said", "Calendar access is blocked.\n\n**Have you granted Errand Full Access to Calendars yet?** A yes and I'll set the watch."),
    line(3, "note", "Notifications are off for Errand in macOS."),
  ];
  FIXTURE.lines["talk-choice"] = [
    line(1, "mine", "Set up the digest"),
    line(2, "said", "Do you want it daily or weekly?"),
  ];
  FIXTURE.lines["talk-told"] = [
    line(1, "mine", "What is 17 times 23?"),
    line(2, "said", "391."),
  ];
  for (const [id, name] of [["talk-asks", "Asks"], ["talk-choice", "Choice"], ["talk-told", "Told"]]) {
    FIXTURE.conversations["agent-outside"].push({ id, agent: "agent-outside", name, opened: true });
  }
  const box = document.getElementById("what");
  const replies = document.getElementById("replies");

  await openTalk("talk-asks");
  await settle(150);
  check(
    "a question left open is what the box says, a note after it notwithstanding",
    box.placeholder === "Answer: Have you granted Errand Full Access to Calendars yet?",
    box.placeholder,
  );
  check("and a yes-or-no question has the two answers above it", !replies.hidden, `hidden=${replies.hidden}`);
  box.value = "Not yet, tomorrow";
  box.dispatchEvent(new Event("input"));
  check("which go away while somebody answers in words of their own", replies.hidden, `hidden=${replies.hidden}`);
  box.value = "";
  box.dispatchEvent(new Event("input"));
  check("and come back when the box is empty again", !replies.hidden, `hidden=${replies.hidden}`);
  const before = asked.filter((a) => a.name === "say").length;
  replies.querySelector('[data-say="Yes"]').click();
  await settle(200);
  const sent = asked.filter((a) => a.name === "say").slice(before);
  check("Yes says yes", sent.length === 1 && sent[0].args.text === "Yes", JSON.stringify(sent.map((a) => a.args.text)));

  await openTalk("talk-choice");
  await settle(150);
  check(
    "a question offering a choice is said in the box, with no yes or no to press",
    box.placeholder === "Answer: Do you want it daily or weekly?" && replies.hidden,
    `${box.placeholder} / hidden=${replies.hidden}`,
  );

  await openTalk("talk-told");
  await settle(150);
  check(
    "an answer that asks nothing leaves the box as it was",
    box.placeholder === "What would you like done?" && replies.hidden,
    `${box.placeholder} / hidden=${replies.hidden}`,
  );
  // Repeat fills in the task without the words saying who asked it: in a
  // routine they would be the routine asking.
  FIXTURE.lines["talk-from-outside"] = [
    line(1, "mine", "something outside asks: Use the GitHub search API to count the stars"),
    line(2, "said", "412 stars."),
  ];
  FIXTURE.conversations["agent-outside"].push({ id: "talk-from-outside", agent: "agent-outside", name: "From outside", opened: true });
  await openTalk("talk-from-outside");
  await settle(150);
  document.getElementById("repeat").click();
  await settle(250);
  const task = document.getElementById("routine-what").value;
  check(
    "a task asked from outside is repeated as the task, without who asked it",
    task === "Use the GitHub search API to count the stars",
    JSON.stringify(task),
  );
  document.getElementById("repeat").click();
  await settle(100);

  // Hidden meaning out of sight, not only marked so. The skills list was
  // marked hidden and stayed on screen, because a display of its own
  // outranked the attribute; checks that read the attribute all passed.
  for (const id of ["replies", "slash"]) {
    const it = document.getElementById(id);
    check(
      `the ${id} list is out of sight when it is hidden`,
      !it.hidden || getComputedStyle(it).display === "none",
      `hidden=${it.hidden}, display=${getComputedStyle(it).display}`,
    );
  }
  return found;
}

/**
 * What an agent is for, under its name, and where a hidden one went.
 *
 * The name was squeezed to "Inb..." by the buttons beside it, and what an agent
 * handled was on screen only in a box behind it. And a hidden agent could be
 * found again only by searching for it, which nothing said.
 */
export async function whatItIsForAndWhereItWent() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const owner = Object.entries(FIXTURE.conversations).find(([, list]) => list.some((c) => c.id === "talk-1"))[0];
  await openTalk("talk-1");
  tell("settled", [owner, { name: "Bitcoin Desk", title: "Markets", about: "I watch the price and the news that moves it.", mark: "chart", hue: "gold" }]);
  await settle(150);
  const purpose = document.getElementById("purpose");
  check(
    "what it is for is on the line under its name",
    !purpose.hidden &&
      getComputedStyle(purpose).display !== "none" &&
      purpose.textContent === "Markets \u00b7 I watch the price and the news that moves it.",
    `hidden=${purpose.hidden}: ${purpose.textContent}`,
  );
  const name = document.getElementById("thread-name");
  check(
    "and its name is not squeezed to a few letters",
    name.scrollWidth <= name.clientWidth + 1 || name.clientWidth >= 90,
    `${name.clientWidth}px of ${name.scrollWidth}px`,
  );

  const list = document.getElementById("threads");
  // The side keeps to its column, with room for the overview's button.
  const side = document.getElementById("side").getBoundingClientRect();
  const plus = document.getElementById("new").getBoundingClientRect();
  check(
    "the side keeps to its column, its buttons inside it",
    side.width <= 233 && plus.right <= side.right,
    `side ${Math.round(side.width)}px, + ends at ${Math.round(plus.right)} of ${Math.round(side.right)}`,
  );

  // By which agent the row is, not by what it says: another agent in the list
  // is called "Bitcoin Desk copy".
  const rowOf = () => list.querySelector(`li[data-agent="${owner}"]`);
  // From the menu behind the last button in the header, where Hide is now.
  const fromTheMenu = async (label) => {
    document.getElementById("more").click();
    await settle(80);
    [...document.querySelectorAll("#menu button")].find((b) => b.textContent === label)?.click();
  };
  await fromTheMenu("Hide from the list");
  await settle(150);
  const hiddenRow = list.querySelector("li.the-hidden button");
  check("hiding it says how many are hidden, at the bottom of the list", /Hidden \(\d+\)/.test(hiddenRow?.textContent || ""), hiddenRow?.textContent || "no row");
  check("and it is out of the list", !rowOf(), rowOf() ? "still listed" : "out");
  hiddenRow?.click();
  await settle(120);
  check("pressing that row shows it again, marked as hidden", rowOf()?.classList.contains("is-hidden"), rowOf()?.className || "not shown");
  list.querySelector("li.the-hidden button")?.click();
  await settle(120);
  check("and pressing it again puts it away", !rowOf(), rowOf() ? "still shown" : "put away");
  await fromTheMenu("Show in the list");
  await settle(150);
  check("showing it in the list again takes the row away when nothing else is hidden", !list.querySelector("li.the-hidden") || /Hidden/.test(list.textContent), list.querySelector("li.the-hidden")?.textContent || "gone");
  return found;
}

/**
 * Every task at once: what happened while you were away, what is open, a tile
 * for each task with its teammate on it, grouped by what each is doing or by
 * teammate, priorities, and tasks marked finished while teammates never are.
 */
export async function theOverview() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const overview = document.getElementById("overview");
  const away = document.getElementById("overview-away");
  const tiles = document.getElementById("overview-tiles");
  const list = document.getElementById("threads");

  // Grouped by subject, the last time somebody looked, before tasks.
  try {
    localStorage.setItem("errand-overview-group", "subject");
  } catch {
    // Without storage there is nothing kept to fall back from.
  }
  // What the app's list of what is running says, arriving the way the app
  // sends it: a question in one task and a step in another. Now reads where a
  // task stands from what arrived, as its chip and the side do; the list only
  // words the line.
  tell("happened", {
    conversation: "talk-3", seq: 9801, kind: "needs_you", asking: "Running a command", detail: "ls",
    tool: "Bash", call: "ov-q", step: "ov-q", can_remember: false, rule: "", allows: "",
  });
  tell("happened", { conversation: "talk-2", seq: 9802, kind: "doing", what: "Looking something up on the web", tool: "WebSearch", call: "ov-s" });
  await settle(150);
  document.getElementById("overview-open").click();
  await settle(300);
  check("the overview opens over the window", !overview.hidden && getComputedStyle(overview).display !== "none", `hidden=${overview.hidden}`);
  check(
    "a grouping kept from before that is no longer offered falls back to what each is doing",
    document.getElementById("overview-group").value === "state",
    document.getElementById("overview-group").value || "nothing",
  );
  check(
    "it says what ran while you were away, and how much of it failed",
    /2 runs on their own, 1 of them failed/.test(away.textContent) &&
      away.textContent.includes("BTC is up 2% overnight.") &&
      /failed\. The model server answered with an error/.test(away.textContent),
    away.textContent.slice(0, 200),
  );
  check("and what is open now, starting with what needs you", /Needs you: [^\n]*Bitcoin Desk/.test(away.textContent), away.textContent.slice(-160));

  const tileOf = (id) => tiles.querySelector(`.job[data-task="${id}"]`);
  const stateOn = (id) => tileOf(id)?.querySelector(".job-state")?.dataset.state;
  const all = () => [...tiles.querySelectorAll(".job")].map((j) => j.dataset.task);
  check(
    "every task has a tile of its own, with its teammate on it",
    all().length >= 6 && /Bitcoin Desk/.test(tileOf("talk-3")?.querySelector(".job-role")?.textContent || ""),
    `${all().length} tiles; ${tileOf("talk-3")?.querySelector(".job-role")?.textContent || "no talk-3"}`,
  );
  check(
    "one teammate's tasks are each in their own state, what waits on you first",
    tiles.querySelector(".job-group h2")?.textContent.startsWith("Needs you") &&
      stateOn("talk-3") === "waiting" &&
      stateOn("talk-2") === "working",
    `${tiles.querySelector(".job-group h2")?.textContent || "no groups"}: talk-3 ${stateOn("talk-3")}, talk-2 ${stateOn("talk-2")}`,
  );
  check(
    "a task nobody named is called by the first thing asked in it",
    tileOf("talk-2")?.querySelector(".job-name")?.textContent === "What moved overnight in Bitcoin?" &&
      tileOf("talk-3")?.querySelector(".job-name")?.textContent === "Asked by Day Check",
    `${tileOf("talk-2")?.querySelector(".job-name")?.textContent} / ${tileOf("talk-3")?.querySelector(".job-name")?.textContent}`,
  );

  const priority = tileOf("talk-2").querySelector("select");
  priority.value = "1";
  priority.dispatchEvent(new Event("change"));
  await settle(150);
  check(
    "a priority chosen on a tile is the task's, and is kept",
    asked.some((a) => a.name === "set_task_priority" && a.args?.id === "talk-2" && a.args?.priority === 1) &&
      tileOf("talk-2")?.dataset.priority === "1" &&
      tileOf("talk-3")?.dataset.priority === "2",
    JSON.stringify(asked.filter((a) => a.name === "set_task_priority").slice(-1)),
  );
  check(
    "and a task that matters most has a frame round its tile",
    getComputedStyle(tileOf("talk-2")).borderTopWidth === "2px",
    getComputedStyle(tileOf("talk-2")).borderTopWidth,
  );
  const deskRow = list.querySelector('li[data-agent="agent-bitcoin"]');
  check(
    "and its teammate is framed in the list while that task is open",
    deskRow?.dataset.priority === "1" && getComputedStyle(deskRow).boxShadow !== "none",
    `${deskRow?.dataset.priority} ${deskRow ? getComputedStyle(deskRow).boxShadow : "no row"}`,
  );

  // What repeats, marked beside the teammate's name, with when and what in
  // its tooltip; and on each task, only its own.
  const repeatMark = deskRow?.querySelector(".repeat-mark");
  check(
    "a teammate that repeats something has a mark in the list saying when and what",
    repeatMark && /Repeats daily 07:00/.test(repeatMark.title) && /What moved overnight/.test(repeatMark.title) && !repeatMark.classList.contains("idle"),
    repeatMark?.title || "no mark",
  );
  const watchMark = deskRow?.querySelector(".watch-mark");
  check(
    "and one that watches has a mark too, dimmed when the watch has stopped",
    watchMark?.classList.contains("idle") && /stopped/.test(watchMark.title),
    watchMark?.title || "no mark",
  );
  const pulseMark = list.querySelector('li[data-agent="agent-unnamed"] .repeat-mark');
  check("a routine whose teammate is paused is dimmed and says so", pulseMark?.classList.contains("idle") && /paused/.test(pulseMark.title), pulseMark?.title || "no mark");
  check(
    "and each task's tile has its own marks, not its teammate's whole lot",
    tileOf("talk-2")?.querySelector(".job-marks .repeat-mark") &&
      !tileOf("talk-2")?.querySelector(".job-marks .watch-mark") &&
      tileOf("talk-4")?.querySelector(".job-marks .watch-mark") &&
      !tileOf("talk-overnight")?.querySelector(".job-marks"),
    `talk-2: ${tileOf("talk-2")?.querySelector(".job-marks")?.children.length || 0} marks, talk-4: ${tileOf("talk-4")?.querySelector(".job-marks")?.children.length || 0}`,
  );

  // Show: only what repeats, then everything again.
  const show = document.getElementById("overview-show");
  check("Show offers repeating, completed and finished", ["repeating", "idle", "finished"].every((v) => [...show.options].some((o) => o.value === v)), [...show.options].map((o) => o.textContent).join(", "));
  show.value = "repeating";
  show.dispatchEvent(new Event("change"));
  await settle(150);
  const repeating = all();
  check(
    "showing what repeats keeps exactly the tasks with a routine or a watch",
    repeating.length === new Set(FIXTURE.standing.map((s) => s.conversation)).size &&
      repeating.every((id) => FIXTURE.standing.some((s) => s.conversation === id)),
    repeating.join(", ") || "none",
  );
  show.value = "all";
  show.dispatchEvent(new Event("change"));
  await settle(150);

  // Search: every task and everything said in it. Named for no teammate, so
  // what is found is found by what the routine does.
  FIXTURE.matching = [];
  const overviewFind = document.getElementById("overview-find");
  overviewFind.value = "Write one small pulse file";
  overviewFind.dispatchEvent(new Event("input"));
  await settle(500);
  check(
    "the overview's search finds the task whose routine does it, and only that",
    all().join(",") === "talk-1",
    all().join(", ") || "nothing",
  );
  delete FIXTURE.matching;
  overviewFind.value = "";
  overviewFind.dispatchEvent(new Event("input"));
  await settle(400);
  check("and emptying it shows every task again", all().length >= 6, `${all().length} tiles`);

  const before = stateOn("talk-4");
  [...tileOf("talk-4").querySelectorAll("button")].find((b) => b.textContent === "Finished").click();
  await settle(200);
  const finishedGroup = [...tiles.querySelectorAll(".job-group")].find((g) => g.querySelector("h2").textContent.startsWith("Finished"));
  check(
    "marking a task finished moves it to Finished",
    asked.some((a) => a.name === "finish_task" && a.args?.id === "talk-4" && a.args?.finished === true) &&
      finishedGroup?.querySelector('.job[data-task="talk-4"]'),
    finishedGroup ? "in Finished" : "no Finished group",
  );
  check(
    "and its teammate carries on, as it was, with its other tasks",
    list.querySelector('li[data-agent="agent-bitcoin"]') &&
      !list.querySelector(".finished-badge") &&
      stateOn("talk-3") === "waiting",
    list.querySelector('li[data-agent="agent-bitcoin"]')?.textContent.slice(0, 60) || "not in the list",
  );
  [...tileOf("talk-4").querySelectorAll("button")].find((b) => b.textContent === "Not finished").click();
  await settle(200);
  check(
    "and not finished after all puts it back where it was",
    asked.some((a) => a.name === "finish_task" && a.args?.id === "talk-4" && a.args?.finished === false) &&
      stateOn("talk-4") === before,
    `${stateOn("talk-4")}, was ${before}`,
  );

  const group = document.getElementById("overview-group");
  group.value = "teammate";
  group.dispatchEvent(new Event("change"));
  await settle(150);
  const headings = [...tiles.querySelectorAll(".job-group h2 .label")].map((h) => h.textContent);
  const deskPanel = [...tiles.querySelectorAll(".job-group")].find((g) => g.querySelector("h2 .label")?.textContent === "Bitcoin Desk");
  check(
    "grouped by teammate, each teammate's tasks are together under its name",
    headings.includes("Bitcoin Desk") &&
      !headings.some((h) => /Needs you|Answered|Next up/.test(h)) &&
      deskPanel?.querySelectorAll(".job").length >= 6,
    `${headings.join(", ")}; ${deskPanel?.querySelectorAll(".job").length || 0} under Bitcoin Desk`,
  );
  group.value = "state";
  group.dispatchEvent(new Event("change"));
  await settle(150);
  const panels = [...tiles.querySelectorAll(".job-group")];
  const [first, second] = panels;
  const between = first && second ? Math.round(second.getBoundingClientRect().top - first.getBoundingClientRect().bottom) : null;
  const ownHead = first?.querySelector("h2")?.getBoundingClientRect();
  const ownTiles = first?.querySelector(".jobs")?.getBoundingClientRect();
  const toOwn = ownHead && ownTiles ? Math.round(ownTiles.top - ownHead.bottom) : null;
  check(
    "each group is a panel, further from the next group than its heading is from its own tiles",
    panels.length >= 2 && getComputedStyle(first).borderTopStyle === "solid" && between > toOwn,
    `${between}px between groups, ${toOwn}px from a heading to its tiles`,
  );
  check(
    "and says what it is in colour as well as in words",
    first?.dataset.state && getComputedStyle(first.querySelector("h2"), "::before").backgroundColor !== "rgba(0, 0, 0, 0)",
    `${first?.dataset.state}: ${first ? getComputedStyle(first.querySelector("h2"), "::before").backgroundColor : "none"}`,
  );

  // Open, on a task: its teammate, at that task.
  [...tileOf("talk-overnight").querySelectorAll("button")].find((b) => b.textContent === "Open").click();
  await settle(400);
  const talks = document.getElementById("talks");
  check(
    "Open on a tile goes to its teammate, at that task",
    overview.hidden && talks.value === "talk-overnight",
    `hidden=${overview.hidden}, on ${talks.value}`,
  );
  check(
    "and the task menu offers a new task, not a new conversation",
    [...talks.options].some((o) => o.textContent === "New task…"),
    [...talks.options].map((o) => o.textContent).join(" | "),
  );

  // Finished, on the task's card: the task on screen, not the teammate.
  const done = document.getElementById("task-done");
  check(
    "on the task's card there is a way to say the task is done",
    done && !done.hidden && done.closest("#task-card") && done.textContent === "Mark finished",
    done ? `hidden=${done.hidden} ${done.textContent} in ${done.closest("#task-card") ? "the card" : "the header"}` : "no button",
  );
  done.click();
  await settle(200);
  const option = [...talks.options].find((o) => o.value === "talk-overnight");
  check(
    "pressing it marks that task finished, ticked in the menu, and it offers to reopen it",
    asked.some((a) => a.name === "finish_task" && a.args?.id === "talk-overnight" && a.args?.finished === true) &&
      done.dataset.finished === "true" &&
      done.textContent === "Reopen" &&
      /✓$/.test(option?.textContent || ""),
    `${done.textContent}; ${option?.textContent}`,
  );
  check(
    "and its teammate is still an ordinary row in the list",
    list.querySelector('li[data-agent="agent-bitcoin"]') && !list.querySelector(".finished-badge, li.is-finished"),
    list.querySelector('li[data-agent="agent-bitcoin"]')?.className ?? "gone",
  );
  // Something new asked in a finished task is that task going again.
  const what = document.getElementById("what");
  what.value = "One more thing about last night";
  what.dispatchEvent(new Event("input"));
  document.getElementById("composer").dispatchEvent(new Event("submit", { cancelable: true }));
  await settle(300);
  check(
    "asking something new in a finished task opens it again",
    asked.some((a) => a.name === "finish_task" && a.args?.id === "talk-overnight" && a.args?.finished === false) &&
      done.dataset.finished === "false" &&
      done.textContent === "Mark finished",
    `${done.textContent}`,
  );

  // How long a finished task stays in the menu is said in Settings, not set.
  const said = document.getElementById("finished-tasks");
  check(
    "Settings says a finished task stays in the menu for a week and nothing is deleted, with nothing to set",
    said && /for a week/.test(said.textContent) && /Nothing is deleted/.test(said.textContent) && !said.querySelector("input"),
    said ? said.textContent.trim().slice(0, 120) : "no card",
  );
  tell("happened", { conversation: "talk-2", seq: 9803, kind: "done" });
  await settle(100);
  return found;
}

/**
 * A teammate made with +: asked who it is before anything else, named by its
 * person, with a job, and ready for its first task.
 */
export async function aNewTeammate() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const whois = document.getElementById("whois");
  const hint = document.getElementById("whois-new");
  const before = asked.length;

  document.getElementById("new").click();
  await settle(300);
  check(
    "+ asks who the new teammate is first, with a word on what to do",
    !whois.hidden && !hint.hidden && document.activeElement?.id === "whois-name",
    `whois hidden=${whois.hidden}, hint hidden=${hint.hidden}, focus on ${document.activeElement?.id}`,
  );
  const wide = (id) => document.getElementById(id).closest("label").getBoundingClientRect().width;
  check(
    "the job has the widest field, and the hint a row of its own",
    wide("whois-about") > wide("whois-title") && hint.getBoundingClientRect().bottom <= document.getElementById("whois-name").getBoundingClientRect().top,
    `job ${Math.round(wide("whois-about"))}px, role ${Math.round(wide("whois-title"))}px`,
  );
  check(
    "and asks for its job by that name",
    /Job/.test(document.getElementById("whois-about").closest("label")?.textContent || ""),
    document.getElementById("whois-about").closest("label")?.textContent || "no label",
  );
  document.getElementById("whois-name").value = "Disk Watch";
  document.getElementById("whois-title").value = "Storage";
  const job = document.getElementById("whois-about");
  job.value = "Keeps an eye on the external SSD";
  job.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  await settle(250);
  const named = asked.slice(before).find((a) => a.name === "rename");
  check(
    "Enter saves it as that teammate, with its role and job",
    named?.args?.name === "Disk Watch" && named?.args?.title === "Storage" && named?.args?.about === "Keeps an eye on the external SSD" && whois.hidden,
    JSON.stringify(named?.args || null),
  );
  const row = [...document.querySelectorAll("#threads li")].find((li) => li.textContent.includes("Disk Watch"));
  check(
    "and it is in the list under its name, ready for its first task",
    row && document.activeElement?.id === "what",
    `row=${!!row}, focus on ${document.activeElement?.id}`,
  );
  check("with its job under its name at the top", /Storage · Keeps an eye on the external SSD/.test(document.getElementById("purpose").textContent), document.getElementById("purpose").textContent);

  // Named now, so opening who it is is no longer about a new teammate.
  document.getElementById("thread-name").click();
  await settle(150);
  check("a teammate with a name is not called new", !whois.hidden && hint.hidden, `whois hidden=${whois.hidden}, hint hidden=${hint.hidden}`);
  document.getElementById("thread-name").click();
  await settle(100);

  row?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 120, clientY: 200 }));
  await settle(120);
  const menu = document.getElementById("menu");
  const labels = [...menu.querySelectorAll("button")].map((b) => b.textContent);
  check("its menu gives it a new task, not a new conversation", labels.includes("New task for this teammate"), labels.join(" | "));
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await settle(80);
  return found;
}

/**
 * A teammate whose words stay on this network: the switch, the line saying
 * where its words go, and the local model it runs on when Errand's model is
 * somebody else's server.
 */
export async function keepingItLocal() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const words = document.getElementById("words-go");
  const open = async (id) => {
    const at = FIXTURE.agents.findIndex((a) => a.id === id);
    [...document.querySelectorAll("#threads li")][at]?.click();
    await settle(300);
  };
  const desk = "agent-bitcoin";

  await open(desk);
  check(
    "the header says where a teammate's words go",
    !words.hidden && words.dataset.state === "away" && /leave your network/.test(words.textContent),
    `${words.dataset.state}: ${words.textContent}`,
  );

  // Errand's model on this Mac: its words stay here.
  FIXTURE.settings.errand_model = "o-local";
  await open(desk);
  check(
    "on a model on this Mac they stay on your network, said with the model",
    words.dataset.state === "here" && /stay on your network/.test(words.textContent) && /qwen2\.5:7b/.test(words.textContent),
    words.textContent,
  );

  // Kept local, from who it is.
  document.getElementById("thread-name").click();
  await settle(150);
  const box = document.getElementById("whois-local");
  box.checked = true;
  box.dispatchEvent(new Event("change"));
  await settle(200);
  check(
    "Keep it local is a switch under who it is, and it is kept",
    asked.some((a) => a.name === "keep_local" && a.args?.id === desk && a.args?.on === true) &&
      /Kept local/.test(words.textContent),
    words.textContent,
  );
  document.getElementById("thread-name").click();
  await settle(100);

  // Errand's model out on the internet, and nothing local chosen: it says it
  // will not run, rather than sending anything.
  FIXTURE.offered.push({
    id: "o-hosted", engine: "local", label: "deepseek-flash \u00b7 DeepSeek", backend: null, sort: 9, mark: "local|hosted",
    settings: '{"provider":"openai-compat","base_url":"https://api.deepseek.com/v1","model":"deepseek-flash"}',
  });
  FIXTURE.settings.errand_model = "o-hosted";
  await open(desk);
  check(
    "kept local with Errand's model elsewhere and nothing local chosen, it says it will not run",
    words.dataset.state === "refused" && /Choose a Local model in Settings/.test(words.textContent),
    `${words.dataset.state}: ${words.textContent}`,
  );

  // The local model, chosen in Settings from what is served here only.
  document.getElementById("setup").click();
  await settle(300);
  const local = document.getElementById("local-model");
  const offered = [...local.options].map((o) => o.value).filter(Boolean);
  check(
    "Settings offers as the local model only what is served here",
    offered.includes("o-local") && !offered.includes("o-default") && !offered.includes("o-opus"),
    offered.join(", ") || "nothing",
  );
  local.value = "o-local";
  local.dispatchEvent(new Event("change"));
  await settle(200);
  check(
    "choosing one is kept",
    asked.some((a) => a.name === "set_setting" && a.args?.key === "local_model" && a.args?.value === "o-local") &&
      /now run on qwen2\.5:7b/.test(document.getElementById("local-model-says").textContent),
    document.getElementById("local-model-says").textContent,
  );
  document.getElementById("models-done").click();
  await settle(150);
  await open(desk);
  check(
    "and the teammate kept local now runs on it, its words on your network",
    words.dataset.state === "here" && /Kept local: its words stay on your network/.test(words.textContent),
    words.textContent,
  );

  // Notifications, always in Settings, with the way straight to Errand's page.
  document.getElementById("setup").click();
  await settle(300);
  const says = document.getElementById("notifications-says");
  check(
    "Settings says whether macOS lets Errand notify, off in so many words",
    says.dataset.state === "refused" && /Off in macOS/.test(says.textContent),
    `${says.dataset.state}: ${says.textContent}`,
  );
  document.getElementById("notifications-open").click();
  await settle(150);
  check(
    "and its button opens Errand's own page of Notifications in System Settings",
    asked.some((a) => a.name === "open_settings" && /Notifications-Settings\.extension\?id=com\.errandai\.errand/.test(a.args?.pane || "")),
    JSON.stringify(asked.filter((a) => a.name === "open_settings").slice(-1)),
  );
  document.getElementById("models-done").click();
  await settle(150);

  // A note from when they were off, read once they are on.
  const line = (seq, kind, text) => ({ seq, at: Date.now() - (10 - seq) * 60000, kind, text, call: null, tool: null, outcome: null });
  FIXTURE.lines["talk-notified"] = [
    line(1, "mine", "Check the disk"),
    line(2, "said", "It is 81% full."),
    line(3, "note", "Notifications are off for Errand in macOS, so nothing says when an errand finishes."),
  ];
  FIXTURE.conversations["agent-outside"].push({ id: "talk-notified", agent: "agent-outside", name: "Notified", opened: true });
  FIXTURE.notifying = "allowed";
  document.getElementById("setup").click();
  await settle(250);
  document.getElementById("models-done").click();
  await settle(100);
  await openTalk("talk-notified");
  await settle(200);
  const shown = document.getElementById("messages").textContent;
  check(
    "an old note that notifications were off says so once they are on",
    /They are on now/.test(shown) && !/Notifications are off for Errand/.test(shown),
    shown.slice(-160),
  );
  delete FIXTURE.notifying;

  // As it was.
  FIXTURE.offered = FIXTURE.offered.filter((o) => o.id !== "o-hosted");
  delete FIXTURE.settings.errand_model;
  delete FIXTURE.settings.local_model;
  const agent = FIXTURE.agents.find((a) => a.id === desk);
  if (agent) delete agent.keep_local;
  return found;
}

export async function aLongConversation() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  // The line under the name, which a long conversation below squeezed until
  // its words were cut through the middle.
  check("the line saying what an agent is for never gives up its height", getComputedStyle(document.getElementById("purpose")).flexShrink === "0", getComputedStyle(document.getElementById("purpose")).flexShrink);
  FIXTURE.lines["talk-long"] = Array.from({ length: 450 }, (_, i) => ({
    seq: i + 1,
    at: Date.now() - (450 - i) * 60000,
    kind: i % 2 ? "said" : "mine",
    text: i === 6 ? "Where is the lighthouse keeper's ledger?" : `Line ${i + 1} of a long day`,
    call: null,
    tool: null,
    outcome: null,
  }));
  FIXTURE.conversations["agent-outside"].push({ id: "talk-long", agent: "agent-outside", name: "A long day", opened: true });
  await openTalk("talk-long");
  const box = document.getElementById("messages");
  const drawnLines = () => box.querySelectorAll(":scope > [data-seq]").length;
  check("a long conversation draws its newest lines, not all of them", drawnLines() === 200, `${drawnLines()} drawn of 450`);
  const earlier = box.querySelector(".earlier button");
  check("and offers the ones before them", /250 more/.test(earlier?.textContent || ""), earlier?.textContent || "no button");
  check("and opens at the bottom", box.scrollHeight - box.scrollTop - box.clientHeight < 80, `${box.scrollTop} of ${box.scrollHeight}`);

  // Words being written leave everything else as it is.
  const firstDrawn = box.querySelector(":scope > [data-seq]");
  ["One ", "moment ", "please"].forEach((piece, n) =>
    tell("happened", { conversation: "talk-long", seq: 9950 + n, kind: "said", text: piece, settled: false }),
  );
  await settle(150);
  check(
    "words arriving redraw only the words",
    firstDrawn.isConnected && /One moment please/.test(box.querySelector(".writing")?.textContent || ""),
    `${firstDrawn.isConnected}: ${box.querySelector(".writing")?.textContent || "nothing being written"}`,
  );

  // Somebody reading further up keeps their place when a step arrives below.
  box.scrollTop = 0;
  tell("happened", { conversation: "talk-long", seq: 9960, kind: "doing", what: "Reading the page", tool: "Bash", call: "long-1" });
  await settle(150);
  check("a line being read further up stays in view when a step arrives below", box.scrollTop < 50, `scrollTop ${box.scrollTop}`);
  tell("happened", { conversation: "talk-long", seq: 9961, kind: "done" });
  await settle(150);

  // The ones before, drawn above, with the same lines still in view.
  box.scrollTop = 0;
  const topSeq = box.querySelector(":scope > [data-seq]").dataset.seq;
  box.querySelector(".earlier button").click();
  await settle(200);
  const still = box.querySelector(`:scope > [data-seq="${topSeq}"]`);
  const at = still ? Math.round(still.getBoundingClientRect().top - box.getBoundingClientRect().top) : null;
  check(
    "showing earlier lines draws them above and keeps the same line in view",
    drawnLines() > 200 && at !== null && at >= -5 && at <= box.clientHeight,
    `${drawnLines()} drawn; line ${topSeq} at ${at}px`,
  );

  // A search that lands further back than is drawn draws back to it.
  await openTalk("talk-1");
  const find = document.getElementById("find");
  find.value = "lighthouse keeper";
  find.dispatchEvent(new Event("input"));
  await settle(400);
  [...document.querySelectorAll("#threads li")].find((li) => li.dataset.agent === "agent-outside")?.click();
  await settle(800);
  const marked = box.querySelector("li.found");
  check("a search that lands further back than is drawn draws back to it", marked?.dataset.seq === "7", marked ? `line ${marked.dataset.seq}` : "nothing marked");
  find.value = "";
  find.dispatchEvent(new Event("input"));
  await settle(300);
  return found;
}

/**
 * Everything that runs on its own, in one list.
 *
 * Routines and watches were found by opening each agent in turn and looking
 * for a clock beside a conversation's name.
 */
export async function everythingThatRunsOnItsOwn() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const panel = document.getElementById("standing");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  const typing = document.getElementById("palette-what");
  typing.value = "runs on its own";
  typing.dispatchEvent(new Event("input"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await settle(300);
  const said = panel.textContent;
  check("the palette opens one list of everything that runs on its own", !panel.hidden && /3 routines and 1 watch, across 2 agents/.test(said), said.slice(0, 90));
  const rows = [...panel.querySelectorAll(".one")];
  const row = (text) => rows.find((r) => r.textContent.includes(text))?.textContent || "";
  check("a routine that will run says when", /daily 07:00/.test(row("What moved overnight")) && /next /.test(row("What moved overnight")), row("What moved overnight"));
  check("one switched off says so, and does not promise a next run", /switched off/.test(row("weekly tally")) && !/next /.test(row("weekly tally")), row("weekly tally"));
  check("one whose agent is paused says that first", /its agent is paused/.test(row("pulse file")), row("pulse file"));
  check("and a watch that stopped says why", /stopped: Stopped looking/.test(row("what changed")), row("what changed"));

  const pausing = asked.length;
  [...rows.find((r) => r.textContent.includes("What moved overnight")).querySelectorAll("button")].find((b) => b.textContent === "Pause")?.click();
  await settle(300);
  const off = asked.slice(pausing).find((a) => a.name === "routine_off");
  check("a routine can be paused from the list, with the same switch as under Repeat", off?.args?.id === "talk-2" && off?.args?.off === true, JSON.stringify(off?.args || "nothing asked"));

  const again = [...panel.querySelectorAll(".one")].find((r) => r.textContent.includes("What moved overnight"));
  [...again.querySelectorAll("button")].find((b) => b.textContent === "Open")?.click();
  await settle(700);
  check("and Open goes to its conversation", panel.hidden && document.getElementById("talks").value === "talk-2", `hidden=${panel.hidden}, showing ${document.getElementById("talks").value}`);
  // Put back as it was: the groups after this one expect that routine live,
  // and the stand-in switches it off for real, as the app does.
  await window.__TAURI__.core.invoke("routine_off", { id: "talk-2", off: false });
  tell("repeats", { conversation: "talk-2", repeats: true });
  await settle(150);
  return found;
}

/**
 * Skills in the window: seen, run and forgotten, and offered behind /.
 *
 * They ran only when asked for in words, and nothing could delete one.
 */
export async function skillsInTheWindow() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  await openTalk("talk-2");
  document.getElementById("thread-name").click();
  await settle(300);
  const rows = () => [...document.querySelectorAll("#skills-list li")];
  check("its skills are listed under who it is", rows().length === 1 && /Morning brief/.test(rows()[0].textContent) && /1 step/.test(rows()[0].textContent), rows().map((li) => li.textContent).join(" | "));

  const forgetting = asked.length;
  [...rows()[0].querySelectorAll("button")].find((b) => b.textContent === "Forget")?.click();
  await settle(250);
  const gone = asked.slice(forgetting).find((a) => a.name === "forget_skill");
  check("forgetting one asks the app to take it back", gone?.args?.name === "Morning brief", JSON.stringify(gone?.args || "nothing asked"));

  const running = asked.length;
  [...rows()[0].querySelectorAll("button")].find((b) => b.textContent === "Run")?.click();
  await settle(600);
  const ran = asked.slice(running).find((a) => a.name === "run_a_skill");
  check("running one asks the app to run it", ran?.args?.agent === "agent-bitcoin" && ran?.args?.name === "Morning brief", JSON.stringify(ran?.args || "nothing asked"));
  const picker = document.getElementById("talks");
  check("and goes to the conversation it runs in", picker.selectedOptions[0]?.textContent.startsWith("Skill: Morning brief"), picker.selectedOptions[0]?.textContent || "nothing chosen");

  // Behind /, as it is typed.
  await openTalk("talk-2");
  const box = document.getElementById("what");
  box.value = "/mor";
  box.dispatchEvent(new Event("input"));
  await settle(250);
  const slash = document.getElementById("slash");
  check("typing / offers its skills", !slash.hidden && /Morning brief/.test(slash.textContent), `hidden=${slash.hidden}: ${slash.textContent}`);
  slash.querySelector("li")?.click();
  await settle(100);
  check("and choosing one puts its name in the box", box.value === "/Morning brief ", JSON.stringify(box.value));

  box.value = "/Morning brief only bitcoin";
  const sending = asked.length;
  document.getElementById("composer").requestSubmit();
  await settle(600);
  const slashed = asked.slice(sending).find((a) => a.name === "run_a_skill");
  check(
    "a line naming a skill runs it, with the rest as what to do differently",
    slashed?.args?.name === "Morning brief" && slashed?.args?.differently === "only bitcoin" && !asked.slice(sending).some((a) => a.name === "say"),
    JSON.stringify(asked.slice(sending).map((a) => [a.name, a.args])),
  );

  // Anything else starting with / is somebody's own words.
  await openTalk("talk-overnight");
  box.value = "/etc/hosts looks wrong";
  const plain = asked.length;
  document.getElementById("composer").requestSubmit();
  await settle(400);
  check(
    "and a line that only starts with / is sent as it is",
    asked.slice(plain).some((a) => a.name === "say" && a.args?.text === "/etc/hosts looks wrong") && !asked.slice(plain).some((a) => a.name === "run_a_skill"),
    JSON.stringify(asked.slice(plain).map((a) => a.name)),
  );
  tell("happened", { conversation: "talk-overnight", seq: 9861, kind: "done" });
  await settle(150);
  return found;
}

/**
 * A turn the window did not start.
 *
 * The clock, a watch, a goal, another agent and the terminal all start turns,
 * and none of them showed: no request, nothing saying Working, nothing to
 * stop, until the answer arrived or the conversation was opened again.
 */
export async function aTurnSomethingElseStarted() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  await openTalk("talk-outside");
  const open = document.querySelector('#threads li[aria-current="true"]')?.dataset.agent || "";
  const row = () =>
    [...document.querySelectorAll("#threads li")].find((li) => li.dataset.agent === open)?.querySelector(".last")?.textContent || "";
  const mine = (text) => [...document.querySelectorAll("#messages li.mine")].filter((li) => li.textContent.includes(text)).length;

  tell("noted", { conversation: "talk-outside", seq: 9901, kind: "mine", text: "What moved overnight?" });
  await settle(200);
  check("a routine's request appears in the open conversation as it is made", mine("What moved overnight?") === 1, `${mine("What moved overnight?")} on screen`);
  check("and the agent reads as Running now down the side", row() === "Running now", row());
  const palette = () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  };
  palette();
  await settle(200);
  const stop = [...document.querySelectorAll("#palette-list li")].find((li) => li.textContent.startsWith("Stop what it is doing"));
  check("and it can be stopped from here", stop && stop.getAttribute("aria-disabled") !== "true", stop?.outerHTML.slice(0, 90) || "no Stop in the palette");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await settle(100);
  tell("happened", { conversation: "talk-outside", seq: 9902, kind: "done" });
  await settle(150);
  check("until it ends", row() !== "Running now", row());

  // The window's own request comes back the same way, and is not shown twice.
  document.getElementById("what").value = "Anything else?";
  document.getElementById("composer").requestSubmit();
  await settle(250);
  tell("noted", { conversation: "talk-outside", seq: 9903, kind: "mine", text: "Anything else?" });
  await settle(200);
  check("a request the window sent itself is shown once", mine("Anything else?") === 1, `${mine("Anything else?")} on screen`);
  tell("happened", { conversation: "talk-outside", seq: 9904, kind: "done" });
  await settle(150);

  // Something the window did, said in the conversation while a turn runs, is
  // not the end of the turn, and a success is not said in red.
  tell("happened", { conversation: "talk-outside", seq: 9905, kind: "doing", what: "Reading the page", tool: "Bash", call: "x1" });
  await settle(150);
  palette();
  await settle(200);
  [...document.querySelectorAll("#palette-list li")].find((li) => li.textContent.startsWith("Export this conversation"))?.click();
  await settle(300);
  const saved = [...document.querySelectorAll("#messages li.ended")].pop();
  check("saving a copy says where, and not as a failure", /Saved to/.test(saved?.textContent || "") && !saved.classList.contains("failed"), `${saved?.className}: ${saved?.textContent}`);
  check("and the turn still going is still going", row() === "Running now", row());
  tell("happened", { conversation: "talk-outside", seq: 9906, kind: "done" });
  await settle(150);
  return found;
}

/**
 * A schedule an agent switched on or off itself, because it was asked to.
 *
 * The clock beside a conversation's name was set by the window's own buttons
 * and by reading the list again, so a schedule an agent set or stopped kept
 * the old clock until something else redrew it. And a schedule switched off
 * under Repeat had its clock back after a relaunch, because the list was read
 * without asking whether it was off.
 */
export async function anAgentThatStopsItsOwnSchedule() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  FIXTURE.conversations["agent-outside"].push(
    { id: "talk-routine-on", agent: "agent-outside", name: "Mornings", opened: true, runs_at: "daily 07:00" },
    { id: "talk-routine-off", agent: "agent-outside", name: "Evenings", opened: true, runs_at: "daily 19:00", routine_off: true },
  );
  await openTalk("talk-outside");
  const option = (id) => [...document.querySelectorAll("#talks option")].find((o) => o.value === id)?.textContent || "";
  check("a conversation whose schedule is on has the clock beside its name", option("talk-routine-on") === "Mornings ⏱", option("talk-routine-on"));
  check("and one whose schedule is switched off does not", option("talk-routine-off") === "Evenings", option("talk-routine-off"));

  tell("repeats", { conversation: "talk-outside", repeats: true });
  await settle(150);
  check("a schedule the agent set puts the clock on at once", option("talk-outside") === "First ⏱", option("talk-outside"));
  tell("repeats", { conversation: "talk-outside", repeats: false });
  await settle(150);
  check("and one it switched off takes it away again", option("talk-outside") === "First", option("talk-outside"));
  return found;
}

/**
 * The picker offers only models that answer, and the way to add one.
 *
 * A model on a server that was switched off was offered like any other, chosen,
 * and found dead two tries later in a red line that said it usually clears on
 * its own.
 */
export async function onlyModelsThatAnswer() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  const picker = document.getElementById("engine");
  const said = () => [...picker.options].map((o) => o.textContent).join(" / ");
  await openTalk("talk-1");
  await settle(200);
  // Errand's model is chosen in Settings now, once, for every teammate.
  document.getElementById("setup").click();
  await settle(250);
  check(
    "Errand's model is chosen in Settings, and the header has no model menu",
    picker.closest("#models") && !document.querySelector("header #engine, #title #engine"),
    picker.closest("#models") ? "in Settings" : "somewhere else",
  );

  // A second server nearby, switched off: the case that happened.
  const gone = {
    id: "o-gone",
    engine: "local",
    label: "Qwen3.8-27B - llama.cpp on 192.168.1.25",
    settings: '{"provider":"llamacpp","base_url":"http://192.168.1.25:8081","model":"Qwen3.8-27B"}',
    backend: null,
    sort: 3,
    mark: "local|http://192.168.1.25:8081|Qwen3.8-27B",
  };
  FIXTURE.offered.push(gone);
  tell("models_changed", {});
  await settle(100);

  const locals = FIXTURE.offered
    .filter((o) => o.engine === "local")
    .map((o) => JSON.parse(o.settings).base_url);
  const [up, ...down] = locals;
  FIXTURE.answering = Object.fromEntries([[up, true], ...down.map((at) => [at, false])]);
  const before = asked.length;
  window.dispatchEvent(new Event("focus"));
  await settle(400);
  check(
    "one knock unanswered is not enough to call a server down",
    [...picker.options].some((o) => o.value === gone.id),
    said(),
  );
  window.dispatchEvent(new Event("focus"));
  await settle(400);
  check(
    "coming back to the window knocks on the model servers nearby",
    asked.slice(before).some((a) => a.name === "answering" && a.args?.addresses?.length === locals.length),
    asked.slice(before).map((a) => a.name).join(",") || "asked nothing",
  );
  const shown = [...picker.options].filter((o) => !o.value.startsWith("__")).map((o) => o.value);
  const downIds = FIXTURE.offered
    .filter((o) => o.engine === "local" && down.includes(JSON.parse(o.settings).base_url))
    .map((o) => o.id);
  const selected = picker.value;
  check(
    "a model whose server does not answer is left out of the menu",
    down.length >= 1 && downIds.every((id) => !shown.includes(id) || id === selected),
    said(),
  );
  check(
    "and the menu says how many it left out",
    [...picker.options].some((o) => o.disabled && /^1 more not answering, so not shown$/.test(o.textContent)),
    said(),
  );
  const claudes = FIXTURE.offered.filter((o) => o.engine === "claude").map((o) => o.id);
  check("Claude and what answers are still offered", claudes.every((id) => shown.includes(id)), said());

  const add = [...picker.options].find((o) => o.value === "__add__");
  check("and the last choice is the way to add a model", add?.textContent === "Add a model…" && picker.options[picker.options.length - 1] === add, said());
  const wasOn = picker.value;
  picker.value = "__add__";
  picker.dispatchEvent(new Event("change"));
  await settle(300);
  check(
    "choosing it goes to where models are added, and changes no model",
    document.activeElement?.id === "hand-label" && picker.value === wasOn && !asked.slice(before).some((a) => a.name === "set_setting"),
    `focus on ${document.activeElement?.id || "nothing"}, menu on ${picker.value}`,
  );

  // Errand's model, when it stops answering: said on the menu itself.
  FIXTURE.answering = Object.fromEntries(locals.map((at) => [at, true]));
  window.dispatchEvent(new Event("focus"));
  await settle(400);
  picker.value = gone.id;
  picker.dispatchEvent(new Event("change"));
  await settle(300);
  check(
    "choosing a model makes it every teammate's, and says when that takes effect",
    asked.some((a) => a.name === "set_setting" && a.args?.key === "errand_model" && a.args?.value === gone.id) &&
      /Every teammate now works on/.test(document.getElementById("errand-model-says").textContent),
    document.getElementById("errand-model-says").textContent,
  );
  FIXTURE.answering[JSON.parse(gone.settings).base_url] = false;
  window.dispatchEvent(new Event("focus"));
  await settle(400);
  window.dispatchEvent(new Event("focus"));
  await settle(400);
  check(
    "Errand's model, when it stops answering, is marked on the menu",
    picker.classList.contains("quiet") &&
      picker.selectedOptions[0]?.textContent.endsWith("· not answering") &&
      /not answering/.test(picker.title),
    `${picker.className} / ${picker.selectedOptions[0]?.textContent} / ${picker.title}`,
  );
  picker.value = claudes[0];
  picker.dispatchEvent(new Event("change"));
  await settle(300);
  check("and not once it is one that answers", !picker.classList.contains("quiet") && picker.value === claudes[0], `${picker.className} / ${picker.value}`);

  // Everything back as the other checks expect it.
  FIXTURE.answering = Object.fromEntries(locals.map((at) => [at, true]));
  window.dispatchEvent(new Event("focus"));
  await settle(400);
  check(
    "and a server that answers again is offered again",
    FIXTURE.offered.every((o) => [...picker.options].some((p) => p.value === o.id)),
    said(),
  );
  delete FIXTURE.answering;
  FIXTURE.offered.splice(FIXTURE.offered.indexOf(gone), 1);
  tell("models_changed", {});
  await settle(100);
  document.getElementById("models-done").click();
  await settle(150);
  return found;
}

/**
 * A step that printed nothing is finished, and a turn that is over leaves
 * nothing spinning.
 *
 * A search that found nothing came back with an empty answer, which read the
 * same as no answer yet: two steps spun for ever under an errand that had
 * been stopped.
 */
export async function aStepThatPrintedNothing() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  FIXTURE.lines["talk-quiet"] = [
    { seq: 1, at: Date.now() - 60000, kind: "mine", text: "Look for the config files", call: null, tool: null, outcome: null },
    { seq: 2, at: Date.now() - 59000, kind: "doing", text: "Search for config files", call: "q-1", tool: "run_command", outcome: "" },
    { seq: 3, at: Date.now() - 58000, kind: "said", text: "There are none.", call: null, tool: null, outcome: null },
  ];
  FIXTURE.conversations["agent-outside"].push({ id: "talk-quiet", agent: "agent-outside", name: "Quiet", opened: true });
  await openTalk("talk-quiet");
  await settle(250);
  const box = document.getElementById("messages");
  const stepNamed = (words) => [...box.querySelectorAll(".doing")].find((d) => d.textContent.includes(words));
  const quiet = stepNamed("Search for config files");
  check(
    "a step that finished without printing anything is finished, and says so",
    quiet && !quiet.classList.contains("running") && /no output/.test(quiet.textContent),
    quiet ? `${quiet.className}: ${quiet.textContent}` : "no step",
  );

  tell("happened", { conversation: "talk-quiet", seq: 10, kind: "doing", what: "List the folder", tool: "run_command", call: "q-2" });
  tell("happened", { conversation: "talk-quiet", seq: 11, kind: "did", call: "q-2", outcome: "" });
  tell("happened", { conversation: "talk-quiet", seq: 12, kind: "doing", what: "Look once more", tool: "run_command", call: "q-3" });
  await settle(200);
  check("one still going spins", stepNamed("Look once more")?.classList.contains("running"), stepNamed("Look once more")?.className || "no step");
  check("and one that answered with nothing does not", stepNamed("List the folder") && !stepNamed("List the folder").classList.contains("running"), stepNamed("List the folder")?.className || "no step");
  tell("happened", { conversation: "talk-quiet", seq: 13, kind: "done" });
  await settle(200);
  const spinning = [...box.querySelectorAll(".doing.running")].map((d) => d.textContent);
  check("and once the turn is over, nothing in it is still spinning", spinning.length === 0, spinning.join(" | ") || "none");
  return found;
}

/**
 * An email an agent left to be checked: changed, then sent or thrown away.
 *
 * Nothing leaves until somebody presses Send, and what went stays readable
 * afterwards as what went, not as a form.
 */
export async function aDraftToCheck() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  await openTalk("talk-1");
  await settle(200);
  const box = document.getElementById("messages");
  const cardAt = (seq) => box.querySelector(`li.draft[data-seq="${seq}"]`);

  tell("drafted", {
    conversation: "talk-1",
    seq: 9800,
    draft: { to: "kim@mailbox.example", subject: "The news", body: "Line one\nLine two" },
  });
  await settle(200);
  let card = cardAt(9800);
  const value = (name) => card?.querySelector(`[name="${name}"]`)?.value;
  check(
    "a draft shows who it is to, its subject and its text, ready to change",
    card && value("to") === "kim@mailbox.example" && value("subject") === "The news" && value("body") === "Line one\nLine two",
    card ? `${value("to")} / ${value("subject")}` : "no card",
  );
  const subject = card.querySelector('[name="subject"]');
  subject.value = "The news, checked";
  subject.dispatchEvent(new Event("input"));
  const before = asked.length;
  [...card.querySelectorAll("button")].find((b) => b.textContent === "Send email").click();
  await settle(250);
  const sent = asked.slice(before).find((a) => a.name === "send_draft");
  check(
    "Send sends it as it stands after being changed, and nothing before",
    sent && sent.args?.subject === "The news, checked" && sent.args?.to === "kim@mailbox.example" && sent.args?.seq === 9800,
    JSON.stringify(sent?.args || "not sent"),
  );
  card = cardAt(9800);
  check(
    "and then it is what was sent, not a form",
    card?.classList.contains("done") && /^Sent/.test(card.textContent) && !card.querySelector("input, textarea"),
    card?.textContent.slice(0, 80) || "no card",
  );

  tell("drafted", {
    conversation: "talk-1",
    seq: 9801,
    draft: { to: "kim@mailbox.example", subject: "Second", body: "Not this one" },
  });
  await settle(200);
  const second = cardAt(9801);
  [...second.querySelectorAll("button")].find((b) => b.textContent === "Discard").click();
  await settle(200);
  check(
    "Discard throws it away unsent",
    asked.some((a) => a.name === "discard_draft" && a.args?.seq === 9801) && /Discarded, not sent/.test(cardAt(9801)?.textContent || ""),
    cardAt(9801)?.textContent.slice(0, 60) || "no card",
  );

  tell("drafted", {
    conversation: "talk-1",
    seq: 9802,
    draft: { to: "kim@mailbox.example", subject: "For later", body: "Finish on the phone" },
  });
  await settle(200);
  [...cardAt(9802).querySelectorAll("button")].find((b) => b.textContent === "Keep in Mail").click();
  await settle(250);
  check(
    "Keep in Mail puts it in Mail's Drafts, unsent",
    asked.some((a) => a.name === "draft_to_mail" && a.args?.seq === 9802) && /Kept in Mail's Drafts/.test(cardAt(9802)?.textContent || "") && !asked.some((a) => a.name === "send_draft" && a.args?.seq === 9802),
    cardAt(9802)?.textContent.slice(0, 60) || "no card",
  );

  // Read back later: one sent, one still waiting.
  FIXTURE.lines["talk-drafts"] = [
    { seq: 1, at: Date.now() - 60000, kind: "mine", text: "Draft me a mail to Kim", call: null, tool: null, outcome: null },
    { seq: 2, at: Date.now() - 50000, kind: "draft", text: JSON.stringify({ to: "kim@mailbox.example", subject: "Went", body: "Gone" }), call: "draft", tool: null, outcome: `sent|${Date.now() - 40000}` },
    { seq: 3, at: Date.now() - 30000, kind: "draft", text: JSON.stringify({ to: "kim@mailbox.example", subject: "Waiting", body: "Still here" }), call: "draft", tool: null, outcome: null },
  ];
  FIXTURE.conversations["agent-outside"].push({ id: "talk-drafts", agent: "agent-outside", name: "Drafts", opened: true });
  await openTalk("talk-drafts");
  await settle(250);
  check(
    "read back later, a sent draft is history and one still waiting is still a form",
    /^Sent/.test(cardAt(2)?.textContent || "") && !cardAt(2)?.querySelector("input") && cardAt(3)?.querySelector('[name="subject"]')?.value === "Waiting",
    `${cardAt(2)?.textContent.slice(0, 30)} | ${cardAt(3) ? "form" : "no card"}`,
  );
  return found;
}

/**
 * An answer read aloud on asking, the way a call reads every answer.
 *
 * Grok Bot answers with voice memos to play; here any answer can be heard,
 * whole, and stopped.
 */
export async function listeningToAnAnswer() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));
  FIXTURE.lines["talk-listen"] = [
    { seq: 1, at: Date.now() - 60000, kind: "mine", text: "What moved overnight?", call: null, tool: null, outcome: null },
    { seq: 2, at: Date.now() - 50000, kind: "said", text: "**BTC** is up 2% overnight.\n\n```\nprice: 81,200\n```\nNothing else moved.", call: null, tool: null, outcome: null },
  ];
  FIXTURE.conversations["agent-outside"].push({ id: "talk-listen", agent: "agent-outside", name: "Listen", opened: true });
  await openTalk("talk-listen");
  await settle(200);
  const box = document.getElementById("messages");
  const button = (label) =>
    [...(([...box.querySelectorAll("li.said")].pop())?.querySelectorAll(".did-with button") || [])].find((b) => b.textContent === label);
  check("an answer can be listened to", button("Listen"), "no Listen button");
  const before = window.__SAID__.length;
  button("Listen").click();
  const stop = button("Stop");
  check(
    "and while it is read, Stop is there without hovering",
    stop && stop.closest(".did-with").classList.contains("listening"),
    stop ? stop.closest(".did-with").className : "no Stop",
  );
  const heard = window.__SAID__.slice(before).join(" ");
  check(
    "what is read is the answer, without its markup or its code",
    /BTC is up 2% overnight/.test(heard) && !/\*\*|```|81,200/.test(heard),
    heard.slice(0, 120),
  );
  stop.click();
  check("and Stop stops it", window.__SAID__.slice(before).includes("<cut off>"), window.__SAID__.slice(before).join(" | ").slice(0, 120));
  await settle(100);
  check("after which it can be listened to again", button("Listen") && !button("Stop"), "still stopping");
  return found;
}

/**
 * Choosing what to allow, rather than guessing how to type it.
 *
 * "Let it" was an empty box with `curl` in it whatever "Using" said, so
 * somebody letting a teammate write into Downloads had nothing to tell them it
 * wanted the whole path. Now each kind comes with its choices filled in, says
 * what it would allow before anything is kept, and offers only what this
 * teammate would ever ask about.
 */
export async function choosingWhatToAllow() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const wasOn = document.getElementById("talks").value;
  await openTalk("talk-2");
  const panel = document.getElementById("granting");
  if (!panel.hidden) document.getElementById("granted").click();
  document.getElementById("granted").click();
  await new Promise((r) => setTimeout(r, 300));

  const using = document.getElementById("allow-tool");
  const letIt = document.getElementById("allow-choice");
  const typed = document.getElementById("allow-what");
  const means = document.getElementById("allow-means");
  const pick = async (select, value) => {
    select.value = value;
    select.dispatchEvent(new Event("change"));
    await new Promise((r) => setTimeout(r, 250));
  };
  const said = (select) => [...select.options].map((o) => o.textContent);

  check(
    "what it is comes first, in words",
    said(using).includes("running commands") && said(using).includes("writing files"),
    said(using).join(" | "),
  );
  check(
    "a teammate that asks and is not walled is not offered a folder, which would change nothing for it",
    !said(using).includes("a folder it may write in"),
    said(using).join(" | "),
  );

  await pick(using, "commands");
  const commands = said(letIt);
  check(
    "running commands comes with programs to choose",
    commands.includes("any curl command") && commands.includes("any git command"),
    commands.join(" | "),
  );
  check(
    "nothing is chosen until somebody chooses",
    letIt.value === "" && letIt.options[0]?.disabled,
    `value=${letIt.value}`,
  );
  check(
    "the whole of it is there, last, and said as the whole of it",
    commands[commands.length - 2] === "running any command at all",
    commands.join(" | "),
  );
  check("the box to type in waits until something else is wanted", typed.hidden, `hidden=${typed.hidden}`);

  await pick(letIt, "0");
  check(
    "choosing says what it would allow before anything is kept",
    /This would allow any curl command/.test(means.textContent),
    means.textContent,
  );

  await pick(letIt, "something-else");
  check(
    "something else opens a box that says what to type",
    !typed.hidden && /a program/.test(typed.placeholder),
    `hidden=${typed.hidden} "${typed.placeholder}"`,
  );

  // Never asking: nothing to allow but somewhere else to write.
  const asks = document.getElementById("asks");
  const postureWas = asks.value;
  asks.value = "auto";
  asks.dispatchEvent(new Event("change"));
  await new Promise((r) => setTimeout(r, 350));
  check(
    "a teammate that never asks is offered only somewhere else to write",
    said(using).length === 1 && using.value === "folder",
    said(using).join(" | "),
  );
  const fewer = document.getElementById("allow-fewer");
  check("and is told why the rest are gone", !fewer.hidden && /never asks/.test(fewer.textContent), fewer.textContent);
  const places = said(letIt);
  check(
    "the folders are there to choose, whole",
    places.some((p) => p.includes("/Users/me/Downloads")) && places.some((p) => p.includes("/Volumes/Archive")),
    places.join(" | "),
  );
  check(
    "and a folder is never offered as all of them at once",
    !places.some((p) => /anywhere$|any file/.test(p)),
    places.join(" | "),
  );

  const downloads = [...letIt.options].find((o) => o.textContent.includes("/Users/me/Downloads"));
  await pick(letIt, downloads?.value ?? "");
  check(
    "choosing Downloads says it may then write anywhere inside it",
    /writing anywhere inside \/Users\/me\/Downloads/.test(means.textContent),
    means.textContent,
  );
  const before = asked.length;
  document.getElementById("allow-ahead").dispatchEvent(new Event("submit"));
  await new Promise((r) => setTimeout(r, 350));
  const sent = asked.slice(before).find((a) => a.name === "allow_in_advance");
  check(
    "allowing it sends the folder, whole, as a folder",
    sent?.args?.tool === "folder" && sent?.args?.rule === "/Users/me/Downloads",
    JSON.stringify(sent?.args),
  );

  // Any other folder comes from the Mac's own chooser, and is never typed.
  const last = letIt.options[letIt.options.length - 1];
  check(
    "the last choice for a place opens the folder chooser rather than a box to type in",
    /choose another folder/.test(last?.textContent || "") && !places.some((p) => /something else/.test(p)),
    last?.textContent,
  );
  const chooserAsked = asked.length;
  await pick(letIt, last.value);
  await new Promise((r) => setTimeout(r, 200));
  check(
    "choosing another folder asks the Mac's own chooser",
    asked.slice(chooserAsked).some((a) => a.name === "choose_a_folder"),
    asked.slice(chooserAsked).map((a) => a.name).join(", "),
  );
  check(
    "and the folder it gives is added to the list, chosen, whole",
    letIt.selectedOptions[0]?.textContent === "Clips (/Users/me/Projects/Clips)",
    letIt.selectedOptions[0]?.textContent,
  );
  check(
    "and it says what that would allow before anything is kept",
    /writing anywhere inside \/Users\/me\/Projects\/Clips/.test(means.textContent),
    means.textContent,
  );
  check("and nothing had to be typed", typed.hidden, `hidden=${typed.hidden}`);
  // Cancelled, it goes back to what was chosen before.
  FIXTURE.folderChosen = null;
  await pick(letIt, letIt.options[letIt.options.length - 1].value);
  await new Promise((r) => setTimeout(r, 200));
  delete FIXTURE.folderChosen;
  check(
    "cancelling the chooser keeps the folder chosen before",
    letIt.selectedOptions[0]?.textContent === "Clips (/Users/me/Projects/Clips)",
    letIt.selectedOptions[0]?.textContent,
  );

  asks.value = postureWas;
  asks.dispatchEvent(new Event("change"));
  await new Promise((r) => setTimeout(r, 300));
  document.getElementById("granted").click();
  await openTalk(wasOn);
  return found;
}

/**
 * The next thing to say, greyed in the box, which Tab takes, as in Claude.
 *
 * The box only repeated the teammate's question, so there was nothing to take.
 */
export async function takingTheSuggestion() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const where = document.getElementById("talks").value;
  const box = document.getElementById("what");
  const hint = document.getElementById("tab-hint");
  const tab = () => {
    const e = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
    box.dispatchEvent(e);
    return e.defaultPrevented;
  };
  box.value = "";

  tell("happened", { conversation: where, seq: 7201, kind: "done" });
  await new Promise((r) => setTimeout(r, 250));
  check(
    "when a turn ends, the next thing to say is greyed in the empty box",
    box.placeholder === "Yes, go ahead with the bigger disk.",
    box.placeholder,
  );
  check("with the key that takes it beside the box", !hint.hidden, `hidden=${hint.hidden}`);

  const took = tab();
  check(
    "Tab puts it in the box, ready to change or send",
    took && box.value === "Yes, go ahead with the bigger disk.",
    `${took} "${box.value}"`,
  );
  check("and the key goes once it is taken", hint.hidden, `hidden=${hint.hidden}`);
  check("so Tab is left alone after that", !tab(), "Tab was taken again");

  // Typing is saying something else, and the suggestion makes way.
  box.value = "";
  tell("happened", { conversation: where, seq: 7202, kind: "done" });
  await new Promise((r) => setTimeout(r, 250));
  box.value = "Actually, wait";
  box.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 50));
  check(
    "typing makes it go",
    hint.hidden && box.placeholder !== "Yes, go ahead with the bigger disk.",
    `hidden=${hint.hidden} "${box.placeholder}"`,
  );
  check("and Tab no longer takes anything", !tab(), "Tab was taken");

  // A teammate with no suggestion, like one on Claude Code, offers nothing.
  box.value = "";
  box.dispatchEvent(new Event("input"));
  FIXTURE.suggestion = null;
  tell("happened", { conversation: where, seq: 7203, kind: "done" });
  await new Promise((r) => setTimeout(r, 250));
  delete FIXTURE.suggestion;
  check("with nothing to suggest, nothing is shown", hint.hidden, `hidden=${hint.hidden}`);
  check("and Tab is left to do what it always did", !tab(), "Tab was taken");
  box.value = "";
  return found;
}

export async function aKeyTheAgentForgot() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const loads = () => asked.filter((a) => a.name === "load_ssh_key").length;

  // Settings: what the key agent holds, the button, and the switch.
  document.getElementById("setup").click();
  await wait(250);
  const card = document.getElementById("ssh-key-card");
  const says = document.getElementById("ssh-key-says");
  const load = document.getElementById("ssh-key-load");
  const atStart = document.getElementById("ssh-key-at-start");
  check(
    "Settings says teammates use the key through the agent and never read it",
    /never hands it over/.test(card?.textContent || "") && /Keychain/.test(card?.textContent || ""),
    (card?.textContent || "missing").slice(0, 160),
  );
  check(
    "an empty agent is said, with the key there is to load",
    /Not loaded/.test(says.textContent) && /id_ed25519/.test(says.textContent) && says.dataset.wrong === "true",
    says.textContent,
  );
  check("the switch starts off, because nobody asked for it yet", !atStart.checked, `checked=${atStart.checked}`);
  check(
    "its words are the ones agreed",
    /Teammates may use my SSH keys \(through the key agent, never by reading them\)/.test(atStart.closest("label")?.textContent || ""),
    atStart.closest("label")?.textContent,
  );

  const before = loads();
  load.click();
  await wait(200);
  check("Load my SSH key asks the app to load it", loads() === before + 1, `${loads() - before} loads`);
  check("and says what was loaded", /Loaded id_ed25519/.test(says.textContent), says.textContent);

  document.getElementById("models-done").click();
  document.getElementById("setup").click();
  await wait(250);
  check(
    "opened again, it reads the agent rather than remembering",
    /Loaded\. The key agent holds a key/.test(says.textContent) && says.dataset.wrong === "false",
    says.textContent,
  );

  atStart.checked = true;
  atStart.dispatchEvent(new Event("change"));
  await wait(250);
  check(
    "turning the switch on keeps it",
    asked.some((a) => a.name === "set_setting" && a.args?.key === "ssh_keys_at_start" && a.args?.value === "on"),
    JSON.stringify(asked.filter((a) => a.name === "set_setting").slice(-1)),
  );
  check(
    "and says it loads the key whenever Errand starts",
    /whenever it starts/.test(document.getElementById("ssh-key-at-start-says").textContent),
    document.getElementById("ssh-key-at-start-says").textContent,
  );
  check("with the key already loaded, nothing is loaded twice", loads() === before + 1, `${loads() - before} loads`);

  atStart.checked = false;
  atStart.dispatchEvent(new Event("change"));
  await wait(200);
  check(
    "and off again",
    asked.some((a) => a.name === "set_setting" && a.args?.key === "ssh_keys_at_start" && a.args?.value === "off"),
    JSON.stringify(asked.filter((a) => a.name === "set_setting").slice(-1)),
  );

  // On, with the agent empty: teammates may use it, so it is loaded now.
  FIXTURE.sshKey = { ...FIXTURE.sshKey, holds: 0 };
  atStart.checked = true;
  atStart.dispatchEvent(new Event("change"));
  await wait(300);
  check("turned on with the agent empty, it is loaded straight away", loads() === before + 2, `${loads() - before} loads`);
  FIXTURE.settings.ssh_keys_at_start = "off";
  document.getElementById("models-done").click();
  document.getElementById("setup").click();
  await wait(250);
  check(
    "opened again, the switch says nothing left over from the last change",
    !atStart.checked && document.getElementById("ssh-key-at-start-says").textContent === "",
    `checked=${atStart.checked} "${document.getElementById("ssh-key-at-start-says").textContent}"`,
  );
  document.getElementById("models-done").click();

  // Above the box: a teammate's SSH refused the key.
  const note = document.getElementById("key-note");
  const noteSays = document.getElementById("key-note-says");
  const noteLoad = document.getElementById("key-note-load");
  const noteClose = document.getElementById("key-note-close");
  check("nothing is said about a key until one is refused", note.hidden, `hidden=${note.hidden}`);
  tell("ssh_key_needed", "talk-1");
  await wait(50);
  check("a refused key is said above the box", !note.hidden, `hidden=${note.hidden}`);
  check(
    "as a key not loaded, and not as something Allowed fixes",
    /isn't loaded/.test(note.textContent) && /nothing under\s+Allowed fixes this/.test(note.textContent),
    note.textContent.replace(/\s+/g, " ").trim(),
  );
  noteLoad.click();
  await wait(250);
  check("its button loads the key", loads() === before + 3, `${loads() - before} loads`);
  check(
    "and then says to ask again, and how to keep it loaded",
    /Loaded id_ed25519\. Ask again/.test(noteSays.textContent) && /whenever it starts/.test(noteSays.textContent),
    noteSays.textContent,
  );
  check("with nothing left to press but Done", noteLoad.hidden && noteClose.textContent === "Done", `${noteLoad.hidden} ${noteClose.textContent}`);
  noteClose.click();
  check("which puts it away", note.hidden, `hidden=${note.hidden}`);

  // A passphrase window closed: said, and the button stays.
  FIXTURE.sshLoad = { added: [], holds: 0, why_not: "No key was loaded: the passphrase window was closed, or left for five minutes without an answer." };
  tell("ssh_key_needed", "talk-1");
  await wait(50);
  noteLoad.click();
  await wait(250);
  check(
    "a key that was not loaded says why, in the colour of something wrong",
    /passphrase window was closed/.test(noteSays.textContent) && noteSays.dataset.wrong === "true",
    noteSays.textContent,
  );
  check("and the button is still there to try again", !noteLoad.hidden, `hidden=${noteLoad.hidden}`);
  noteClose.click();
  tell("ssh_key_needed", "talk-1");
  await wait(50);
  check("Not now is not asked again the next time a teammate tries", note.hidden, `hidden=${note.hidden}`);

  FIXTURE.sshLoad = { added: ["id_ed25519"], holds: 1, why_not: null };
  FIXTURE.sshKey = { agent: true, holds: 0, keys: ["id_ed25519"] };
  delete FIXTURE.settings.ssh_keys_at_start;
  return found;
}

/**
 * When each thing was said, under it.
 *
 * Bell Ahead's calendar answers read as one undated column, so which of them
 * had failed, and when, could only be worked out from the store. A divider is
 * drawn only where the day changes and never above the first line, so each
 * stamp has to make sense read on its own.
 */
export async function whenEachThingWasSaid() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const midnight = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const daysAgo = (iso) => Math.round((midnight(new Date()) - midnight(new Date(iso))) / 86400000);

  // Three days of lines.
  await openTalk("talk-overnight");
  const lines = [...document.querySelectorAll("#messages li.said, #messages li.mine")];
  const stamps = lines.map((li) => li.querySelector("time.at"));
  check(
    "every message says when it was said",
    lines.length >= 3 && stamps.every(Boolean),
    `${stamps.filter(Boolean).length} of ${lines.length}`,
  );
  const read = stamps.filter(Boolean).map((t) => ({ text: t.textContent, ago: daysAgo(t.dateTime), title: t.title }));
  const on = (pick) => read.filter(pick);
  check(
    "today's say the time alone",
    on((r) => r.ago === 0).length > 0 && on((r) => r.ago === 0).every((r) => /\d:\d\d/.test(r.text) && !/Yesterday|,/.test(r.text)),
    JSON.stringify(read),
  );
  check(
    "yesterday's say so",
    on((r) => r.ago === 1).length > 0 && on((r) => r.ago === 1).every((r) => r.text.startsWith("Yesterday, ")),
    JSON.stringify(read),
  );
  check(
    "older ones say their date as well, so a stamp is never ambiguous on its own",
    on((r) => r.ago >= 2).length > 0 && on((r) => r.ago >= 2).every((r) => r.text.includes(",") && !/Yesterday/.test(r.text)),
    JSON.stringify(read),
  );
  check(
    "and the whole date is there on hover",
    read.every((r) => /\d{4}/.test(r.title)),
    JSON.stringify(read.map((r) => r.title)),
  );

  // In view without hovering, unlike the buttons beside it.
  const first = stamps.find(Boolean);
  const row = first?.closest(".did-with");
  const button = row?.querySelector("button");
  check(
    "the time is in view without hovering",
    first && getComputedStyle(first).opacity === "1" && getComputedStyle(row).opacity === "1",
    first ? `${getComputedStyle(first).opacity}/${getComputedStyle(row).opacity}` : "no stamp",
  );
  check(
    "while the buttons beside it still wait for a hover",
    button && getComputedStyle(button).opacity === "0",
    button ? getComputedStyle(button).opacity : "no button",
  );
  // On the bubble's outer edge: first under an answer, last under your words.
  const saidRow = document.querySelector("#messages li.said .did-with");
  const mineRow = document.querySelector("#messages li.mine .did-with");
  check(
    "it sits on the outer edge: first under an answer, last under your own words",
    saidRow?.firstElementChild?.matches("time.at") && mineRow?.lastElementChild?.matches("time.at"),
    `${saidRow?.firstElementChild?.tagName} / ${mineRow?.lastElementChild?.tagName}`,
  );

  // A turn that ended part way says when.
  await openTalk("talk-cut-off");
  const ended = document.querySelector("#messages li.ended time.at");
  check("a turn that ended part way says when", !!ended && /\d:\d\d/.test(ended.textContent), ended?.textContent);

  // And something that has only just arrived, before it is written down.
  const where = document.getElementById("talks").value;
  tell("happened", { conversation: where, seq: 9901, kind: "said", text: "Recycling goes out at 17:30.", settled: true });
  await wait(150);
  const live = [...document.querySelectorAll("#messages li.said")].pop();
  const liveStamp = live?.querySelector("time.at")?.textContent || "";
  check(
    "an answer that has only just arrived says when too",
    live?.textContent.includes("Recycling goes out") && /\d:\d\d/.test(liveStamp) && !/,/.test(liveStamp),
    liveStamp,
  );
  tell("happened", { conversation: where, seq: 9902, kind: "done" });
  await wait(100);
  return found;
}

/**
 * What a teammate does on its own, seen from the list.
 *
 * It was a grey mark the size of a letter beside the name, and a routine
 * switched off by mistake went four hours without anybody seeing it.
 */
export async function whatRunsOnItsOwnStandsOut() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  // From Bitcoin Desk's daily routine live, whatever an earlier group left.
  await window.__TAURI__.core.invoke("routine_off", { id: "talk-2", off: false });
  tell("repeats", { conversation: "talk-2", repeats: true });
  await wait(150);
  const list = document.getElementById("threads");
  const rowOf = (agent) => list.querySelector(`li[data-agent="${agent}"]`);
  const badgeOf = (agent) => rowOf(agent)?.querySelector(".on-its-own");
  // From nothing working, whatever earlier checks left going: a badge that
  // says "running" is right while anything of its teammate's is.
  ["talk-1", "talk-2", "talk-3", "talk-4", "talk-overnight", "talk-cut-off"].forEach((id, n) =>
    tell("happened", { conversation: id, seq: 9940 + n, kind: "done" }),
  );
  await wait(200);
  const accent = (() => {
    const probe = document.createElement("span");
    probe.style.color = "var(--accent)";
    document.body.append(probe);
    const seen = getComputedStyle(probe).color;
    probe.remove();
    return seen;
  })();

  const desk = badgeOf("agent-bitcoin");
  check(
    "a teammate with something due has a badge on its mark",
    desk && desk.dataset.shows === "repeat" && !desk.classList.contains("idle") && !desk.classList.contains("now"),
    desk ? `${desk.className} ${desk.dataset.shows}` : "no badge",
  );
  check(
    "filled in the accent, so it can be seen across the list",
    desk && getComputedStyle(desk).backgroundColor === accent,
    desk ? `${getComputedStyle(desk).backgroundColor}, accent ${accent}` : "no badge",
  );
  check(
    "saying in its tooltip when it next runs, and what",
    /Runs on its own, next /.test(desk?.title || "") && /Repeats daily 07:00/.test(desk?.title || "") && /What moved overnight/.test(desk?.title || ""),
    desk?.title,
  );
  const mark = rowOf("agent-bitcoin")?.querySelector(".tile")?.getBoundingClientRect();
  const badge = desk?.getBoundingClientRect();
  const row = rowOf("agent-bitcoin")?.getBoundingClientRect();
  check(
    "in the corner of the mark, and inside the row",
    mark && badge && row &&
      badge.right > mark.right && badge.bottom > mark.bottom &&
      badge.left > mark.left + mark.width / 2 && badge.top > mark.top + mark.height / 2 &&
      badge.bottom <= row.bottom && badge.right <= row.right,
    mark && badge && row ? `mark ${Math.round(mark.right)},${Math.round(mark.bottom)}; badge ${Math.round(badge.left)}-${Math.round(badge.right)},${Math.round(badge.top)}-${Math.round(badge.bottom)}; row bottom ${Math.round(row.bottom)}` : "missing",
  );
  check(
    "and the name beside it keeps its room",
    !rowOf("agent-bitcoin")?.querySelector(".name .on-its-own"),
    "badge on the mark, not in the name",
  );

  const pulse = badgeOf("agent-unnamed");
  check(
    "a teammate whose routine is paused has a quiet outlined badge saying so",
    pulse?.classList.contains("idle") && /^Paused/.test(pulse.title) && getComputedStyle(pulse).backgroundColor !== accent,
    pulse ? `${pulse.className} "${pulse.title.split("\n")[0]}"` : "no badge",
  );
  const none = [...list.querySelectorAll("li[data-agent]")].find(
    (li) => !FIXTURE.standing.some((s) => s.agent === li.dataset.agent) && !/Working/.test(li.textContent),
  );
  check("a teammate with nothing running and nothing of its own to run has no badge", none && !none.querySelector(".on-its-own"), none?.dataset.agent);

  // Running, while it works, and back to what is scheduled after.
  tell("happened", { conversation: "talk-2", kind: "said", settled: false, text: "Looking at the overnight moves" });
  await wait(150);
  const going = badgeOf("agent-bitcoin");
  check(
    "while it works, the badge says it is running",
    going?.classList.contains("now") && going.dataset.shows === "running" && /^Running now/.test(going.title) && getComputedStyle(going).backgroundColor === accent,
    going ? `${going.className} ${going.dataset.shows} "${going.title.split("\n")[0]}"` : "no badge",
  );
  tell("happened", { conversation: "talk-2", seq: 9950, kind: "done" });
  await wait(150);
  const after = badgeOf("agent-bitcoin");
  check(
    "and when it is done, it shows what is scheduled again",
    after && !after.classList.contains("now") && after.dataset.shows === "repeat",
    after ? `${after.className} ${after.dataset.shows}` : "no badge",
  );

  // Finished means nothing more runs: the task whose routine was the only live
  // one is finished, and the badge goes quiet; reopened, it is live again.
  await openTalk("talk-2");
  const done = document.getElementById("task-done");
  done.click();
  // What runs is read again on a timer of nothing, which a page the browser is
  // not drawing can hold back for most of a second.
  await wait(700);
  const finished = badgeOf("agent-bitcoin");
  check(
    "finishing the task that runs on its own switches it off, and the badge goes quiet",
    asked.some((a) => a.name === "finish_task" && a.args?.id === "talk-2" && a.args?.finished === true) &&
      finished?.classList.contains("idle"),
    finished ? `${finished.className} "${finished.title.split("\n")[0]}"` : "no badge",
  );
  done.click();
  await wait(700);
  const reopened = badgeOf("agent-bitcoin");
  check(
    "and reopening it switches it back on",
    asked.some((a) => a.name === "finish_task" && a.args?.id === "talk-2" && a.args?.finished === false) &&
      reopened && !reopened.classList.contains("idle") && reopened.dataset.shows === "repeat",
    reopened ? `${reopened.className} ${reopened.dataset.shows}` : "no badge",
  );
  return found;
}

/**
 * A task says what it is, at its top.
 *
 * Finding out what a task did, when it would run and whether it was running
 * meant reading back through it, or opening Repeat; a new task was in three
 * menus; and a message sent while it worked waited without a word.
 */
export async function aTaskSaysWhatItIs() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  // From Bitcoin Desk's daily routine live, whatever an earlier group left.
  await window.__TAURI__.core.invoke("routine_off", { id: "talk-2", off: false });
  tell("repeats", { conversation: "talk-2", repeats: true });
  await wait(150);
  const card = document.getElementById("task-card");
  const chip = () => card.querySelector(".state");
  const buttons = () => [...card.querySelectorAll(".actions button")];
  const button = (label) => buttons().find((b) => b.textContent === label);
  const fact = (label) => {
    const at = [...card.querySelectorAll("dt")].findIndex((d) => d.textContent === label);
    const dd = card.querySelectorAll("dd")[at];
    return at < 0 ? "" : (dd.querySelector(".words") || dd).textContent;
  };
  ["talk-1", "talk-2", "talk-3", "talk-4", "talk-overnight", "talk-cut-off"].forEach((id, n) =>
    tell("happened", { conversation: id, seq: 9960 + n, kind: "done" }),
  );
  await wait(200);

  await openTalk("talk-2");
  check(
    "a task says at its top where it stands: due to run, and when",
    !card.hidden && chip()?.dataset.kind === "scheduled" && /^Next /.test(chip()?.textContent || ""),
    `${card.hidden ? "hidden" : ""} ${chip()?.dataset.kind} "${chip()?.textContent}"`,
  );
  check("what it does", fact("Does") === "What moved overnight", fact("Does"));
  check("when it repeats, and next", /daily 07:00/.test(fact("Runs")) && /next /.test(fact("Runs")), fact("Runs"));
  check("what came of it last, with when", /\d:\d\d/.test(fact("Last")) || fact("Last") === "Nothing yet", fact("Last"));
  check("with Pause and Run now on it", button("Pause") && button("Run now"), buttons().map((b) => b.textContent).join(", "));

  button("Pause").click();
  await wait(300);
  check(
    "Pause holds its schedule, and the task says it is paused",
    asked.some((a) => a.name === "routine_off" && a.args?.id === "talk-2" && a.args?.off === true) &&
      chip()?.dataset.kind === "paused" && button("Resume"),
    `${chip()?.textContent} / ${buttons().map((b) => b.textContent).join(", ")}`,
  );
  button("Resume").click();
  await wait(300);
  check(
    "and Resume starts it again",
    asked.some((a) => a.name === "routine_off" && a.args?.id === "talk-2" && a.args?.off === false) &&
      chip()?.dataset.kind === "scheduled",
    chip()?.textContent,
  );
  const before = asked.length;
  button("Run now").click();
  await wait(250);
  check(
    "Run now does it now, in this task",
    asked.slice(before).some((a) => a.name === "say" && a.args?.id === "talk-2" && a.args?.text === "What moved overnight"),
    JSON.stringify(asked.slice(before).map((a) => a.name)),
  );

  // Working: said on the card and above the box, with the way to stop it.
  tell("happened", { conversation: "talk-2", kind: "said", settled: false, text: "Looking at the overnight moves" });
  await wait(200);
  const note = document.getElementById("running-note");
  check(
    "while it works, it says it is running, with Stop",
    chip()?.dataset.kind === "running" && /Running now/.test(chip()?.textContent || "") && button("Stop"),
    `${chip()?.dataset.kind} / ${buttons().map((b) => b.textContent).join(", ")}`,
  );
  check(
    "and above the box, that what is sent waits for the run to end",
    !note.hidden && /read when this run ends/.test(note.textContent),
    `hidden=${note.hidden} "${note.textContent.trim().replace(/\s+/g, " ")}"`,
  );
  const stopping = asked.length;
  document.getElementById("running-stop").click();
  await wait(300);
  check(
    "its Stop stops the run, and the note goes",
    asked.slice(stopping).some((a) => a.name === "stop" && a.args?.id === "talk-2") && note.hidden && chip()?.dataset.kind !== "running",
    `${JSON.stringify(asked.slice(stopping).map((a) => a.name))} hidden=${note.hidden} ${chip()?.dataset.kind}`,
  );

  // Answered, finished, and needing you.
  await openTalk("talk-overnight");
  check("a task with nothing of its own to run, not done, is idle until asked", chip()?.dataset.kind === "idle" && fact("Runs") === "Only when you ask", `${chip()?.dataset.kind} / ${fact("Runs")}`);
  document.getElementById("task-done").click();
  await wait(250);
  check("marked finished, it says finished", chip()?.dataset.kind === "finished" && /Finished/.test(chip()?.textContent || ""), `${chip()?.dataset.kind} "${chip()?.textContent}"`);
  document.getElementById("task-done").click();
  await wait(250);
  tell("happened", {
    conversation: "talk-overnight",
    seq: 9970,
    kind: "needs_you",
    asking: "Delete last night's notes",
    detail: "rm notes-old.md",
    tool: "Bash",
    call: "card-q",
    step: "card-q",
    can_remember: false,
    rule: "",
    allows: "",
  });
  await wait(250);
  check("a task stopped on a question says it needs you", chip()?.dataset.kind === "needs-you", `${chip()?.dataset.kind} "${chip()?.textContent}"`);

  // Another task, in plain sight: under the teammate, in the list down the side.
  const newTask = document.querySelector('#threads li[data-agent="agent-bitcoin"] .task-new .add');
  check("a teammate's tasks end with New task in plain sight", newTask && newTask.offsetWidth > 0, newTask ? `width=${newTask.offsetWidth}` : "missing");
  const starting = asked.length;
  newTask.click();
  await wait(300);
  check(
    "and it starts a new task for that teammate and opens it",
    asked.slice(starting).some((a) => a.name === "start_conversation" && a.args?.agent === "agent-bitcoin") &&
      (document.getElementById("talks").selectedOptions[0]?.textContent || "").startsWith("New task") &&
      chip()?.dataset.kind === "idle" && fact("Does") === "Nothing asked yet",
    `${document.getElementById("talks").selectedOptions[0]?.textContent} ${chip()?.dataset.kind} ${fact("Does")}`,
  );

  // A second schedule the app set up as a task of its own is there at once.
  FIXTURE.conversations["agent-bitcoin"].push({
    id: "talk-overdue", agent: "agent-bitcoin", name: "The overdue list", opened: false,
    runs_at: "weekly mon 09:00", runs_what: "The overdue list", routine_off: false,
  });
  FIXTURE.standing.push({
    conversation: "talk-overdue", agent: "agent-bitcoin", who: "Bitcoin Desk", name: "The overdue list",
    kind: "routine", at: "weekly mon 09:00", what: "The overdue list", due: Date.now() + 3 * 86400000,
    off: false, stopped: null, paused: false,
  });
  tell("task_made", { conversation: "talk-overdue", agent: "agent-bitcoin", name: "The overdue list" });
  await wait(400);
  check(
    "a task the app made for a second schedule is in the task menu at once, with its clock",
    [...document.getElementById("talks").options].some((o) => o.textContent.startsWith("The overdue list") && o.textContent.includes("⏱")),
    [...document.getElementById("talks").options].map((o) => o.textContent).join(" | "),
  );
  FIXTURE.conversations["agent-bitcoin"] = FIXTURE.conversations["agent-bitcoin"].filter((c) => c.id !== "talk-overdue");
  FIXTURE.standing = FIXTURE.standing.filter((s) => s.conversation !== "talk-overdue");
  return found;
}

/**
 * A teammate's tasks, down the side, and Now.
 *
 * Its tasks were a menu in the header, with New task at its bottom, and what
 * a teammate had going could not be seen without opening that menu. What
 * needed somebody across every teammate was behind four squares nobody read.
 */
export async function tasksDownTheSideAndNow() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  await window.__TAURI__.core.invoke("routine_off", { id: "talk-2", off: false });
  tell("repeats", { conversation: "talk-2", repeats: true });
  ["talk-1", "talk-2", "talk-3", "talk-4", "talk-overnight", "talk-cut-off"].forEach((id, n) =>
    tell("happened", { conversation: id, seq: 9980 + n, kind: "done" }),
  );
  await wait(200);
  await openTalk("talk-2");
  const block = () => document.querySelector('#threads li[data-agent="agent-bitcoin"] .tasks-of');
  const rows = () => [...(block()?.querySelectorAll(".task") || [])];
  const menu = [...document.getElementById("talks").options].filter((o) => !["+", "room"].includes(o.value));
  check(
    "the teammate on screen has its tasks listed under it, one row each, as the menu had them",
    block() && rows().length === menu.length && rows().length > 1,
    `${rows().length} rows, ${menu.length} in the menu`,
  );
  check(
    "inside its own row, so the list is still one row per teammate",
    document.querySelectorAll("#threads > li .tasks-of").length === 1 && !document.querySelector("#threads > div"),
    `${document.querySelectorAll("#threads .tasks-of").length} lists`,
  );
  const current = rows().find((r) => r.getAttribute("aria-current") === "true");
  check("the task on screen is marked", current?.dataset.task === "talk-2", current?.dataset.task || "none");
  check(
    "each says where it stands, a scheduled one when it next runs",
    rows().every((r) => r.querySelector(".task-mark svg")) &&
      current?.dataset.kind === "scheduled" && /\d/.test(current.querySelector(".task-when")?.textContent || ""),
    `${current?.dataset.kind} "${current?.querySelector(".task-when")?.textContent}"`,
  );
  const order = ["needs-you", "running", "stopped", "scheduled", "idle", "paused", "finished"];
  const kinds = rows().map((r) => r.dataset.kind);
  check(
    "in the order they are looked at: what needs you first, what is finished last",
    kinds.every((k, i) => i === 0 || order.indexOf(kinds[i - 1]) <= order.indexOf(k)),
    kinds.join(", "),
  );
  check(
    "the header no longer has a task menu or New task of its own",
    getComputedStyle(document.getElementById("talks")).display === "none" && !document.getElementById("new-task"),
    `${getComputedStyle(document.getElementById("talks")).display} ${document.getElementById("new-task") ? "New task is still there" : "no New task"}`,
  );

  const other = rows().find((r) => r.dataset.task === "talk-overnight");
  other?.querySelector("button").click();
  await wait(500);
  check(
    "a task in the list opens with a click, and becomes the one marked",
    document.getElementById("talks").value === "talk-overnight" &&
      rows().find((r) => r.getAttribute("aria-current") === "true")?.dataset.task === "talk-overnight",
    `${document.getElementById("talks").value}`,
  );
  check(
    "and its teammate stays open around it",
    document.querySelector('#threads li[aria-current="true"]')?.dataset.agent === "agent-bitcoin",
    document.querySelector('#threads li[aria-current="true"]')?.dataset.agent,
  );
  const add = block()?.querySelector(".task-new .add");
  const room = block()?.querySelector(".task-new .room");
  check("after the tasks, New task and New room", add?.textContent === "+ New task" && room?.textContent === "New room", `${add?.textContent} / ${room?.textContent}`);
  const starting = asked.length;
  add.click();
  await wait(400);
  check(
    "New task starts one for this teammate, listed and marked",
    asked.slice(starting).some((a) => a.name === "start_conversation" && a.args?.agent === "agent-bitcoin") &&
      rows().find((r) => r.getAttribute("aria-current") === "true")?.querySelector(".task-name")?.textContent === "New task",
    rows().map((r) => `${r.getAttribute("aria-current") === "true" ? "*" : ""}${r.querySelector(".task-name")?.textContent}`).join(" | "),
  );

  // Now: said in words, with how many tasks need you.
  const now = document.getElementById("overview-open");
  check("the way to everything at once says Now", /^Now/.test(now.textContent.trim()), now.textContent.trim());
  tell("happened", {
    conversation: "talk-overnight", seq: 9990, kind: "needs_you", asking: "Delete last night's notes",
    detail: "rm notes-old.md", tool: "Bash", call: "now-q", step: "now-q", can_remember: false, rule: "", allows: "",
  });
  await wait(250);
  const count = document.getElementById("now-count");
  check("with how many tasks need you beside it", !count.hidden && Number(count.textContent) >= 1, `hidden=${count.hidden} "${count.textContent}"`);

  // Two coming up, the later one listed first, to be put in order.
  FIXTURE.standing.push(
    {
      conversation: "talk-cut-off", agent: "agent-bitcoin", who: "Bitcoin Desk", name: "Cut off",
      kind: "routine", at: "daily 23:00", what: "The late check", due: Date.now() + 7200000, off: false, stopped: null, paused: false,
    },
    {
      conversation: "talk-room", agent: "agent-bitcoin", who: "Bitcoin Desk", name: "Bitcoin room",
      kind: "routine", at: "daily 21:00", what: "The evening check", due: Date.now() + 600000, off: false, stopped: null, paused: false,
    },
  );
  now.click();
  await wait(600);
  const overview = document.getElementById("overview");
  check("Now opens, called Now", !overview.hidden && overview.querySelector("h1")?.textContent === "Now", overview.querySelector("h1")?.textContent);
  const next = [...overview.querySelectorAll('.job-group[data-state="scheduled"] .job')].map((j) => j.dataset.task);
  const heading = overview.querySelector('.job-group[data-state="scheduled"] h2')?.textContent || "";
  const dueOf = (id) =>
    Math.min(...FIXTURE.standing.filter((s) => s.conversation === id && !s.off && s.due).map((s) => s.due));
  check(
    "Next up lists what is coming soonest first",
    heading.startsWith("Next up") && next.indexOf("talk-room") === 0 && next.includes("talk-cut-off") &&
      next.every((id, n) => n === 0 || dueOf(next[n - 1]) <= dueOf(id)),
    `${heading}: ${next.join(", ")}`,
  );
  check(
    "and what needs you is the first group",
    overview.querySelector(".job-group h2")?.textContent.startsWith("Needs you"),
    overview.querySelector(".job-group h2")?.textContent,
  );
  // A notification opens its task, out from under Now.
  tell("go_to", "talk-2");
  await wait(500);
  check(
    "a notification opens its task from under Now",
    overview.hidden && document.getElementById("talks").value === "talk-2",
    `hidden=${overview.hidden} ${document.getElementById("talks").value}`,
  );
  FIXTURE.standing = FIXTURE.standing.filter((s) => !["daily 21:00", "daily 23:00"].includes(s.at));
  return found;
}

/**
 * The header is the teammate's, and the card is the task's.
 *
 * The row under a teammate's name had eleven things in it, the task's mixed in
 * with the teammate's: Mark finished, Repeat, Watch and Goal were the task's,
 * and Pin and Hide are done to a teammate once in a while. Now the row has the
 * teammate's few, Pin and Hide are in the menu behind its last button, and the
 * task's are on its card, every control one size and none of them filled.
 */
export async function theHeaderAndTheCard() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  await window.__TAURI__.core.invoke("routine_off", { id: "talk-2", off: false });
  tell("repeats", { conversation: "talk-2", repeats: true });
  ["talk-2", "talk-3", FIXTURE.goalIn].forEach((id, n) => tell("happened", { conversation: id, seq: 9940 + n, kind: "done" }));
  await wait(150);
  await openTalk("talk-2");
  const title = document.getElementById("title");
  const card = document.getElementById("task-card");
  const shown = (e) => e && e.getBoundingClientRect().width > 0;
  const fact = (label) => {
    const at = [...card.querySelectorAll("dt")].findIndex((d) => d.textContent === label);
    const dd = card.querySelectorAll("dd")[at];
    return at < 0 ? "" : (dd.querySelector(".words") || dd).textContent;
  };

  // The header: the teammate's own few, and nothing of the task's.
  const inHeader = [...title.querySelectorAll("button")].filter(shown).map((b) => b.id);
  check(
    "the header has the teammate's name and its own few controls, and nothing of the task's",
    ["thread-name", "pause", "granted", "reach", "more"].every((id) => inHeader.includes(id)) &&
      !["pin", "hide", "task-done", "repeat", "watch", "goal"].some((id) => title.querySelector(`#${id}`)),
    inHeader.join(", "),
  );
  if (beingDrawn()) {
    const controls = [
      ...title.querySelectorAll("#pause, #granted, #reach, #more"),
      ...card.querySelectorAll(".actions button"),
    ].filter(shown);
    const heights = controls.map((b) => Math.round(b.getBoundingClientRect().height));
    check(
      "every control in the header and on the card is one height",
      heights.length >= 6 && new Set(heights).size === 1,
      heights.join(", "),
    );
    // The type size of the words, not of the menu's dots, which are drawn a
    // touch larger so three of them read as a button.
    const sizes = new Set(controls.filter((b) => b.id !== "more").map((b) => getComputedStyle(b).fontSize));
    const ways = new Set(
      controls.map((b) => {
        const c = getComputedStyle(b);
        return `${c.borderTopWidth} ${c.borderRadius} ${c.backgroundColor}`;
      }),
    );
    check(
      "and drawn the same way, outlined, none of them filled",
      sizes.size === 1 && ways.size === 1,
      `${[...sizes].join(", ")}; ${[...ways].join(" | ")}`,
    );
    const tops = [...card.querySelectorAll(".actions button")].filter(shown).map((b) => Math.round(b.getBoundingClientRect().top));
    check("the card's are on one line, with Mark finished last", new Set(tops).size === 1 && [...card.querySelectorAll(".actions button")].filter(shown).pop()?.id === "task-done", tops.join(", "));
  }

  // The menu behind the last button: Pin and Hide, with the rest done to a
  // teammate once in a while.
  const more = document.getElementById("more");
  const menu = document.getElementById("menu");
  const pin = () => [...menu.querySelectorAll("button")].find((b) => /^(Pin|Unpin)/.test(b.textContent));
  more.click();
  await wait(120);
  const items = [...menu.querySelectorAll("button")].map((b) => b.textContent);
  check(
    "the last button in the header opens the teammate's menu, with Pin and Hide in it",
    !menu.hidden && pin() && items.some((l) => /^(Hide from|Show in)/.test(l)) && more.getAttribute("aria-expanded") === "true",
    `${menu.hidden ? "closed" : "open"}: ${items.join(", ")}`,
  );
  if (beingDrawn()) {
    const box = menu.getBoundingClientRect();
    const under = more.getBoundingClientRect();
    check(
      "under the button, and inside the window",
      box.top >= under.bottom && box.right <= innerWidth,
      `menu top ${Math.round(box.top)}, right ${Math.round(box.right)}; button bottom ${Math.round(under.bottom)}`,
    );
  }
  const pins = () => asked.filter((a) => a.name === "pin").length;
  const pinsBefore = pins();
  pin()?.click();
  await wait(150);
  check(
    "Pin in it pins the teammate, and the menu closes",
    pins() === pinsBefore + 1 && menu.hidden && more.getAttribute("aria-expanded") === "false",
    `${pins() - pinsBefore} pin; menu ${menu.hidden ? "closed" : "open"}`,
  );
  // As it was.
  more.click();
  await wait(100);
  pin()?.click();
  await wait(150);
  more.click();
  await wait(100);
  more.click();
  await wait(100);
  check("and pressing the button again closes the menu", menu.hidden, menu.hidden ? "closed" : "open");

  // How it runs: said on the card, changed from there, one way at a time.
  const runs = () => {
    const at = [...card.querySelectorAll("dt")].findIndex((d) => d.textContent === "Runs");
    return at < 0 ? null : card.querySelectorAll("dd")[at];
  };
  const change = () => runs()?.querySelector(".change");
  const schedule = document.getElementById("schedule");
  const tab = (id) => document.getElementById(id);
  const panel = (id) => document.getElementById(id);
  check("the card says how it runs, with a way to change it", /daily 07:00/.test(fact("Runs")) && change(), runs()?.textContent);
  change().click();
  await wait(300);
  check(
    "Change opens the three ways a task can run, at the one it runs by",
    !schedule.hidden &&
      tab("repeat").getAttribute("aria-selected") === "true" &&
      !panel("routine").hidden &&
      change().getAttribute("aria-expanded") === "true",
    `tabs ${schedule.hidden ? "hidden" : "shown"}, schedule selected=${tab("repeat").getAttribute("aria-selected")}, routine ${panel("routine").hidden ? "closed" : "open"}`,
  );
  const ways = [...schedule.querySelectorAll("button")].map((b) => b.textContent);
  check(
    "said as the three answers to how it runs",
    ways.join(" / ") === "On a schedule / When something changes / Until a goal is met",
    ways.join(" / "),
  );
  tab("watch").click();
  await wait(300);
  check(
    "choosing another puts the first away: one open at a time",
    panel("routine").hidden &&
      !panel("watching").hidden &&
      tab("watch").getAttribute("aria-selected") === "true" &&
      tab("repeat").getAttribute("aria-selected") === "false",
    `routine ${panel("routine").hidden ? "closed" : "open"}, watch ${panel("watching").hidden ? "closed" : "open"}`,
  );
  if (beingDrawn()) {
    const tabs = schedule.getBoundingClientRect();
    const under = panel("watching").getBoundingClientRect();
    check("the tabs sit right on top of what they open", Math.abs(under.top - tabs.bottom) <= 1, `${Math.round(tabs.bottom)} / ${Math.round(under.top)}`);
  }
  change().click();
  await wait(250);
  check(
    "and Change again puts it all away",
    schedule.hidden && panel("watching").hidden && panel("routine").hidden && change().getAttribute("aria-expanded") === "false",
    `tabs ${schedule.hidden ? "hidden" : "shown"}`,
  );

  // A task that works toward a goal says so, and opens at it.
  await openTalk(FIXTURE.goalIn);
  await wait(200);
  check("a task with a goal says it runs toward it", /^Toward a goal: Get the tests passing/.test(fact("Runs")), fact("Runs"));
  change()?.click();
  await wait(300);
  check(
    "and Change opens at its goal",
    tab("goal").getAttribute("aria-selected") === "true" && !panel("aiming").hidden,
    `goal selected=${tab("goal").getAttribute("aria-selected")}`,
  );
  change()?.click();
  await wait(200);

  // A task another teammate asked for is named by what it asked, and says who.
  const was = FIXTURE.tasks["talk-3"];
  FIXTURE.tasks["talk-3"] = { ...(was || {}), first: "Day Check asks: The weekly tally for Friday." };
  tell("task_made", { conversation: "talk-3", agent: "agent-bitcoin", name: "Asked by Day Check" });
  await wait(250);
  await openTalk("talk-3");
  const sideNames = [...document.querySelectorAll("#threads .task-name")].map((n) => n.textContent);
  check(
    "a task another teammate asked for is named by what it asked, not by who asked",
    card.querySelector(".name")?.textContent === "The weekly tally for Friday" && sideNames.includes("The weekly tally for Friday"),
    `${card.querySelector(".name")?.textContent}; ${sideNames.join(" | ")}`,
  );
  check("and its card says who asked", fact("From") === "Day Check", fact("From") || "no From");
  FIXTURE.tasks["talk-3"] = was;
  tell("task_made", { conversation: "talk-3", agent: "agent-bitcoin", name: "Asked by Day Check" });
  await wait(200);

  // Now: no tile shouts. The one filled button in the window is Send.
  document.getElementById("overview-open").click();
  await wait(600);
  const open = document.querySelector("#overview-tiles .job-foot .open");
  const send = getComputedStyle(document.getElementById("send")).backgroundColor;
  check(
    "Open on a tile in Now is tinted, not filled like Send",
    open && getComputedStyle(open).backgroundColor !== send,
    open ? `${getComputedStyle(open).backgroundColor} against ${send}` : "no tile",
  );
  document.getElementById("overview-done").click();
  await wait(200);
  return found;
}

/**
 * Something new in a task, not read yet, is an orange dot down the side: on
 * the task and on its teammate, until the task is opened.
 *
 * The app knew which lines nobody had read and said so only by teammate, in a
 * line of grey words, with a dot after the name that was never drawn: an
 * empty inline pseudo-element has no size.
 */
export async function anOrangeDotUntilItIsRead() {
  const found = [];
  const check = (what, ok, saw) => found.push({ what, ok: !!ok, saw });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const stand = window.__TAURI__;
  const rowOf = (agent) => document.querySelector(`#threads li[data-agent="${agent}"]`);
  const taskRow = (id) => document.querySelector(`#threads .task[data-task="${id}"]`);
  const dotOn = (el) => el?.querySelector(".unread") || null;
  const orange = (() => {
    const probe = document.createElement("span");
    probe.style.color = "var(--new)";
    document.body.append(probe);
    const seen = getComputedStyle(probe).color;
    probe.remove();
    return seen;
  })();

  // Something new in one task of Bitcoin Desk, with another of its tasks open.
  await openTalk("talk-2");
  stand.nowUnreadIn("talk-cut-off", 2);
  await openTalk("talk-overnight");
  const teammate = rowOf("agent-bitcoin");
  check(
    "the task with something new has a dot",
    dotOn(taskRow("talk-cut-off")) && taskRow("talk-cut-off")?.dataset.unread === "true",
    taskRow("talk-cut-off")?.outerHTML.slice(0, 120) || "no row",
  );
  check("the ones without do not", !dotOn(taskRow("talk-overnight")) && !dotOn(taskRow("talk-2")), "checked talk-overnight and talk-2");
  check("and so does its teammate", dotOn(teammate?.querySelector(".top")), teammate?.className);
  check(
    "orange, in the colour kept for it",
    getComputedStyle(dotOn(taskRow("talk-cut-off"))).backgroundColor === orange &&
      getComputedStyle(dotOn(teammate)).backgroundColor === orange,
    `${getComputedStyle(dotOn(taskRow("talk-cut-off"))).backgroundColor} / ${orange}`,
  );
  check(
    "saying in words what it means, for whoever cannot see a colour",
    /2 new, not read yet/.test(dotOn(taskRow("talk-cut-off"))?.getAttribute("aria-label") || ""),
    dotOn(taskRow("talk-cut-off"))?.getAttribute("aria-label"),
  );
  if (beingDrawn()) {
    const sizeOf = (dot) => dot.getBoundingClientRect();
    const onTask = sizeOf(dotOn(taskRow("talk-cut-off")));
    const onTeammate = sizeOf(dotOn(teammate));
    const row = teammate.getBoundingClientRect();
    check(
      "drawn, at a size that can be seen, inside its row",
      onTask.width >= 7 && onTeammate.width >= 7 && onTeammate.right <= row.right && onTeammate.left > row.left,
      `task dot ${Math.round(onTask.width)}px, teammate dot ${Math.round(onTeammate.width)}px at ${Math.round(onTeammate.right)} of ${Math.round(row.right)}`,
    );
    // In the same place on every teammate's row, so a list can be skimmed.
    stand.nowUnreadIn("talk-waiting", 1);
    await openTalk("talk-overnight");
    const other = dotOn(rowOf("agent-unnamed"));
    check(
      "a teammate whose task is not open has one too, lined up with the others",
      other && Math.abs(other.getBoundingClientRect().right - dotOn(rowOf("agent-bitcoin")).getBoundingClientRect().right) <= 1,
      other ? `${Math.round(other.getBoundingClientRect().right)} / ${Math.round(dotOn(rowOf("agent-bitcoin")).getBoundingClientRect().right)}` : "no dot",
    );
  }

  // Opening the task is reading it.
  await openTalk("talk-cut-off");
  await wait(150);
  check(
    "opening the task reads it, and its dot goes",
    asked.some((a) => a.name === "seen" && a.args?.conversation === "talk-cut-off") && !dotOn(taskRow("talk-cut-off")),
    taskRow("talk-cut-off")?.dataset.unread || "no mark",
  );
  check("and its teammate's, with nothing else of its unread", !dotOn(rowOf("agent-bitcoin")), rowOf("agent-bitcoin")?.className);

  // Something arriving in the task on screen: read at once if somebody is
  // looking, and the moment they come back to the window if not.
  stand.nowUnreadIn("talk-cut-off", 1);
  tell("happened", { conversation: "talk-cut-off", seq: 9930, kind: "done" });
  await wait(250);
  window.dispatchEvent(new Event("focus"));
  await wait(250);
  check(
    "an answer in the task on screen is read once somebody is looking at the window",
    !document.hasFocus() || (!dotOn(taskRow("talk-cut-off")) && !dotOn(rowOf("agent-bitcoin"))),
    document.hasFocus() ? (dotOn(taskRow("talk-cut-off")) ? "still marked" : "read") : "not judged: this window is not in front",
  );

  // A teammate that needs you still says something new arrived.
  stand.nowUnreadIn("talk-waiting", 1);
  tell("happened", {
    conversation: "talk-waiting", seq: 9931, kind: "needs_you", asking: "Sign in to the shop",
    detail: "", tool: "Bash", call: "dot-q", step: "dot-q", can_remember: false, rule: "", allows: "",
  });
  tell("happened", { conversation: "talk-waiting", seq: 9932, kind: "done" });
  await wait(250);
  const waiting = rowOf("agent-unnamed");
  check(
    "a teammate that needs you still shows that something new arrived",
    /Needs you/.test(waiting?.querySelector(".last")?.textContent || "") && dotOn(waiting),
    `${waiting?.querySelector(".last")?.textContent} ${dotOn(waiting) ? "with a dot" : "no dot"}`,
  );
  await openTalk("talk-waiting");
  await wait(150);
  check("until it is read", !dotOn(rowOf("agent-unnamed")), rowOf("agent-unnamed")?.className);
  return found;
}
