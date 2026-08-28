// The window's whole behaviour.
//
// Three things go out (open a thread, say something, stop it) and one kind of
// thing comes back, on its own, as the agent produces it. The page never learns
// which engine answered: it renders the seven events in the protocol and would
// render them identically if a local model were behind them.

// Nothing here works outside the window, and a page whose script dies silently
// is a page that looks dead. Say so on the page itself, since that is where
// somebody is already looking.
if (!window.__TAURI__) {
  document.body.innerHTML =
    '<p style="padding:40px;color:#9aa2b1">This page is the inside of the Errand window ' +
    "and cannot run on its own.</p>";
  throw new Error("no tauri");
}
window.addEventListener("error", (e) => complain(e.message));
window.addEventListener("unhandledrejection", (e) => complain(String(e.reason)));

import { tile, forTool, kindOf } from "./icons.js";
import { render } from "./markdown.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

function complain(why) {
  const t = showing && threads.get(showing);
  if (!t) {
    document.getElementById("thread-name").textContent = why;
    return;
  }
  t.working = false;
  t.messages.push({ kind: "ended", failed: true, text: why });
  drawMessages();
}

const threads = new Map(); // id → { id, name, messages, working }
let showing = null;

const el = {
  threads: document.getElementById("threads"),
  messages: document.getElementById("messages"),
  name: document.getElementById("thread-name"),
  engine: document.getElementById("engine"),
  what: document.getElementById("what"),
  send: document.getElementById("send"),
  form: document.getElementById("composer"),
  new: document.getElementById("new"),
  mark: document.getElementById("mark"),
  find: document.getElementById("find"),
};

/**
 * What a thread is about, as a picture.
 *
 * Worked out from what was asked rather than from what was done, because it has
 * to be on the row the moment somebody presses enter, before anything has
 * happened at all. Kept once worked out, so a row does not change its face
 * halfway through its own job.
 */
function kindFor(t) {
  if (t.kind) return t.kind;
  const asked = t.messages.find((m) => m.kind === "mine");
  // The name is the first few words of the request, which is the only thing
  // there is to go on for a thread that has been read back but not opened.
  const words = asked ? asked.text : t.name === "New errand" ? "" : t.name;
  if (!words) return "spark";
  t.kind = kindOf(words);
  return t.kind;
}

/**
 * Everything on this machine that could answer a thread.
 *
 * Asked for once and kept, because the answer is a handful of network probes
 * and the list does not change while somebody is picking from it. Claude is
 * always in it; the rest is whatever is actually running right now, which is
 * the point -- offering a model that was there yesterday means choosing it and
 * finding out later, somewhere less obvious.
 */
let couldAnswer = null;
async function whatCouldAnswer() {
  if (!couldAnswer) couldAnswer = await invoke("engines");
  return couldAnswer;
}

/** How one choice is recognised again, since a model id alone does not say where it lives. */
function keyOf(engine, settings) {
  if (engine !== "local" || !settings) return "claude";
  try {
    const s = JSON.parse(settings);
    return `local|${s.base_url}|${s.model}`;
  } catch {
    return "claude";
  }
}

/** Fill the picker, and mark what this thread is on. */
async function drawEngines(t) {
  const mine = keyOf(t.on, t.onSettings);
  const choices = await whatCouldAnswer();
  // The thread may have moved on while the probes were out.
  if (showing !== t.id) return;

  el.engine.replaceChildren(
    ...choices.map((c) => {
      const option = document.createElement("option");
      option.value = keyOf(c.engine, c.settings);
      option.textContent = c.name;
      option.selected = option.value === mine;
      return option;
    }),
  );

  // A thread on a model that has since gone quiet still has to say what it is
  // on, or the picker silently claims it is something else.
  if (!choices.some((c) => keyOf(c.engine, c.settings) === mine)) {
    const gone = document.createElement("option");
    gone.value = mine;
    gone.textContent = `${JSON.parse(t.onSettings || "{}").model || "?"} · not running`;
    gone.selected = true;
    el.engine.prepend(gone);
  }
}

/**
 * Move a thread onto a different engine.
 *
 * Said out loud in the thread, because it is not a settings change: an engine
 * holds the memory of the conversation it had, and the new one has not had it.
 * A person who switches and then says "carry on with that" deserves to know
 * that nobody knows what "that" is.
 */
/** The open thread's own mark, which is the same mark as its row in the list. */
function drawMark(t) {
  el.mark.replaceChildren(tile(kindFor(t), t.working));
}

// ------------------------------------------------------------- threads --

function uuid() {
  return crypto.randomUUID();
}

