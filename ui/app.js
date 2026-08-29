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
  const t = talking();
  if (!t) {
    document.getElementById("thread-name").textContent = why;
    return;
  }
  t.working = false;
  t.messages.push({ kind: "ended", failed: true, text: why });
  drawMessages();
}

// Two maps, because there are two things.
//
// An agent is who; a conversation is what was said. They were one record and
// one `showing` id, which quietly answered two different questions -- whose
// name is in the header, and whose messages are on screen -- and gave the same
// answer to both. That is fine until an agent has a second conversation.
const agents = new Map(); // agent id → identity, engine, pinned, hidden
const talks = new Map(); // conversation id → { id, agent, name, messages, working, loaded }

/// What an agent is called before it has settled on anything. Matches the store.
const NOT_YET_NAMED = "New errand";
let showingAgent = null;
let showing = null; // the conversation on screen

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
  reach: document.getElementById("reach"),
  talks: document.getElementById("talks"),
  repeat: document.getElementById("repeat"),
  granted: document.getElementById("granted"),
  granting: document.getElementById("granting"),
  asks: document.getElementById("asks"),
  allowed: document.getElementById("allowed"),
  routine: document.getElementById("routine"),
  routineAt: document.getElementById("routine-at"),
  routineWhat: document.getElementById("routine-what"),
  routineSave: document.getElementById("routine-save"),
  routineStop: document.getElementById("routine-stop"),
  routineSays: document.getElementById("routine-says"),
  pin: document.getElementById("pin"),
  hide: document.getElementById("hide"),
  whois: document.getElementById("whois"),
  whoisName: document.getElementById("whois-name"),
  whoisTitle: document.getElementById("whois-title"),
  whoisAbout: document.getElementById("whois-about"),
  whoisSave: document.getElementById("whois-save"),
  reachable: document.getElementById("reachable"),
};

/**
 * What a thread is about, as a picture.
 *
 * Worked out from what was asked rather than from what was done, because it has
 * to be on the row the moment somebody presses enter, before anything has
 * happened at all. Kept once worked out, so a row does not change its face
 * halfway through its own job.
 */
