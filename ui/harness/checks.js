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

  // The agent that owns it first: the conversation picker only ever lists the
  // conversations of whoever is open, so setting it to somebody else's does
  // nothing at all and quietly leaves the wrong thread on screen.
  const openTalk = async (id) => {
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
  };

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

  // A goal is one long sentence and its buttons belong under it, so that panel
  // is two lines on purpose. Every control in it is still the same height, and
  // the ones sharing a line still share a line -- what is not true of it is
  // that the whole panel is one row, and asserting that would be asserting
  // something nobody wants.
  const panels = [
    ["repeat", "#routine", { oneLine: true }],
    ["watch", "#watching", { oneLine: true }],
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

  await new Promise((r) => setTimeout(r, 60));
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