async function start() {
  const id = uuid();
  threads.set(id, {
    id,
    name: "New errand",
    messages: [],
    working: false,
    engine: "",
    on: "claude",
    onSettings: null,
    loaded: true,
  });
  await invoke("open_thread", { id });
  show(id);
  drawThreads();
  el.what.focus();
}

/**
 * What was here before.
 *
 * The window used to be the only place a conversation existed, so closing it
 * was the same as ending everything in it. Now the threads are read back at the
 * start and their messages when one is opened -- lazily, because a person with
 * forty threads should not wait for thirty-nine of them.
 */
async function catchUp() {
  const known = await invoke("threads");
  for (const t of known) {
    threads.set(t.id, {
      id: t.id,
      name: t.name,
      messages: [],
      working: false,
      engine: t.model || "",
      on: t.engine || "claude",
      onSettings: t.engine_settings || null,
      loaded: false,
    });
  }
  drawThreads();
  if (known.length) await open(known[0].id);
  else await start();
}

/** Show a thread, fetching what was said in it the first time. */
async function open(id) {
  const t = threads.get(id);
  if (!t.loaded) {
    t.messages = (await invoke("lines", { id })).map(fromStore);
    t.loaded = true;
  }
  // Reopening is what makes it a conversation rather than a transcript: the
  // agent is handed back its own memory of this thread, not just our copy of it.
  await invoke("open_thread", { id });
  show(id);
  el.what.focus();
}

/** One stored line, as the page holds it. */
function fromStore(line) {
  switch (line.kind) {
    case "mine":
    case "said":
      return { kind: line.kind, text: line.text };
    case "asking":
      // A question that was answered is settled history; one that was not is
      // a question nobody will ever answer now, because the process that asked
      // it is gone. Both are drawn as answered, and only a live one gets
      // buttons.
      return {
        kind: "asking",
        text: line.text,
        tool: line.tool,
        step: line.call,
        answered: line.outcome || "That question expired when the thread closed.",
      };
    case "doing":
      return {
        kind: "doing",
        text: line.text,
        tool: line.tool,
        call: line.call,
        outcome: line.outcome || "",
      };
    default:
      return { kind: "ended", failed: line.kind === "ended", text: line.text };
  }
}

function show(id) {
  showing = id;
  const t = threads.get(id);
  drawMark(t);
  el.name.textContent = t.name;
  drawEngines(t);
  drawThreads();
  drawMessages();
}

function drawThreads() {
  // Not `showing`, which is the thread that is open. Shadowing that here would
  // quietly stop every row knowing whether it is the current one.
  const listed = [...threads.values()].filter((t) => !narrowedTo || narrowedTo.has(t.id));
  if (!listed.length) {
    const none = document.createElement("li");
    none.className = "nothing";
    none.textContent = "Nothing matches that.";
    el.threads.replaceChildren(none);
    return;
  }
  el.threads.replaceChildren(
    ...listed.map((t) => {
      const li = document.createElement("li");
      li.setAttribute("aria-current", String(t.id === showing));
      li.onclick = () => open(t.id);
      li.append(tile(kindFor(t), t.working));

      const words = document.createElement("span");
      words.className = "words";
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = t.name;

      const last = document.createElement("span");
      last.className = "last";
      const said = [...t.messages].reverse().find((m) => m.kind === "said" || m.kind === "mine");
      const waiting = t.messages.some((m) => m.kind === "asking" && !m.answered);
      last.textContent = waiting
        ? "Waiting on you"
        : t.working
          ? "Working…"
          : said
            ? said.text
            : "Nothing said yet";
      if (waiting) last.classList.add("waiting");

      words.append(name, last);
      li.append(words);
      return li;
    }),
  );
}

// ------------------------------------------------------------ messages --

function drawMessages() {
  const t = threads.get(showing);
  if (!t) return;
  el.messages.replaceChildren(...t.messages.map(draw).filter(Boolean));
  if (t.working) el.messages.append(thinking());
  el.messages.scrollTop = el.messages.scrollHeight;
}

function draw(m) {
  const node = document.createElement("li");
  switch (m.kind) {
    case "said":
      node.className = "said";
      node.append(render(m.text), doneWith(m));
      return node;
    // Your own words are shown exactly as you typed them. Reading somebody's
    // asterisks as emphasis is a small thing to get wrong and an odd one to
    // explain.
    case "mine":
      node.className = "mine";
      node.textContent = m.text;
      return node;
    case "doing": {
      // A step with no answer yet is a step still happening, and it is the only
      // thing on the screen that knows that. So it says so, rather than sitting
      // there looking exactly like the four finished steps above it.
      node.className = m.outcome ? "doing" : "doing running";
      node.append(tile(forTool(m.tool), !m.outcome));
      const what = document.createElement("span");
      what.className = "what";
      what.textContent = m.text;
      node.append(what);
      if (m.outcome) {
        const out = document.createElement("span");
        out.className = "outcome";
        out.textContent = m.outcome;
        node.append(out);
      }
      return node;
    }
    case "asking":
      return asks(m);
    case "ended":
      node.className = m.failed ? "ended failed" : "ended";
      node.textContent = m.text;
      return node;
    default:
      return null;
  }
}