function kindFor(a) {
  // What it chose for itself, where it has chosen. The guess below is only for
  // an agent that has not been asked yet.
  if (a.mark) return a.mark;
  if (a.kind) return a.kind;
  // Its name is the only thing to go on before it has settled on anything, and
  // for a brand new one there is not even that.
  const words = a.name === NOT_YET_NAMED ? "" : a.name;
  if (!words) return "spark";
  a.kind = kindOf(words);
  return a.kind;
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

/**
 * The value of the last option, which is not an engine but a question.
 *
 * A model somewhere else on the network is a real answer to "what can answer
 * this", but finding one means thousands of probes across every machine on the
 * subnet and most of a minute. So it is offered rather than done: opening the
 * picker stays instant, and looking wider is a thing somebody asks for.
 */
const LOOK_WIDER = "__wider";

/** Sweep the network for models, and put whatever answered into the picker. */
async function lookWider(a) {
  const was = el.engine.value;
  el.engine.disabled = true;
  const saying = el.engine.options[el.engine.selectedIndex];
  if (saying) saying.textContent = "Looking on the network…";

  try {
    couldAnswer = await invoke("engines", { wider: true });
  } catch (why) {
    couldAnswer = null;
    if (saying) saying.textContent = String(why);
    el.engine.disabled = false;
    return;
  }

  el.engine.disabled = false;
  await drawEngines(a);
  // Nothing was chosen, only looked for, so the agent stays on what it was on.
  el.engine.value = was === LOOK_WIDER ? keyOf(a.on, a.onSettings) : was;
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

/** Fill the picker, and mark what this agent is on. */
async function drawEngines(a) {
  const mine = keyOf(a.on, a.onSettings);
  const choices = await whatCouldAnswer();
  // The window may have moved on while the probes were out.
  if (showingAgent !== a.id) return;

  el.engine.replaceChildren(
    ...choices.map((c) => {
      const option = document.createElement("option");
      option.value = keyOf(c.engine, c.settings);
      option.textContent = c.name;
      option.selected = option.value === mine;
      return option;
    }),
  );

  const wider = document.createElement("option");
  wider.value = LOOK_WIDER;
  wider.textContent = "Look on the network…";
  el.engine.append(wider);

  // A thread on a model that has since gone quiet still has to say what it is
  // on, or the picker silently claims it is something else.
  if (!choices.some((c) => keyOf(c.engine, c.settings) === mine)) {
    const gone = document.createElement("option");
    gone.value = mine;
    gone.textContent = `${JSON.parse(a.onSettings || "{}").model || "?"} · not running`;
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
/** The open agent's own mark, which is the same mark as its row in the list. */
function drawMark(a) {
  el.mark.replaceChildren(tile(kindFor(a), busy(a.id), a.hue));
}

/** Is any of this agent's conversations working? */
function busy(agent) {
  return [...talks.values()].some((t) => t.agent === agent && t.working);
}

/** Is any of them waiting on somebody? */
function waitingOn(agent) {
  return [...talks.values()].some(
    (t) => t.agent === agent && t.messages.some((m) => m.kind === "asking" && !m.answered),
  );
}

// ------------------------------------------------------------- threads --

function uuid() {
  return crypto.randomUUID();
}

/**
 * A new agent, with its first conversation.
 *
 * Both get the same id, which is what the store does for a first conversation
 * and what the migration did for every agent that predates conversations. One
 * rule rather than two, and the id is the engine's session id either way.
 */
async function start() {
  const id = uuid();
  agents.set(id, asAgent({ id, name: NOT_YET_NAMED }));
  talks.set(id, asTalk({ id, agent: id, name: "First" }, { loaded: true }));
  await invoke("open_thread", { id });
  await show(id);
  drawThreads();
  el.what.focus();
}

/**
 * Another conversation with the agent already open.
 *
 * The point of the whole change: asking this agent about something unrelated no
 * longer means asking a stranger, and it no longer means dragging this
 * morning's briefing along in front of the question.
 */
async function alsoAsk() {
  const a = whose();
  if (!a) return;
  const id = uuid();
  await invoke("start_conversation", { id, agent: a.id, name: "New conversation" });
  talks.set(id, asTalk({ id, agent: a.id, name: "New conversation" }, { loaded: true }));
  // `show` opens it. Doing it here as well was harmless and still wrong: the
  // second call is a no-op only because the first one already succeeded.
  await show(id);
  el.what.focus();
}

/**
 * What was here before.
 *
 * The agents at the start, and a conversation's messages when it is opened --
 * lazily, because somebody with forty agents should not wait for thirty-nine of
 * them.
 */
async function catchUp() {
  const known = await invoke("agents");
  for (const a of known) agents.set(a.id, asAgent(a, agents.get(a.id)));
  drawThreads();
  if (known.length) await openAgent(known[0].id);
  else await start();
}

/** Open an agent, at whichever conversation it spoke in most recently. */
async function openAgent(agent) {
  const theirs = (await invoke("conversations", { agent })).map((c) =>
    asTalk(c, talks.get(c.id)),
  );
  for (const t of theirs) talks.set(t.id, t);
  // An agent with no conversation at all should not be possible, but a window
  // that shows nothing and says nothing would be the worst way to find out.
  if (!theirs.length) {
    const id = uuid();
    await invoke("start_conversation", { id, agent, name: "First" });
    talks.set(id, asTalk({ id, agent, name: "First" }, { loaded: true }));
    return show(id);
  }
  return show(theirs[0].id);
}

/** Show a conversation, fetching what was said in it the first time. */
async function show(id) {
  const t = talks.get(id);
  if (!t) return;
  showing = id;
  showingAgent = t.agent;

  if (!t.loaded) {
    t.messages = (await invoke("lines", { id })).map(fromStore);
    t.loaded = true;
  }
  // Reopening is what makes it a conversation rather than a transcript: the
  // engine is handed back its own memory of this one, not just our copy of it.
  await invoke("open_thread", { id });

  const a = whose();
  el.whois.hidden = true;
  el.routine.hidden = true;
  el.granting.hidden = true;
  if (a) {
    drawMark(a);
    drawPinned(a);
    el.name.textContent = a.name;
    drawEngines(a);
  }
  drawTalks();
  drawThreads();
  drawMessages();
}

/**
 * The agent's conversations, and a way to start another.
 *
 * A picker rather than a second list down the side, because most agents will
 * have one and a list of one is furniture.
 */
function drawTalks() {
  const a = whose();
  const theirs = [...talks.values()].filter((t) => t.agent === showingAgent);
  el.talks.replaceChildren(
    ...theirs.map((t) => {
      const option = document.createElement("option");
      option.value = t.id;
      // A clock on the name, so a scheduled conversation is recognisable
      // without opening the panel that would tell you.
      option.textContent = t.repeats ? `${t.name} ⏱` : t.name;
      option.selected = t.id === showing;
      return option;
    }),
  );
  const another = document.createElement("option");
  another.value = "+";
  another.textContent = "New conversation…";
  el.talks.append(another);
  el.talks.hidden = !a;
}

/**
 * One agent, as the page holds it.
 *
 * The identity it settled on is kept apart from the guess: `mark` is what it
 * chose and nothing means it has not been asked yet, which is when the guess
 * from the words is the best there is.
 */
function asAgent(a, keeping) {
  return {
    id: a.id,
    name: a.name,
    title: a.title || "",
    about: a.about || "",
    mark: a.mark || null,
    hue: a.hue || null,
    asks: a.asks || "ask",
    pinned: !!a.pinned,
    hidden: !!a.hidden,
    engine: a.model || "",
    on: a.engine || "claude",
    onSettings: a.engine_settings || null,
    kind: keeping?.kind,
  };
}

/** One conversation, as the page holds it. */
function asTalk(c, keeping) {
  return {
    id: c.id,
    agent: c.agent,
    name: c.name,
    repeats: !!c.runs_at,
    messages: keeping?.messages ?? [],
    working: keeping?.working ?? false,
    loaded: keeping?.loaded ?? false,
  };
}

/** The agent whose conversation is on screen. */
function whose() {
  return agents.get(showingAgent);
}

/** The conversation on screen. */
function talking() {
  return talks.get(showing);
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

function drawThreads() {
  // Hidden ones are out of the way, not gone: a search still finds them,
  // because "where did that go" is exactly when somebody looks.
  const listed = [...agents.values()].filter((a) =>
    narrowedTo ? narrowedTo.has(a.id) : !a.hidden,
  );
  if (!listed.length) {
    const none = document.createElement("li");
    none.className = "nothing";
    none.textContent = "Nothing matches that.";
    el.threads.replaceChildren(none);
    return;
  }
  el.threads.replaceChildren(
    ...listed.map((a) => {
      const li = document.createElement("li");
      li.setAttribute("aria-current", String(a.id === showingAgent));
      li.onclick = () => openAgent(a.id);
      li.append(tile(kindFor(a), busy(a.id), a.hue));
      if (a.pinned) li.classList.add("pinned");

      const words = document.createElement("span");
      words.className = "words";

      const name = document.createElement("span");
      name.className = "name";
      name.textContent = a.name;
      if (a.title) {
        // The role, so a list of agents can be read at a glance rather than
        // deciphered from names somebody's agents chose for themselves.
        const role = document.createElement("span");
        role.className = "role";
        role.textContent = a.title;
        name.append(role);
      }

      // What it is for, rather than the last thing said to it. An agent is a
      // standing job, and the useful line under its name is the job -- the last
      // message belongs to one of its conversations, not to it.
      const last = document.createElement("span");
      last.className = "last";
      last.textContent = waitingOn(a.id)
        ? "Waiting on you"
        : busy(a.id)
          ? "Working…"
          : a.about || "Nothing said yet";
      if (waitingOn(a.id)) last.classList.add("waiting");

      words.append(name, last);
      li.append(words);
      return li;
    }),
  );
}

// ------------------------------------------------------------ messages --

function drawMessages() {
  const t = talking();
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
  const t = talking();
  // Settled here as well as in the store, so the buttons stop being buttons
  // the moment they are pressed rather than when the answer comes back.
  m.answered = label === "Always" ? "You said yes, and to stop asking" : `You said ${label.toLowerCase()}`;
  t.working = said !== "no";
  drawMessages();
  drawThreads();
  try {
    await invoke("answer", {
      id: t.id,
      call: m.call,
      step: m.step,
      said,
      tool: m.tool || "",
      rule: m.rule || "",
    });
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

// An agent that has worked out what it is for. Arrives once, some seconds
// after its first errand ends, and changes its name under the pointer -- which
// is the intended effect: it is the moment it stops being "New errand".
listen("settled", ({ payload }) => {
  const [id, on] = payload;
  const t = agents.get(id);
  if (!t) return;
  t.name = on.name;
  t.title = on.title;
  t.about = on.about;
  t.mark = on.mark;
  t.hue = on.hue;
  if (id === showingAgent) {
    el.name.textContent = t.name;
    drawMark(t);
  }
  drawThreads();
});

listen("happened", ({ payload }) => {
  const t = talks.get(payload.conversation);
  if (!t) return;

  switch (payload.kind) {
    // What is answering is the agent's, not this conversation's: choosing an
    // engine is a decision about the agent and every conversation it has.
    case "started": {
      const on = agents.get(t.agent);
      if (on) on.engine = payload.model;
      break;
    }

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
        // What "always" would actually allow, so the app can store it and show
        // it back. Empty means any use of the tool.
        rule: payload.rule || "",
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
      // Naming is the agent's own job now, asked for after this by the app.
      break;

    case "failed":
      t.working = false;
      t.messages.push({ kind: "ended", failed: true, text: payload.why || "It could not finish." });
      break;
  }

  // The agent's mark shows that one of its conversations is working, so it is
  // redrawn whichever conversation the event belongs to.
  const a = agents.get(t.agent);
  if (a) {
    if (payload.conversation === showing) drawMark(a);
    drawThreads();
  }
  if (payload.conversation === showing) drawMessages();
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
  const t = talking();
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
  const t = talking();
  if (!t) return;
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
  const t = talking();
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
  const t = whose();
  if (!t) return;
  if (el.engine.value === LOOK_WIDER) {
    await lookWider(t);
    return;
  }
  const choice = (await whatCouldAnswer()).find(
    (c) => keyOf(c.engine, c.settings) === el.engine.value,
  );
  if (!choice) return;

  t.on = choice.engine;
  t.onSettings = choice.settings;

  const talk = talking();
  if (talk) talk.working = false;
  try {
    // The choice is the agent's; the session that has to be restarted is this
    // conversation's. Two ids, and passing either one to both is the mistake
    // this whole change exists to make impossible.
    await invoke("use_engine", { id: t.id, engine: choice.engine, settings: choice.settings });
    if (talk) await invoke("open_thread", { id: talk.id });
    // Said in the conversation rather than in a toast that disappears.
    // Somebody scrolling back next week needs to see where it changed hands,
    // or the gap in what it remembers looks like a fault.
    talk?.messages.push({
      kind: "ended",
      failed: false,
      text: `Now on ${choice.name}. It has not seen anything said before this line.`,
    });
  } catch (why) {
    talk?.messages.push({ kind: "ended", failed: true, text: String(why) });
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
  // An agent that has never been opened is not in the page's list yet, and a
  // search that finds one has to be able to show it.
  for (const a of found) agents.set(a.id, asAgent(a, agents.get(a.id)));
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

/**
 * What this thread can reach, beyond what the engine brought with it.
 *
 * The same list for either engine, and read from the same file Claude Code
 * reads, which is the point: Claude Code connects to these servers itself and
 * the local engine connects through ours, so a tool that works under one works
 * under the other and is called the same thing.
 *
 * A server that did not start says why. Before this existed, a tool that was
 * quietly absent was indistinguishable from a tool the agent chose not to use,
 * and there was nowhere to look.
 */
el.reach.addEventListener("click", async () => {
  if (!el.reachable.hidden) {
    el.reachable.hidden = true;
    return;
  }
  if (!showing) return;
  el.reachable.hidden = false;
  el.reachable.replaceChildren(saying("Asking them…"));

  let servers;
  try {
    servers = await invoke("outside", { id: showing });
  } catch (why) {
    el.reachable.replaceChildren(saying(String(why)));
    return;
  }

  if (!servers.length) {
    el.reachable.replaceChildren(
      note(
        "p",
        "No MCP servers configured. They are read from ~/.claude.json, the " +
          "same ones Claude Code uses, so anything set up there works here too.",
      ),
    );
    return;
  }

  el.reachable.replaceChildren(
    note("p", "MCP servers, read from ~/.claude.json. Both engines get the same ones."),
    ...servers.map((s) => {
      const box = document.createElement("div");
      box.className = s.trouble ? "server broken" : "server";

      const name = document.createElement("span");
      name.className = "server-name";
      name.textContent = s.name;
      const where = document.createElement("span");
      where.className = "server-from";
      where.textContent = s.from;
      box.append(name, where);

      const what = document.createElement("p");
      what.className = "server-what";
      what.textContent = s.trouble
        ? s.trouble
        : s.tools.length
          ? s.tools.join(", ")
          : "started, but offers nothing";
      box.append(what);
      return box;
    }),
  );
});

/** One line of explanation in a panel, as whichever element belongs there. */
function note(as, text) {
  const line = document.createElement(as);
  line.className = "server-what";
  line.textContent = text;
  return line;
}

/** One line in the panel, for when there is nothing to list. */
function saying(text) {
  return note("p", text);
}

/**
 * Who this agent is, and a way to overrule it.
 *
 * It named itself, and it can be told otherwise: it is somebody's agent, not
 * its own. Opened from its name, which is where anybody would look for this.
 */
el.name.addEventListener("click", () => {
  const t = whose();
  if (!t) return;
  if (!el.whois.hidden) {
    el.whois.hidden = true;
    return;
  }
  el.whoisName.value = t.name === NOT_YET_NAMED ? "" : t.name;
  el.whoisTitle.value = t.title;
  el.whoisAbout.value = t.about;
  el.whois.hidden = false;
  el.whoisName.focus();
});

el.whoisSave.addEventListener("click", async () => {
  const t = whose();
  if (!t) return;
  t.name = el.whoisName.value.trim() || NOT_YET_NAMED;
  t.title = el.whoisTitle.value.trim();
  t.about = el.whoisAbout.value.trim();
  el.whois.hidden = true;
  el.name.textContent = t.name;
  drawThreads();
  await invoke("rename", { id: t.id, name: t.name, title: t.title, about: t.about });
});

el.pin.addEventListener("click", async () => {
  const t = whose();
  if (!t) return;
  t.pinned = !t.pinned;
  drawPinned(t);
  drawThreads();
  await invoke("pin", { id: t.id, pinned: t.pinned });
});

el.hide.addEventListener("click", async () => {
  const t = whose();
  if (!t) return;
  t.hidden = !t.hidden;
  drawPinned(t);
  drawThreads();
  await invoke("hide", { id: t.id, hidden: t.hidden });
});

/** The two toggles, saying which way they are. */
function drawPinned(t) {
  el.pin.textContent = t.pinned ? "Pinned" : "Pin";
  el.pin.setAttribute("aria-pressed", String(t.pinned));
  el.hide.textContent = t.hidden ? "Hidden" : "Hide";
  el.hide.setAttribute("aria-pressed", String(t.hidden));
}

el.talks.addEventListener("change", async () => {
  if (el.talks.value === "+") return alsoAsk();
  await show(el.talks.value);
});

/**
 * A conversation that runs itself.
 *
 * Set on the conversation rather than on the agent, so an agent can have a
 * morning briefing and an ordinary conversation at the same time and the
 * briefing accumulates in one place: yesterday's directly above today's.
 */
el.repeat.addEventListener("click", async () => {
  if (!el.routine.hidden) {
    el.routine.hidden = true;
    return;
  }
  const t = talking();
  if (!t) return;
  const mine = (await invoke("routines")).find((r) => r.conversation === t.id);
  el.routineAt.value = mine?.at || "";
  el.routineWhat.value = mine?.what || "";
  el.routineSays.textContent = sayWhen(mine);
  el.routine.hidden = false;
  el.routineAt.focus();
});

/** When it next runs, in words, or what is wrong with what was typed. */
function sayWhen(routine) {
  if (!routine) return "This runs only when you ask it to.";
  if (!routine.due) return `${routine.at} · nothing due`;
  const due = new Date(routine.due);
  const ran = routine.ran ? ` · last ran ${new Date(routine.ran).toLocaleString()}` : " · never run";
  return `Next ${due.toLocaleString()}${ran}`;
}

el.routineSave.addEventListener("click", async () => {
  const t = talking();
  if (!t) return;
  const at = el.routineAt.value.trim();
  const what = el.routineWhat.value.trim();
  if (!at || !what) {
    el.routineSays.textContent = "It needs both a time and something to do.";
    return;
  }
  try {
    await invoke("runs", { id: t.id, at, what });
  } catch (why) {
    // The schedule is read before it is stored, so an unreadable one is
    // refused here rather than at seven in the morning by not happening.
    el.routineSays.textContent = String(why);
    return;
  }
  const mine = (await invoke("routines")).find((r) => r.conversation === t.id);
  el.routineSays.textContent = sayWhen(mine);
  t.repeats = true;
  drawTalks();
});

el.routineStop.addEventListener("click", async () => {
  const t = talking();
  if (!t) return;
  await invoke("runs", { id: t.id, at: null, what: null });
  el.routineAt.value = "";
  el.routineWhat.value = "";
  el.routineSays.textContent = sayWhen(null);
  t.repeats = false;
  drawTalks();
});

/**
 * What this agent may already do, and how much it asks.
 *
 * The list is the point. An "always" used to go into Claude Code's own
 * settings, where this app could neither show it nor take it back, and an
 * allowlist you cannot read is not a boundary.
 */
el.granted.addEventListener("click", async () => {
  if (!el.granting.hidden) {
    el.granting.hidden = true;
    return;
  }
  await drawGranted();
  el.granting.hidden = false;
});

async function drawGranted() {
  const a = whose();
  if (!a) return;
  el.asks.value = a.asks || "ask";

  const allowed = await invoke("allowances", { agent: a.id });
  el.allowed.replaceChildren(
    ...(allowed.length
      ? allowed.map((one) => {
          const row = document.createElement("li");
          const what = document.createElement("span");
          // A rule is the beginning of what is allowed; nothing means the whole
          // tool, and saying which is the difference between a boundary and a
          // blank cheque.
          what.textContent = one.rule ? `${one.tool} · ${one.rule}` : `${one.tool} · anything`;
          const take = document.createElement("button");
          take.type = "button";
          take.textContent = "Take back";
          take.onclick = async () => {
            await invoke("revoke", { id: one.id });
            await drawGranted();
          };
          row.append(what, take);
          return row;
        })
      : [
          note(
            "li",
            "Nothing yet, so it asks every time. Choose Always on one of its " +
              "questions and the rule appears here, where you can take it back.",
          ),
        ]),
  );
}

el.asks.addEventListener("change", async () => {
  const a = whose();
  if (!a) return;
  a.asks = el.asks.value;
  await invoke("asks", { id: a.id, how: a.asks });
});

el.new.addEventListener("click", start);

// What was here before, and something to type into if there was nothing.
catchUp();
