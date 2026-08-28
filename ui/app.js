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
};

// ------------------------------------------------------------- threads --

function uuid() {
  return crypto.randomUUID();
}

async function start() {
  const id = uuid();
  threads.set(id, { id, name: "New errand", messages: [], working: false, engine: "", loaded: true });
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
    case "doing":
      return { kind: "doing", text: line.text, call: line.call, outcome: line.outcome || "" };
    default:
      return { kind: "ended", failed: line.kind === "ended", text: line.text };
  }
}

function show(id) {
  showing = id;
  const t = threads.get(id);
  el.name.textContent = t.name;
  el.engine.textContent = t.engine;
  drawThreads();
  drawMessages();
}

function drawThreads() {
  el.threads.replaceChildren(
    ...[...threads.values()].map((t) => {
      const li = document.createElement("li");
      li.setAttribute("aria-current", String(t.id === showing));
      li.onclick = () => open(t.id);

      const name = document.createElement("span");
      name.className = "name";
      name.textContent = t.name;

      const last = document.createElement("span");
      last.className = "last";
      const said = [...t.messages].reverse().find((m) => m.kind === "said" || m.kind === "mine");
      last.textContent = t.working ? "Working…" : said ? said.text : "Nothing said yet";

      li.append(name, last);
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
    case "mine":
    case "said":
      node.className = m.kind;
      node.textContent = m.text;
      return node;
    case "doing": {
      node.className = "doing";
      node.append(m.text);
      if (m.outcome) {
        const out = document.createElement("span");
        out.className = "outcome";
        out.textContent = m.outcome;
        node.append(out);
      }
      return node;
    }
    case "ended":
      node.className = m.failed ? "ended failed" : "ended";
      node.textContent = m.text;
      return node;
    default:
      return null;
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
      const step = t.messages.find((m) => m.kind === "doing" && m.call === payload.call);
      if (step) step.outcome = payload.outcome;
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
    el.engine.textContent = t.engine;
    el.name.textContent = t.name;
    drawMessages();
  }
  drawThreads();
});

/** A thread is named after what was asked of it, since that is how it is remembered. */
function titleFrom(t) {
  const first = t.messages.find((m) => m.kind === "mine");
  if (!first) return "New errand";
  const words = first.text.trim().split(/\s+/).slice(0, 6).join(" ");
  return words.length > 42 ? words.slice(0, 41) + "…" : words;
}

// -------------------------------------------------------------- saying --

el.form.addEventListener("submit", async (e) => {
  e.preventDefault();
  const text = el.what.value.trim();
  if (!text || !showing) return;

  const t = threads.get(showing);
  t.messages.push({ kind: "mine", text });
  // Working from the moment it is sent, not from the moment something comes
  // back: the gap between the two is exactly when a person wonders whether the
  // thing they typed went anywhere.
  t.working = true;
  el.what.value = "";
  el.what.style.height = "auto";
  drawMessages();
  drawThreads();

  try {
    await invoke("say", { id: showing, text });
  } catch (why) {
    t.working = false;
    t.messages.push({ kind: "ended", failed: true, text: String(why) });
    drawMessages();
  }
});

// Enter sends; shift-enter is a new line. And the box grows with what is in it,
// because an errand worth describing is sometimes worth two sentences.
el.what.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    el.form.requestSubmit();
  }
});
el.what.addEventListener("input", () => {
  el.what.style.height = "auto";
  el.what.style.height = Math.min(el.what.scrollHeight, window.innerHeight * 0.4) + "px";
});

el.new.addEventListener("click", start);

// What was here before, and something to type into if there was nothing.
catchUp();