/**
 * A step that has stopped, and the three things you can say to it.
 *
 * The command is shown whole and unabbreviated, because a question about
 * something you cannot see is not a question anybody can answer honestly, and
 * the part of a long command worth worrying about is usually at the end of it.
 *
 * Once answered the card becomes a line of history rather than disappearing.
 * What you allowed is worth being able to look back at.
 */
function asks(m) {
  const li = document.createElement("li");
  li.className = "asking";
  li.append(tile(forTool(m.tool), false));

  const body = document.createElement("div");
  body.className = "question";

  const what = document.createElement("p");
  what.className = "wants";
  what.textContent = m.text;
  body.append(what);

  if (m.detail) {
    const detail = document.createElement("pre");
    detail.className = "detail";
    detail.textContent = m.detail;
    body.append(detail);
  }

  if (m.answered) {
    const settled = document.createElement("p");
    settled.className = "settled";
    settled.textContent = m.outcome ? `${m.answered} · ${m.outcome}` : m.answered;
    body.append(settled);
  } else {
    const choices = document.createElement("div");
    choices.className = "choices";
    const say = (label, said, kind) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = kind;
      b.textContent = label;
      b.onclick = () => answer(m, said, label);
      return b;
    };
    choices.append(say("Yes", "yes", "yes"));
    if (m.can_remember) choices.append(say("Always", "always", "always"));
    choices.append(say("No", "no", "no"));
    body.append(choices);
  }

  li.append(body);
  return li;
}

/** Say yes or no, and let the halted work go on or not. */
async function answer(m, said, label) {
  const t = threads.get(showing);
  // Settled here as well as in the store, so the buttons stop being buttons
  // the moment they are pressed rather than when the answer comes back.
  m.answered = label === "Always" ? "You said yes, and to stop asking" : `You said ${label.toLowerCase()}`;
  t.working = said !== "no";
  drawMessages();
  drawThreads();
  try {
    await invoke("answer", { id: t.id, call: m.call, step: m.step, said });
  } catch (why) {
    m.answered = String(why);
    drawMessages();
  }
}

function thinking() {
  const li = document.createElement("li");
  li.className = "thinking";
  li.append(...[0, 1, 2].map(() => document.createElement("i")));
  return li;
}

// ------------------------------------------------------- what happened --

listen("happened", ({ payload }) => {
  const t = threads.get(payload.thread);
  if (!t) return;

  switch (payload.kind) {
    case "started":
      t.engine = payload.model;
      break;

    // Only settled lines are kept. The partial ones exist so that a sentence
    // being written looks like a sentence being written, and keeping them all
    // would mean keeping every prefix of every sentence.
    case "said":
      if (payload.settled) t.messages.push({ kind: "said", text: payload.text });
      break;

    case "doing":
      t.messages.push({ kind: "doing", text: payload.what, tool: payload.tool, call: payload.call });
      break;

    // A step that has answered stops looking like a step that has hung, so the
    // outcome lands on the step rather than on a line of its own.
    // Joined by the id both sides carry, not by guessing at the most recent
    // step: two calls to the same tool are otherwise indistinguishable, and
    // several can be in flight at once.
    case "did": {
      const step = t.messages.find(
        (m) => (m.kind === "doing" && m.call === payload.call) || m.step === payload.call,
      );
      if (step) step.outcome = payload.outcome;
      break;
    }

    // Stopped, and waiting. Not working any more: a spinner beside a question
    // says the machine is busy when the truth is that it is waiting for you.
    case "needs_you": {
      t.working = false;
      const asking = {
        kind: "asking",
        text: payload.asking,
        detail: payload.detail,
        tool: payload.tool,
        // Two ids: one to answer the engine with, one that names the step and
        // is what the outcome will arrive against.
        call: payload.call,
        step: payload.step,
        can_remember: payload.can_remember,
        answered: null,
      };
      // The step it halted is already on screen. Turn that line into the
      // question rather than adding a second one saying the same sentence.
      const already = t.messages.findIndex((m) => m.kind === "doing" && m.call === payload.step);
      if (already >= 0) t.messages[already] = asking;
      else t.messages.push(asking);
      break;
    }

    case "done":
      t.working = false;
      if (t.name === "New errand") {
        t.name = titleFrom(t);
        invoke("call_it", { id: t.id, name: t.name });
      }
      break;

    case "failed":
      t.working = false;
      t.messages.push({ kind: "ended", failed: true, text: payload.why || "It could not finish." });
      break;
  }

  if (payload.thread === showing) {
    drawMark(t);
    el.name.textContent = t.name;
    drawMessages();
  }
  drawThreads();
});

/**
 * The two things worth doing with an answer.
 *
 * Copy, because an answer is often the point of the errand and it has to be
 * able to leave. And ask again, which re-sends the last thing you asked rather
 * than re-running the reply: an engine cannot un-say something, so the honest
 * version of "regenerate" is asking the same question a second time.
 *
 * On hover rather than always, since a column of buttons down the side of a
 * conversation competes with the conversation.
 */
function doneWith(m) {
  const row = document.createElement("span");
  row.className = "did-with";

  const copy = document.createElement("button");
  copy.type = "button";
  copy.textContent = "Copy";
  copy.onclick = async () => {
    await navigator.clipboard.writeText(m.text);
    copy.textContent = "Copied";
    setTimeout(() => (copy.textContent = "Copy"), 1400);
  };
  row.append(copy);

  // Offered on the last answer only. Asking again from halfway up the thread
  // would put the reply at the bottom, under everything that came after it.
  const t = threads.get(showing);
  const asked = t && [...t.messages].reverse().find((x) => x.kind === "mine");
  const isLast = t && [...t.messages].reverse().find((x) => x.kind === "said") === m;
  if (asked && isLast) {
    const again = document.createElement("button");
    again.type = "button";
    again.textContent = "Ask again";
    again.onclick = () => sayIt(asked.text);
    row.append(again);
  }
  return row;
}

/** A thread is named after what was asked of it, since that is how it is remembered. */
function titleFrom(t) {
  const first = t.messages.find((m) => m.kind === "mine");
  if (!first) return "New errand";
  const words = first.text.trim().split(/\s+/).slice(0, 6).join(" ");
  return words.length > 42 ? words.slice(0, 41) + "…" : words;
}

// -------------------------------------------------------------- saying --

el.form.addEventListener("submit", (e) => {
  e.preventDefault();
  const text = el.what.value.trim();
  if (!text) return;
  el.what.value = "";
  el.what.style.height = "auto";
  sayIt(text);
});

/** Say something to the thread that is open, from wherever it was typed. */
async function sayIt(text) {
  if (!showing) return;
  const t = threads.get(showing);
  t.messages.push({ kind: "mine", text });
  // Working from the moment it is sent, not from the moment something comes
  // back: the gap between the two is exactly when a person wonders whether the
  // thing they typed went anywhere.
  t.working = true;
  drawMessages();
  drawThreads();

  try {
    await invoke("say", { id: t.id, text });
  } catch (why) {
    t.working = false;
    t.messages.push({ kind: "ended", failed: true, text: String(why) });
    drawMessages();
  }
}

// Enter sends; shift-enter is a new line. And the box grows with what is in it,
// because an errand worth describing is sometimes worth two sentences.
// Up and down walk back through what you have already asked this thread, the
// way a terminal does, because the second thing you ask is usually the first
// thing again with one word changed. Only from an empty box, or while already
// walking, so it never steals the arrow keys from somebody editing a sentence.
let walkedBack = null;

el.what.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    walkedBack = null;
    el.form.requestSubmit();
    return;
  }

  if (e.key !== "ArrowUp" && e.key !== "ArrowDown") {
    walkedBack = null;
    return;
  }
  const t = threads.get(showing);
  if (!t) return;
  const asked = t.messages.filter((m) => m.kind === "mine").map((m) => m.text);
  if (!asked.length) return;
  if (walkedBack === null && el.what.value.trim()) return;

  e.preventDefault();
  if (walkedBack === null) walkedBack = asked.length;
  walkedBack += e.key === "ArrowUp" ? -1 : 1;

  if (walkedBack >= asked.length) {
    // Past the newest is where you started: an empty box, ready for a new one.
    walkedBack = null;
    el.what.value = "";
  } else {
    walkedBack = Math.max(0, walkedBack);
    el.what.value = asked[walkedBack];
  }
  el.what.style.height = "auto";
  el.what.style.height = Math.min(el.what.scrollHeight, window.innerHeight * 0.4) + "px";
  el.what.setSelectionRange(el.what.value.length, el.what.value.length);
});
el.what.addEventListener("input", () => {
  el.what.style.height = "auto";
  el.what.style.height = Math.min(el.what.scrollHeight, window.innerHeight * 0.4) + "px";
});

el.engine.addEventListener("change", async () => {
  const t = threads.get(showing);
  if (!t) return;
  const choice = (await whatCouldAnswer()).find(
    (c) => keyOf(c.engine, c.settings) === el.engine.value,
  );
  if (!choice) return;

  t.on = choice.engine;
  t.onSettings = choice.settings;
  t.working = false;
  try {
    await invoke("use_engine", { id: t.id, engine: choice.engine, settings: choice.settings });
    await invoke("open_thread", { id: t.id });
    // Said in the thread rather than in a toast that disappears. Somebody
    // scrolling back next week needs to see where the conversation changed
    // hands, or the gap in what it remembers looks like a fault.
    t.messages.push({
      kind: "ended",
      failed: false,
      text: `Now on ${choice.name}. It has not seen anything said before this line.`,
    });
  } catch (why) {
    t.messages.push({ kind: "ended", failed: true, text: String(why) });
  }
  drawMessages();
  drawThreads();
});

// One handler for every link in every thread, rather than one per link: the
// timeline is redrawn constantly and listeners attached to its contents would
// be attached to nodes that are already gone.
el.messages.addEventListener("click", (e) => {
  const link = e.target.closest("a[data-away]");
  if (!link) return;
  e.preventDefault();
  invoke("show_in_browser", { url: link.href }).catch((why) => complain(String(why)));
});

/**
 * Which threads the list is showing.
 *
 * Null while nothing is being looked for, which is not the same as an empty
 * search returning everything: the difference is that an unsuccessful search
 * shows nothing and says so, rather than quietly showing the whole list as
 * though it had matched.
 */
let narrowedTo = null;

let searchingAfter = null;
el.find.addEventListener("input", () => {
  // Waited on briefly, because searching on every keystroke asks the store a
  // question about a word somebody is still in the middle of typing.
  clearTimeout(searchingAfter);
  searchingAfter = setTimeout(look, 140);
});

async function look() {
  const lookingFor = el.find.value.trim();
  if (!lookingFor) {
    narrowedTo = null;
    drawThreads();
    return;
  }
  // camelCase on the way over: the command takes `looking_for` and the bridge
  // renames it. Every other command here has single-word arguments, so this is
  // the first place it could show up, and it showed up as a red line in a
  // thread rather than as anything a stub would have caught.
  const found = await invoke("matching", { lookingFor });
  narrowedTo = new Set(found.map((t) => t.id));
  // A thread that has never been opened is not in the page's list yet, and a
  // search that finds one has to be able to show it.
  for (const t of found) {
    if (!threads.has(t.id)) {
      threads.set(t.id, {
        id: t.id,
        name: t.name,
        messages: [],
        working: false,
        engine: t.model || "",
        on: t.engine || "claude",
        onSettings: t.engine_settings || null,
        loaded: false,
      });
    }
  }
  drawThreads();
}

/**
 * A file dropped on the window.
 *
 * Its path goes into the box rather than its contents. Both engines can open a
 * file they are told about -- Claude Code natively, the local one through
 * `read_file` -- so handing over the path is the whole job, and it keeps a
 * hundred-megabyte CSV out of a context window that could never hold it.
 *
 * Through the window's own drag events rather than the page's, because the two
 * are not equivalent here: the page is given a file with no path, for the same
 * reason a web page is, and a name with no directory is not something either
 * engine can open. The window is given the real path.
 *
 * A picture is the case this does not cover, and it is uncovered on purpose
 * rather than half-done: showing one to a model means base64 in the message and
 * an endpoint that can see, and guessing at either would produce something that
 * silently sends nothing.
 */
function catchFiles() {
  const webview = window.__TAURI__?.webview?.getCurrentWebview?.();
  if (!webview) return; // Not inside the window, which is only true in a browser.
  webview.onDragDropEvent(({ payload }) => {
    if (payload.type === "over" || payload.type === "enter") {
      document.body.classList.add("catching");
      return;
    }
    document.body.classList.remove("catching");
    if (payload.type !== "drop" || !payload.paths?.length) return;
    const already = el.what.value.trim();
    el.what.value = already ? `${already}\n${payload.paths.join("\n")}` : payload.paths.join("\n");
    el.what.focus();
    el.what.dispatchEvent(new Event("input"));
  });
}
catchFiles();

el.new.addEventListener("click", start);

// What was here before, and something to type into if there was nothing.
catchUp();
