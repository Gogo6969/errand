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
import { toSay } from "./speech.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

function complain(why) {
  const t = talking();
  if (!t) {
    document.getElementById("thread-name").textContent = why;
    return;
  }
  t.working = false;
  t.writing = "";
  t.messages.push({ kind: "ended", failed: true, text: why });
  drawMessages();
}

// Two maps, because there are two things.
//
// An agent is who; a conversation is what was said. They were one record and
// one `showing` id, which quietly answered two different questions -- whose
// name is in the header, and whose messages are on screen -- and gave the same
// answer to both. That is fine until an agent has a second conversation.
/**
 * Light, dark, or whatever the Mac is set to.
 *
 * Kept in the window's own storage rather than the store, because it is a fact
 * about this screen and not about the work: two people on two machines looking
 * at the same agents should not have to agree about it. Applied before the
 * first draw, so nothing ever flashes the wrong theme on the way in.
 *
 * "System" is the default and stamps nothing, so the palette in the stylesheet
 * follows `prefers-color-scheme` on its own.
 */
const LOOKS = ["system", "dark", "light"];

function looksLike() {
  try {
    const kept = localStorage.getItem("looks");
    return LOOKS.includes(kept) ? kept : "system";
  } catch {
    // A window with no storage still has to draw.
    return "system";
  }
}

function lookLike(how) {
  const root = document.documentElement;
  if (how === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", how);
  try {
    localStorage.setItem("looks", how);
  } catch {
    // Not worth failing over. It will be right until the window closes.
  }
}

lookLike(looksLike());

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
  sweeping: document.getElementById("sweeping"),
  setup: document.getElementById("setup"),
  models: document.getElementById("models"),
  modelsDone: document.getElementById("models-done"),
  atLogin: document.getElementById("at-login"),
  atLoginSays: document.getElementById("at-login-says"),
  lookHere: document.getElementById("look-here"),
  lookWide: document.getElementById("look-wide"),
  findSays: document.getElementById("find-says"),
  found: document.getElementById("found"),
  presets: document.getElementById("presets"),
  byHand: document.getElementById("by-hand"),
  handLabel: document.getElementById("hand-label"),
  handUrl: document.getElementById("hand-url"),
  handKey: document.getElementById("hand-key"),
  handWire: document.getElementById("hand-wire"),
  handSave: document.getElementById("hand-save"),
  handSays: document.getElementById("hand-says"),
  chosen: document.getElementById("chosen"),
  checkup: document.getElementById("checkup"),
  working: document.getElementById("working"),
  costing: document.getElementById("costing"),
  tour: document.getElementById("tour"),
  speak: document.getElementById("speak"),
  call: document.getElementById("call"),
  watch: document.getElementById("watch"),
  watching: document.getElementById("watching"),
  goal: document.getElementById("goal"),
  aiming: document.getElementById("aiming"),
  goalWhat: document.getElementById("goal-what"),
  goalSave: document.getElementById("goal-save"),
  goalStop: document.getElementById("goal-stop"),
  goalSays: document.getElementById("goal-says"),
  watchAt: document.getElementById("watch-at"),
  watchWhat: document.getElementById("watch-what"),
  watchSave: document.getElementById("watch-save"),
  watchStop: document.getElementById("watch-stop"),
  watchAgain: document.getElementById("watch-again"),
  watchSays: document.getElementById("watch-says"),
  attached: document.getElementById("attached"),
  palette: document.getElementById("palette"),
  paletteWhat: document.getElementById("palette-what"),
  paletteList: document.getElementById("palette-list"),
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
  asksMeans: document.getElementById("asks-means"),
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

/** Forget the picker's list, so the next draw reads it again. */
function thePickerHasChanged() {
  couldAnswer = null;
}

/** How one choice is recognised again, since a model id alone does not say where it lives. */
function keyOf(engine, settings) {
  if (engine !== "local") {
    // Claude with a model named is not the same choice as Claude without one.
    // Collapsing them all to "claude" meant the picker could show which engine
    // was answering but never which model, so every one of them looked
    // selected and choosing between them did nothing.
    return settings ? `claude|${settings}` : "claude";
  }
  if (!settings) return "claude";
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

  // An agent on something that is not in the list still has to say what it is
  // on, or the picker quietly claims it is something else.
  //
  // "Not listed" rather than "not running", which is what this used to say and
  // is no longer true: when the picker was a live search, missing meant the
  // server had not answered. Now it means somebody took it out of the list, or
  // never put it in, and the model may be perfectly well. Telling them it is
  // down sends them to go and look at a server that is fine.
  if (!choices.some((c) => keyOf(c.engine, c.settings) === mine)) {
    const gone = document.createElement("option");
    gone.value = mine;
    gone.textContent = `${whatItIsOn(a)} · not in the list`;
    gone.selected = true;
    el.engine.prepend(gone);
  }
}

/** What an agent is on, named the way the picker would name it. */
function whatItIsOn(a) {
  if (a.on !== "local") return a.onSettings ? `Claude · ${a.onSettings}` : "Claude";
  try {
    return JSON.parse(a.onSettings || "{}").model || "a local model";
  } catch {
    return "a local model";
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
  // Not written down and nothing started until something is said to it. An
  // agent somebody made and then thought better of should not survive as a row
  // in a list, and it certainly should not have cost a process.
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
  else {
    await start();
    // Nothing has ever been done in this copy, so there is nothing on screen
    // to read and nothing to work out from. Shown once, here, rather than
    // remembered and shown again: the second time somebody opens this app they
    // have an agent, and this never runs.
    showTheTour();
  }
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
    // Whether the engine behind this is still there, which the stored lines
    // cannot say. A question with nothing written against it is either one
    // nobody will ever answer or one being waited on this second, and those are
    // the same row on disk. An errand started from outside stops at its first
    // question, and drawing that as expired made it unanswerable while the
    // engine sat there waiting.
    const [lines, live] = await Promise.all([
      invoke("lines", { id }),
      invoke("still_going", { id }).catch(() => false),
    ]);
    t.messages = lines.map((line) => fromStore(line, live));
    t.loaded = true;
  }

  // Drawn from what is already known, before anything slow is started. This
  // used to wait on `open_thread` first, which was harmless while that only
  // read a row and became the whole window when it grew to start an engine and
  // bind a socket: for those seconds the header said "Nothing open", both
  // pickers were empty, and nothing anywhere said why. A window must never wait
  // on a subprocess to say what it already knows.
  const a = whose();
  el.whois.hidden = true;
  el.routine.hidden = true;
  el.granting.hidden = true;
  el.watching.hidden = true;
  el.aiming.hidden = true;
  el.costing.hidden = true;
  if (a) {
    drawMark(a);
    drawPinned(a);
    el.name.textContent = a.name;
    drawEngines(a);
  }
  drawTalks();
  drawThreads();
  drawMessages();

  // Nothing is started by looking. The engine is handed back its own memory of
  // this conversation when there is something to say to it, which is the first
  // moment it has anything to do.
  //
  // It used to start here, and that cost more than a process: resuming does
  // not only reload a transcript. A message an engine was sent and killed
  // before finishing is queued inside its own session and runs again on the
  // next resume, so opening the window ran an errand nobody had asked for that
  // minute, and ran it again on every restart until one was left alone long
  // enough to finish.
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

/**
 * One stored line, as the page holds it.
 *
 * `live` says whether the engine behind this conversation is still there, which
 * decides what an unanswered question means.
 */
function fromStore(line, live = false) {
  switch (line.kind) {
    case "mine":
    case "said":
      return { kind: line.kind, text: line.text, seq: line.seq };
    case "asking":
      // A question that was answered is settled history. One that was not is
      // either still being waited on, or one nobody will ever answer because
      // the process that asked it is gone -- the same row on disk, and only
      // whether the engine is still there tells them apart.
      return {
        kind: "asking",
        seq: line.seq,
        text: line.text,
        tool: line.tool,
        step: line.call,
        answered:
          line.outcome ||
          (live ? "" : "That question expired when the thread closed."),
      };
    case "doing":
      return {
        kind: "doing",
        seq: line.seq,
        text: line.text,
        tool: line.tool,
        call: line.call,
        outcome: line.outcome || "",
      };
    default:
      return { kind: "ended", failed: line.kind === "ended", text: line.text, seq: line.seq };
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
  // What is being written this second, under everything already said. Dots
  // while there are no words yet, because dots say "working" and an empty box
  // says nothing.
  if (t.writing) {
    const writing = document.createElement("li");
    writing.className = "said writing";
    writing.textContent = t.writing;
    el.messages.append(writing);
  } else if (t.working) {
    el.messages.append(thinking());
  }
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
    //
    // With the same row of things to do with it as an answer has, because
    // going back to something you said and saying it differently is the whole
    // of a rewind, and this is the line somebody points at to do it.
    case "mine": {
      node.className = "mine";
      const words = document.createElement("span");
      words.className = "mine-words";
      words.textContent = m.text;
      node.append(words, doneWith(m));
      return node;
    }
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
    if (m.can_remember) {
      // What it will allow, on the button. "Always" on its own is not a choice
      // anybody can make: for a shell command it now allows every use of that
      // program, which is a real widening and has to be visible before it is
      // pressed rather than discoverable afterwards in a list.
      const always = say(m.allows ? `Always · ${m.allows}` : "Always", "always", "always");
      always.title = m.allows
        ? `From now on this agent may do ${m.allows} without asking. You can take it back under Allowed.`
        : "";
      choices.append(always);
    }

    // Where the friction actually is. Somebody who has answered this three
    // times is not weighing each question, they are clicking through them, and
    // an app that watches that happen and says nothing has decided the setting
    // it has is somebody else's problem to find. It is behind a button called
    // Allowed, which is not where anybody looks while being interrupted.
    const answeredAlready = (talking()?.messages || []).filter(
      (one) => one.kind === "asking" && one.answered,
    ).length;
    if (answeredAlready >= 3) {
      const enough = document.createElement("button");
      enough.type = "button";
      enough.className = "enough";
      enough.textContent = "Stop asking me";
      enough.title =
        "Set this agent to get on with it without asking. It is walled into its " +
        "own folder when you do, so it can write there and in the usual temporary " +
        "places and nowhere else.";
      enough.onclick = () => {
        el.granting.hidden = false;
        drawGranted();
        el.asks.focus();
      };
      choices.append(enough);
    }
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
  m.answered = said === "always"
    ? `You said yes, and to allow ${m.allows || "this"} from now on`
    : `You said ${label.toLowerCase()}`;
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

/**
 * A line the app itself put in, rather than an agent or a person.
 *
 * A goal ending is neither of those, and it is the one line that says why
 * nothing more is going to happen. Waiting for somebody to click away and back
 * before it appears would hide it at exactly the moment it is about.
 */
listen("noted", ({ payload }) => {
  const t = talks.get(payload.conversation);
  if (!t) return;
  // Through the same reader a reload goes through, so a line that arrives live
  // and the same line read back tomorrow are the same thing. Pushed straight in
  // as its own kind, it drew as nothing at all live and drew fine after a
  // reload, which is the worst way round.
  t.messages.push(fromStore(payload));
  if (showing === payload.conversation) drawMessages();
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

    // Only settled lines are kept. The partial ones are shown and thrown away:
    // a sentence being written should look like one, and keeping them all would
    // keep every prefix of every sentence.
    //
    // They used to be dropped without being shown, which left several seconds
    // of dots and then a wall of text -- the exact thing this app says reads as
    // a hang rather than as thinking.
    case "said":
      if (payload.settled) {
        t.writing = "";
        t.messages.push({ kind: "said", text: payload.text, seq: payload.seq });
        // In a call it is also read out, as each line settles rather than all
        // at once at the end: the first paragraph is spoken while the second
        // is still being written.
        if (inACall && payload.conversation === showing) sayOutLoud(payload.text);
      } else {
        t.writing = (t.writing || "") + payload.text;
      }
      break;

    case "doing":
      t.messages.push({
        kind: "doing",
        text: payload.what,
        tool: payload.tool,
        call: payload.call,
        seq: payload.seq,
      });
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
        // What that would cover, in words, so the button can say it rather than
        // leaving somebody to guess how wide "always" is.
        allows: payload.allows || "",
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
      // Half a sentence must not outlive the turn writing it. Normally the
      // settled line has already cleared this; a turn stopped mid-word has not.
      t.writing = "";
      // Either end of the turn can finish last: a short answer is read out
      // before the turn ends and a long one is still being read after it.
      if (inACall && payload.conversation === showing) listenAgain();
      // Naming is the agent's own job now, asked for after this by the app.
      break;

    case "failed":
      t.working = false;
      t.writing = "";
      // A call that goes quiet after a failure sounds like one that hung up.
      if (inACall && payload.conversation === showing) {
        sayOutLoud(payload.why || "It could not finish.");
      }
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

  // Carrying on from here is the answer to the limit below. "Ask again" can
  // only be offered on the last answer, because a reply to something halfway
  // up would land at the bottom under everything that came after it. This
  // makes halfway up the thread the bottom of somewhere else instead, and
  // takes nothing away from where it came from.
  //
  // Only on lines that were written down. A message still on its way has no
  // position in the conversation yet, so there is no "here" to carry on from.
  if (typeof m.seq === "number") {
    const onward = document.createElement("button");
    onward.type = "button";
    onward.textContent = "From here";
    onward.title = "Carry on in a new conversation, leaving this one alone";
    onward.onclick = () => carryOn(m.seq, m.kind === "mine" ? m.text : "");
    row.append(onward);
  }

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

/**
 * Pictures waiting to go with the next thing said.
 *
 * Held apart from the text rather than pasted into it as a path, because a
 * path in the box is a thing somebody has to not delete by accident, and a
 * pasted image has no path at all. Cleared when it is sent, so an image never
 * goes twice.
 */
let attached = [];

function drawAttached() {
  el.attached.hidden = attached.length === 0;
  el.attached.replaceChildren(
    ...attached.map((one, at) => {
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "chip";
      chip.textContent = one.name;
      chip.title = "Take this off again";
      chip.onclick = () => {
        attached.splice(at, 1);
        drawAttached();
      };
      return chip;
    }),
  );
}

el.form.addEventListener("submit", (e) => {
  e.preventDefault();
  const text = el.what.value.trim();
  // A picture on its own is a question: "what is this". So something has to be
  // said, but it does not have to be typed.
  if (!text && !attached.length) return;
  el.what.value = "";
  el.what.style.height = "auto";
  sayIt(text || "What is this?");
});

/**
 * Anything pasted that is a picture rather than words.
 *
 * A screenshot on the clipboard has no filename and never touches disk, so
 * this is the only way one ever arrives. Read here into a data URL, because
 * the window is the only place that can see it.
 */
el.what.addEventListener("paste", (e) => {
  const pictures = [...(e.clipboardData?.items || [])].filter((one) =>
    one.type.startsWith("image/"),
  );
  if (!pictures.length) return;
  e.preventDefault();
  for (const one of pictures) {
    const file = one.getAsFile();
    if (!file) continue;
    const read = new FileReader();
    read.onload = () => {
      attached.push({ name: file.name || "pasted picture", url: String(read.result) });
      drawAttached();
    };
    read.readAsDataURL(file);
  }
});

/** Say something to the thread that is open, from wherever it was typed. */
async function sayIt(text) {
  const t = talking();
  if (!t) return;
  const going = attached.map((one) => one.url);
  const withThem = going.length
    ? `${text}\n\n(with ${going.length === 1 ? "a picture" : `${going.length} pictures`})`
    : text;
  attached = [];
  drawAttached();
  const mine = { kind: "mine", text: withThem };
  t.messages.push(mine);
  // Working from the moment it is sent, not from the moment something comes
  // back: the gap between the two is exactly when a person wonders whether the
  // thing they typed went anywhere.
  t.working = true;
  drawMessages();
  drawThreads();

  try {
    // Stamped with where it landed, so it can be carried on from without
    // waiting for a reload. Optimistic on the way out and corrected on the way
    // back, because the alternative is a message that sits there unshown until
    // the store has answered.
    mine.seq = await invoke("say", { id: t.id, text, attached: going.length ? going : null });
    drawMessages();
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
    // Whatever was answering has been stopped. The new one starts when there
    // is something to say to it, which is the only moment it has any work.
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

    // A picture is attached; anything else is still a path in the box, which
    // is what dropping a file did before pictures were understood and is
    // still the right thing for a spreadsheet or a folder.
    const looksLikeAPicture = /\.(png|jpe?g|gif|webp)$/i;
    const pictures = payload.paths.filter((p) => looksLikeAPicture.test(p));
    const rest = payload.paths.filter((p) => !looksLikeAPicture.test(p));

    for (const path of pictures) {
      attached.push({ name: path.split("/").pop(), url: path });
    }
    if (pictures.length) drawAttached();

    if (rest.length) {
      const already = el.what.value.trim();
      el.what.value = already ? `${already}\n${rest.join("\n")}` : rest.join("\n");
      el.what.dispatchEvent(new Event("input"));
    }
    el.what.focus();
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

  // What the engine itself turned up with, above the servers the app gives it.
  // Same question, two sources: an agent can reach these and they are not
  // ours, which until now was true and invisible.
  let kit = {};
  try {
    kit = await invoke("brought", { id: showing });
  } catch {
    // An engine that has not started yet has nothing to say, which is not a
    // failure worth showing.
  }
  const lists = [
    ["skills", "Skills"],
    ["helpers", "Kinds of helper"],
    ["plugins", "Plugins"],
    ["commands", "Commands"],
  ];
  // Claude Code says what it has on its first turn and not before, so a
  // conversation that has been opened and not yet spoken to has nothing here.
  // Said rather than left blank: a section that is silently absent looks like
  // an engine that brought nothing, which for Claude Code is untrue by about
  // sixty skills.
  const engine = whose()?.on || "claude";
  const nothingYet =
    engine !== "local" && lists.every(([which]) => !(kit[which] || []).length);

  const itsOwn = lists
    .filter(([which]) => (kit[which] || []).length)
    .map(([which, called]) => {
      const box = document.createElement("div");
      box.className = "server";
      const name = document.createElement("span");
      name.className = "server-name";
      name.textContent = called;
      const from = document.createElement("span");
      from.className = "server-from";
      from.textContent = `${kit[which].length}, from the engine`;
      const what = document.createElement("p");
      what.className = "server-what";
      // All of them, because a list that stops at ten is a list somebody has
      // to go elsewhere to finish reading.
      what.textContent = kit[which].join(", ");
      box.append(name, from, what);
      return box;
    });

  el.reachable.replaceChildren(
    ...itsOwn,
    ...(nothingYet
      ? [
          note(
            "p",
            "What the engine itself brought will appear here once it has answered " +
              "something. Claude Code says what skills, helpers and plugins it has " +
              "on its first turn, not when it starts.",
          ),
        ]
      : []),
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
function note(as, text, looks = "server-what") {
  const line = document.createElement(as);
  line.className = looks;
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
  theRoutineShown = mine || null;
  el.routineAt.value = mine?.at || "";
  el.routineWhat.value = mine?.what || "";
  el.routineSays.textContent = sayWhen(mine);
  el.routine.hidden = false;
  el.routineAt.focus();
});

/**
 * Whether something else already does this, said while it is being typed.
 *
 * Before saving rather than after, because two agents on the same job every
 * morning is a thing somebody wants to know about while deciding, not to
 * discover later from two identical briefings. Nothing is blocked: it is
 * usually a mistake and occasionally exactly what somebody meant.
 */
async function sayIfSomethingElseAlreadyDoesThis() {
  const t = talking();
  const at = el.routineAt.value.trim();
  const what = el.routineWhat.value.trim();
  if (!t || !at || !what) return;
  let already;
  try {
    already = await invoke("already_runs", { id: t.id, at, what });
  } catch {
    return;
  }
  // Written whether or not there is a clash, never only when there is. Setting
  // it only on a clash leaves the last clash on screen after the text has been
  // changed to something that does not clash, which is a warning about a
  // routine nobody is proposing any more.
  //
  // Added to what the panel already says rather than replacing it: when this
  // one would next run is the thing somebody opened the panel to see.
  el.routineSays.textContent = already
    ? `${sayWhen(theRoutineShown)} ${already}`
    : sayWhen(theRoutineShown);
}

/** The routine as last read, so the warning can be added to what it says. */
let theRoutineShown = null;

el.routineAt.addEventListener("input", sayIfSomethingElseAlreadyDoesThis);
el.routineWhat.addEventListener("input", sayIfSomethingElseAlreadyDoesThis);

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
  theRoutineShown = mine || null;
  el.routineSays.textContent = sayWhen(mine);
  // Said again after saving, because somebody who went ahead anyway should not
  // then be told it is fine.
  await sayIfSomethingElseAlreadyDoesThis();
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
  sayWhatAsksMeans();

  const allowed = await invoke("allowances", { agent: a.id });
  el.allowed.replaceChildren(
    ...(allowed.length
      ? allowed.map((one) => {
          const row = document.createElement("li");
          const what = document.createElement("span");
          // What it covers, in words, rather than the rule it is stored as. A
          // rule that is a whole command line covers that line and nothing
          // else, which reads like a permission and behaves like a one-off, and
          // nothing about the line says which of the two it is.
          what.textContent = `${one.tool} · ${one.covers}`;
          if (one.rule) what.title = one.rule;
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

/**
 * What choosing this actually does, said next to the choice.
 *
 * "Never" is the one that needs saying. Switching the asking off does not leave
 * an agent with nothing between it and the machine: it leaves it walled into
 * its own folder, because asking and a wall are the two mechanisms there are
 * and turning one off is when the other has to be on. Somebody choosing it
 * should know both halves before they choose, not discover the second half as a
 * refused write later.
 */
function sayWhatAsksMeans() {
  el.asksMeans.textContent =
    {
      plan: "It reads, looks things up and comes back with what it would do. It changes nothing.",
      ask: "It stops and asks before anything that changes something. Your answer can become a rule below.",
      edits: "It writes files in its own folder without asking, and stops for everything else.",
      auto:
        "It never asks. Nobody is going to say no, so it is walled into its own folder instead: " +
        "it can write there and in the usual temporary places, and nowhere else on this Mac.",
    }[el.asks.value] || "";
}

el.asks.addEventListener("change", async () => {
  const a = whose();
  if (!a) return;
  a.asks = el.asks.value;
  sayWhatAsksMeans();
  await invoke("asks", { id: a.id, how: a.asks });
});

el.new.addEventListener("click", start);

// What was here before, and something to type into if there was nothing.
catchUp();


/* ------------------------------------------------------------ palette -- */

/**
 * Everything the app can do, in one place you can type at.
 *
 * The header holds about nine controls before the last one falls off the end,
 * and there are more than nine things worth doing. Rather than keep adding
 * buttons until that happens again, everything else lives here and is found by
 * typing a word of it.
 *
 * Built fresh each time it opens rather than kept in a list: half of these
 * depend on what is open, and a menu offering to export a conversation when
 * none is open is a menu that lies.
 */
function whatCouldBeDone() {
  const a = whose();
  const t = talking();
  const could = [];
  const add = (what, why, run, when = true) => {
    if (when) could.push({ what, why, run });
  };

  add("Export this conversation", "to the Desktop", async () => {
    const onto = await invoke("export_conversation", { id: showing });
    complain(`Saved to ${onto}`);
  }, !!showing);
  add("New conversation with this agent", a?.name || "", () => alsoAsk(), !!a);
  add("New agent", "", () => start());
  add("Search everything", "", () => el.find.focus());
  add(a?.pinned ? "Unpin this agent" : "Pin this agent", "", () => el.pin.click(), !!a);
  add(a?.hidden ? "Show this agent" : "Hide this agent", "", () => el.hide.click(), !!a);
  add("What it may do without asking", "", () => el.granted.click(), !!a);
  add("What this thread can reach", "MCP servers", () => el.reach.click(), !!showing);
  add("Make this run on a schedule", "", () => el.repeat.click(), !!showing);
  add("Which models show up", "", () => showModels(), true);
  add("What Errand is", `the whole thing, in ${WHAT_THIS_IS.length} lines`, () => showTheTour(), true);
  add("What it has cost", "today and this month", () => whatItCost(), true);
  add("Check this setup", "what is wrong, and what to do", () => checkup());
  add("What is running", "everywhere, not just here", () => whatsRunning());
  add(
    "Carry this on in a new conversation",
    "leaves this one alone",
    () => carryOn(null, ""),
    !!showing,
  );
  for (const how of LOOKS) {
    add(
      `Look ${how === "system" ? "however the Mac does" : how}`,
      looksLike() === how ? "in use" : "",
      () => lookLike(how),
      looksLike() !== how,
    );
  }
  add(
    "Stop what it is doing",
    "",
    async () => {
      const stopping = showing;
      await invoke("stop", { id: stopping });
      // Nothing else will say it stopped. A turn ends in the window when an
      // ending arrives from the engine, and an engine that was killed never
      // sends one, so without this the conversation goes on saying "Working"
      // and offering to stop something that stopped minutes ago.
      const talk = talks.get(stopping);
      if (talk) talk.working = false;
      if (showing === stopping) drawMessages();
      drawThreads();
      drawTalks();
    },
    !!t?.working,
  );
  return could;
}

let offered = [];
let picked = 0;

function openPalette() {
  el.palette.hidden = false;
  el.paletteWhat.value = "";
  drawPalette();
  el.paletteWhat.focus();
}

function closePalette() {
  el.palette.hidden = true;
  el.what.focus();
}

function drawPalette() {
  const typed = el.paletteWhat.value.trim().toLowerCase();
  // Every word has to appear somewhere, in any order, so "export conv" finds
  // "Export this conversation" without anybody having to remember the wording.
  offered = whatCouldBeDone().filter((one) =>
    typed
      .split(/\s+/)
      .filter(Boolean)
      .every((word) => `${one.what} ${one.why}`.toLowerCase().includes(word)),
  );
  picked = 0;

  if (!offered.length) {
    const none = document.createElement("li");
    none.className = "none";
    none.textContent = "Nothing here matches that.";
    el.paletteList.replaceChildren(none);
    return;
  }

  el.paletteList.replaceChildren(
    ...offered.map((one, at) => {
      const row = document.createElement("li");
      row.setAttribute("aria-selected", String(at === picked));
      const what = document.createElement("span");
      what.className = "what";
      what.textContent = one.what;
      row.append(what);
      if (one.why) {
        const why = document.createElement("span");
        why.className = "why";
        why.textContent = one.why;
        row.append(why);
      }
      row.onclick = () => run(at);
      return row;
    }),
  );
}

function highlight() {
  [...el.paletteList.children].forEach((row, at) =>
    row.setAttribute("aria-selected", String(at === picked)),
  );
  el.paletteList.children[picked]?.scrollIntoView({ block: "nearest" });
}

async function run(at) {
  const one = offered[at];
  if (!one) return;
  // Closed first. Half of these open a panel, and a palette still covering it
  // would hide the thing somebody just asked for.
  closePalette();
  try {
    await one.run();
  } catch (why) {
    complain(String(why));
  }
}

el.paletteWhat.addEventListener("input", drawPalette);
el.palette.addEventListener("mousedown", (e) => {
  if (e.target === el.palette) closePalette();
});

window.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    el.palette.hidden ? openPalette() : closePalette();
    return;
  }
  if (el.palette.hidden) return;
  if (e.key === "Escape") {
    e.preventDefault();
    closePalette();
  } else if (e.key === "ArrowDown") {
    e.preventDefault();
    picked = Math.min(picked + 1, offered.length - 1);
    highlight();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    picked = Math.max(picked - 1, 0);
    highlight();
  } else if (e.key === "Enter") {
    e.preventDefault();
    run(picked);
  }
});


/**
 * What is wrong with this setup, asked all at once.
 *
 * Everything it reports is something that has gone wrong here and was invisible
 * from inside a conversation: a tool server whose interpreter an upgrade had
 * removed, a model server bound so that nothing on the network could see it,
 * rows left pointing at a conversation that no longer existed. None of those
 * announces itself. All of them are one question away.
 */
async function checkup() {
  if (!el.checkup.hidden) {
    el.checkup.hidden = true;
    return;
  }
  el.checkup.hidden = false;
  el.checkup.replaceChildren(note("p", "Looking…"));

  let found;
  try {
    found = await invoke("checkup");
  } catch (why) {
    el.checkup.replaceChildren(note("p", String(why)));
    return;
  }

  // What only the window can answer. The app can see the machine; it cannot
  // see what this particular webview will let a page do, and the two together
  // are what somebody means by "is this set up right".
  found = found.concat(whatThisWindowCanDo());

  const wrong = found.filter((f) => f.how !== "fine").length;
  el.checkup.replaceChildren(
    note(
      "p",
      wrong
        ? `${wrong} of ${found.length} things want attention.`
        : `All ${found.length} checks are fine.`,
    ),
    ...found.map((f) => {
      const box = document.createElement("div");
      box.className = "finding";
      box.dataset.how = f.how;

      const what = document.createElement("span");
      what.className = "finding-what";
      what.textContent = f.what;
      const said = document.createElement("p");
      said.className = "finding-said";
      said.textContent = f.said;
      box.append(what, said);

      // The half that makes it worth reading. A check that says something is
      // wrong and leaves somebody to work out the fix has done the easy part.
      if (f.fix) {
        const fix = document.createElement("p");
        fix.className = "finding-fix";
        fix.textContent = f.fix;
        box.append(fix);
      }
      return box;
    }),
  );
}


/**
 * Carry this conversation on somewhere else, from a point in it.
 *
 * The one gesture behind two things somebody would name separately. Carrying
 * on from the end is "try something without disturbing this"; carrying on from
 * your own message is "go back and say that differently", and it puts those
 * words back in the box for you to edit. Nothing is removed from where it came
 * from either way, which is what makes going back safe enough to do casually.
 */
async function carryOn(upTo, saidAgain) {
  const from = showing;
  if (!from) return;
  const id = uuid();
  try {
    await invoke("carry_on", { id, from, upTo });
  } catch (why) {
    complain(String(why));
    return;
  }
  // Read back rather than assumed, because the name is decided in the store:
  // it has to not collide with one this agent already has.
  const theirs = (await invoke("conversations", { agent: showingAgent })).map((c) =>
    asTalk(c, talks.get(c.id)),
  );
  for (const t of theirs) talks.set(t.id, t);
  await show(id);
  if (saidAgain) {
    el.what.value = saidAgain;
    el.what.dispatchEvent(new Event("input"));
    el.what.select();
  }
  el.what.focus();
}


/**
 * Everything working right now, wherever it is happening.
 *
 * The window knows only about conversations somebody has opened, and the work
 * worth being able to see is exactly the work happening somewhere nobody is
 * looking: a routine firing at seven, an agent answering another agent. So the
 * app is asked rather than the page working it out.
 */
async function whatsRunning() {
  if (!el.working.hidden) {
    el.working.hidden = true;
    return;
  }
  el.working.hidden = false;
  el.working.replaceChildren(note("p", "Looking…"));

  let going;
  try {
    going = await invoke("whats_running");
  } catch (why) {
    el.working.replaceChildren(note("p", String(why)));
    return;
  }

  if (!going.length) {
    el.working.replaceChildren(note("p", "Nothing is running. Everything has finished."));
    return;
  }

  const waiting = going.filter((one) => one.waiting).length;
  el.working.replaceChildren(
    note(
      "p",
      waiting
        ? `${going.length} running, ${waiting} stopped waiting on you.`
        : `${going.length} running.`,
    ),
    ...going.map((one) => {
      const row = document.createElement("div");
      row.className = "one";
      row.dataset.waiting = String(one.waiting);
      row.title = one.command ? "A command that is still running" : "Open it";
      // Opening it is the thing anybody wants next, and it is the only way to
      // answer one that has stopped to ask.
      row.onclick = async () => {
        // A command has no turn to open, and clicking through to a conversation
        // that has moved on would be a lie about where the work is.
        if (one.command) return;
        el.working.hidden = true;
        if (!talks.has(one.conversation)) {
          const theirs = (await invoke("conversations", { agent: one.agent })).map((c) =>
            asTalk(c, talks.get(c.id)),
          );
          for (const t of theirs) talks.set(t.id, t);
        }
        await show(one.conversation);
      };

      const who = document.createElement("span");
      who.className = "who";
      who.textContent = one.who;
      const where = document.createElement("span");
      where.className = "where";
      where.textContent = one.talk;
      const what = document.createElement("p");
      what.className = "what";
      what.textContent = one.what;
      row.append(who, where, what);

      // A command left running is the one kind of work here that can be stopped
      // on its own, so it is the one kind that offers to be.
      if (one.command) {
        row.dataset.command = one.command;
        const stop = document.createElement("button");
        stop.className = "stop-command";
        stop.type = "button";
        stop.textContent = "Stop";
        stop.title = `Stop ${one.command}`;
        stop.onclick = async (e) => {
          e.stopPropagation();
          stop.disabled = true;
          stop.textContent = "Stopping…";
          await invoke("stop_a_command", { handle: one.command });
          // Asked again rather than the row being removed, so what is shown is
          // what is running rather than what this page believes is running.
          el.working.hidden = true;
          whatsRunning();
        };
        row.append(stop);
      }
      return row;
    }),
  );
}


/**
 * What only the window can answer about this setup.
 *
 * The app knows about the machine. It cannot know what this particular webview
 * will let a page do, and a capability that is quietly missing here looks from
 * the outside like a feature nobody built.
 */
function whatThisWindowCanDo() {
  const found = [];
  const say = (what, how, said, fix = "") => found.push({ what, how, said, fix });

  const listens = window.SpeechRecognition || window.webkitSpeechRecognition;
  say(
    "Dictation",
    listens ? "fine" : "odd",
    listens
      ? "this window can turn speech into words"
      : "this window cannot turn speech into words",
    listens
      ? ""
      : "Dictating an errand is not available here. macOS dictation still works: " +
        "put the cursor in the box and press the dictation key.",
  );

  const hears = !!navigator.mediaDevices?.getUserMedia;
  say(
    "Microphone",
    hears ? "fine" : "odd",
    hears ? "this window may ask for it" : "this window cannot ask for it",
    hears ? "" : "Anything needing the microphone will not work.",
  );

  const remembers = (() => {
    try {
      localStorage.setItem("probe", "1");
      localStorage.removeItem("probe");
      return true;
    } catch {
      return false;
    }
  })();
  say(
    "Remembering how you like it",
    remembers ? "fine" : "odd",
    remembers ? "kept between launches" : "cannot be kept",
    remembers ? "" : "The light or dark choice will go back to following the Mac each time.",
  );

  return found;
}


/* -------------------------------------------------------------- speak -- */

/**
 * Dictating an errand instead of typing it.
 *
 * An errand is a thing you hand over and walk away from, and saying one out
 * loud suits that better than typing it does. This window can turn speech into
 * words, which is not true of every webview, so the button is not there at all
 * where it would do nothing: a control that silently fails is worse than one
 * that is absent, and the setup check says why it is absent.
 *
 * What is heard goes into the box rather than being sent. Speech recognition
 * mishears, and sending on silence would mean an errand nobody read leaving
 * before it could be corrected.
 */
const Listening = window.SpeechRecognition || window.webkitSpeechRecognition;
let ears = null;

if (Listening) {
  el.speak.hidden = false;
  el.speak.setAttribute("aria-pressed", "false");
}

function stopListening() {
  if (!ears) return;
  const going = ears;
  ears = null;
  el.speak.setAttribute("aria-pressed", "false");
  try {
    going.stop();
  } catch {
    // Already stopped, which is the thing we wanted.
  }
}

/**
 * Start listening, and say where what is heard should go.
 *
 * One set of ears for both things that want them. Dictation puts words in the
 * box and stops there; a call does the same and then sends on a pause, which
 * is the only difference between the two and is worth it being the only one.
 */
function startListening({ sendOnPause = false } = {}) {
  if (ears) return true;

  const hearing = new Listening();
  hearing.continuous = true;
  hearing.interimResults = true;
  // The language the Mac is set to, because an errand is dictated in whatever
  // somebody actually speaks and the default is not always that.
  hearing.lang = navigator.language || "en-US";

  // What was in the box before, kept whole. Dictation adds to what somebody
  // has typed rather than replacing it, so half a typed errand can be
  // finished out loud.
  const already = el.what.value;
  let settled = "";

  hearing.onresult = (e) => {
    let saying = "";
    let finished = false;
    for (let i = e.resultIndex; i < e.results.length; i += 1) {
      const heard = e.results[i][0].transcript;
      if (e.results[i].isFinal) {
        settled += heard;
        finished = true;
      } else saying += heard;
    }
    // The unsettled part is shown too, so a long sentence looks like it is
    // being heard rather than like nothing is happening.
    const joined = [already.trim(), (settled + saying).trim()].filter(Boolean).join(" ");
    el.what.value = joined;
    el.what.dispatchEvent(new Event("input"));

    // In a call, a pause is how somebody finishes talking. Restarted on every
    // result rather than set once, because a sentence with a breath in the
    // middle of it is one sentence and sending half of it is worse than
    // waiting.
    if (!sendOnPause) return;
    clearTimeout(waitingForAPause);
    if (!finished) return;
    waitingForAPause = setTimeout(() => {
      if (inACall && el.what.value.trim()) el.form.requestSubmit();
    }, ENOUGH_OF_A_PAUSE);
  };

  // Any of these means it has stopped, whether or not anybody asked it to.
  //
  // Recognition gives up on its own after a stretch of quiet, which in
  // dictation is fine and in a call is the call dying while somebody is still
  // thinking about what to ask. So in a call it starts again, and only while
  // the call is actually waiting to hear something: restarting while the
  // answer is being spoken is how it transcribes its own voice.
  hearing.onend = () => {
    stopListening();
    if (inACall && itsYourTurn === "listening") startListening({ sendOnPause: true });
  };
  hearing.onerror = (e) => {
    // Whatever went wrong, it went wrong with the ears, and a call with no
    // ears is somebody talking to a window that cannot hear them.
    if (inACall) endTheCall();
    stopListening();
    // The one worth saying out loud: a refusal is permanent until somebody
    // changes it in System Settings, and it looks exactly like a broken button.
    if (e.error === "not-allowed" || e.error === "service-not-allowed") {
      complain(
        "Errand was not allowed to use the microphone. Turn it on for Errand in " +
          "System Settings, under Privacy and Security.",
      );
    }
  };

  try {
    hearing.start();
    ears = hearing;
    el.speak.setAttribute("aria-pressed", "true");
    return true;
  } catch (why) {
    complain(String(why));
    return false;
  }
}

el.speak.addEventListener("click", () => {
  if (ears) {
    stopListening();
    el.what.focus();
    return;
  }
  startListening();
});

// Sending ends the dictation. Carrying on listening into the next errand is
// how somebody ends up dictating a reply they meant to think about.
//
// In a call it also stops, and starts again when the answer has been spoken.
// Listening through the answer means hearing its own voice and sending that
// back as the next thing said.
el.form.addEventListener("submit", () => {
  clearTimeout(waitingForAPause);
  if (inACall) itsYourTurn = "working";
  stopListening();
});


/* --------------------------------------------------------------- call --- */

/**
 * Talking to it with your hands somewhere else.
 *
 * Dictation is still typing: it puts words in the box and waits to be sent.
 * A call is the thing somebody actually wants while cooking or driving, and
 * it is three differences from dictation, not thirty. It sends when you stop
 * talking, it reads the answer out, and then it listens again.
 *
 * The transcript is the conversation itself. Nothing separate is drawn for it,
 * because a second copy of what was said that scrolls on its own is one more
 * thing to look at in the one situation where somebody is not looking.
 *
 * Every part of this needs both halves. A window that can hear but not speak
 * cannot hold a call, so the button is not there at all rather than being
 * there and doing half of it.
 */
const Speaking = window.speechSynthesis;

/** How long a silence means somebody has finished a sentence. */
const ENOUGH_OF_A_PAUSE = 1400;

let inACall = false;
/** What the call is doing: listening, working, or speaking. */
let itsYourTurn = "listening";
let waitingForAPause = null;

if (Listening && Speaking) {
  el.call.hidden = false;
  el.call.setAttribute("aria-pressed", "false");
}

function endTheCall() {
  inACall = false;
  itsYourTurn = "listening";
  clearTimeout(waitingForAPause);
  // Mid-sentence if it is talking. Somebody ending a call wants it to stop
  // now, not at the end of the paragraph it is reading.
  try {
    Speaking.cancel();
  } catch {
    // Nothing to cancel, which is the thing we wanted.
  }
  el.call.setAttribute("aria-pressed", "false");
  document.body.classList.remove("in-a-call");
  el.what.placeholder = "What would you like done?";
  stopListening();
}

el.call.addEventListener("click", () => {
  if (inACall) {
    endTheCall();
    return;
  }
  inACall = true;
  itsYourTurn = "listening";
  el.call.setAttribute("aria-pressed", "true");
  document.body.classList.add("in-a-call");
  // What the box says while it is on, because sending on a pause is the one
  // thing in this window that acts without being told to.
  el.what.placeholder = "Talk. It sends when you stop.";
  if (!startListening({ sendOnPause: true })) endTheCall();
});

// The way out that somebody reaches for without thinking. A call is the one
// state in this window where the keyboard is not where their hands are, and
// the one they most want to be able to leave quickly.
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && inACall) endTheCall();
});

/**
 * Read a line of the answer out, and listen again when there is nothing left.
 *
 * Said line by line as they settle rather than all at once at the end, so the
 * first paragraph is being read while the second is still being written. The
 * browser queues them in order, which is the whole of the ordering logic here.
 */
function sayOutLoud(text) {
  const saying = toSay(text);
  if (!saying) return;
  itsYourTurn = "speaking";
  stopListening();

  const utterance = new SpeechSynthesisUtterance(saying);
  utterance.lang = navigator.language || "en-US";
  utterance.onend = listenAgain;
  // A voice that fails silently leaves a call that never listens again, which
  // looks exactly like a call that hung up.
  utterance.onerror = listenAgain;
  Speaking.speak(utterance);
}

/**
 * Back to listening, once there is nothing left to say and nothing left to do.
 *
 * Called from both ends of the turn, because either can finish last: a short
 * answer is read out before the turn ends, and a long one is still being read
 * after it.
 */
function listenAgain() {
  if (!inACall) return;
  if (Speaking.speaking || Speaking.pending) return;
  const t = talking();
  if (t && t.working) return;
  itsYourTurn = "listening";
  startListening({ sendOnPause: true });
}


/* -------------------------------------------------------------- watch -- */

/**
 * A conversation woken by something changing.
 *
 * The third way an errand can start, after somebody typing and the clock. It
 * is the nearest thing to a connector that needs nobody to sign in to
 * anything: a folder gets a file, a page changes its mind, and the agent is
 * told what changed and gets on with it.
 */
el.watch.addEventListener("click", async () => {
  if (!el.watching.hidden) {
    el.watching.hidden = true;
    clearInterval(watchTicking);
    watchTicking = null;
    return;
  }
  if (!showing) return;
  el.watching.hidden = false;
  await drawWatch();
  keepWatchHonest();
  el.watchAt.focus();
});

/**
 * Looking happens on a timer in the background, so a panel drawn once says "it
 * has not looked yet" long after it has, and goes on saying it while the agent
 * it describes is being woken behind it. A description of something live has
 * to be live, or it is not a description, it is a claim.
 */
let watchTicking = null;
function keepWatchHonest() {
  clearInterval(watchTicking);
  watchTicking = setInterval(() => {
    // Only while it is on screen, and only while the same conversation is.
    if (el.watching.hidden || !showing) {
      clearInterval(watchTicking);
      watchTicking = null;
      return;
    }
    drawWatch({ leaveTheFields: true });
  }, 5000);
}

async function drawWatch({ leaveTheFields = false } = {}) {
  if (!showing) return;
  let now;
  try {
    now = await invoke("watches", { id: showing });
  } catch (why) {
    el.watchSays.textContent = String(why);
    return;
  }
  // On a redraw the sentence is always refreshed and the boxes are not, so a
  // half-typed path is never taken back while it is being typed. The field
  // takes focus the moment the panel opens, so guarding the whole redraw on
  // that silenced it altogether.
  if (!leaveTheFields) {
    el.watchAt.value = now.watches || "";
    el.watchWhat.value = now.what || "";
  }
  el.watchAgain.hidden = !now.paused;
  el.watchSays.dataset.paused = String(!!now.paused);

  // Stopped is the thing to say first, because it is the only state somebody
  // has to do something about.
  if (now.paused) {
    el.watchSays.textContent = now.paused;
    return;
  }
  if (!now.watches) {
    el.watchSays.textContent =
      "Nothing is watched. Name a folder, a file or a page, and how often to look.";
    return;
  }
  const when = (at) => (at ? new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : null);
  const looked = when(now.looked_at);
  const woke = when(now.woke_at);
  // A look that failed is said now, not after the fifth one. Four silent
  // failures before a watch admits anything is the shape of quiet failure this
  // whole app is written against.
  const failing =
    now.misses > 0
      ? `The last ${now.misses === 1 ? "look" : `${now.misses} looks`} failed. It stops after five.`
      : null;
  el.watchSays.dataset.paused = String(now.misses > 0);
  el.watchSays.textContent = [
    failing,
    now.means,
    looked ? `Last looked at ${looked}.` : "It has not looked yet.",
    woke ? `Last woke this at ${woke}, ${now.woke_today} today.` : "It has not woken this yet.",
  ]
    .filter(Boolean)
    .join(" ");
}

/**
 * Something to get to, rather than something to do.
 *
 * The fourth way an errand can start, and the only one where the agent decides
 * the steps. Everything else here is a request in some form: typed, on a
 * schedule, or when the world changes. A goal is a description of what being
 * finished looks like, and it keeps going on the strength of the agent's own
 * account of where it has got to, which it gives at the end of every turn.
 *
 * Changeable while it is running, on purpose. Watching an agent work is the
 * fastest way to find out the goal was the wrong one, and being able to say so
 * without starting again is the difference between telling something what to do
 * and working something out with it.
 */
el.goal.addEventListener("click", async () => {
  if (!el.aiming.hidden) {
    el.aiming.hidden = true;
    clearInterval(goalTicking);
    goalTicking = null;
    return;
  }
  if (!showing) return;
  el.aiming.hidden = false;
  await drawGoal();
  keepGoalHonest();
  el.goalWhat.focus();
});

// A goal moves on its own, turn by turn, so a panel drawn once is a panel that
// is wrong within a minute. The same reason the watch panel re-reads.
let goalTicking = null;
function keepGoalHonest() {
  clearInterval(goalTicking);
  goalTicking = setInterval(() => {
    if (el.aiming.hidden || !showing) {
      clearInterval(goalTicking);
      goalTicking = null;
      return;
    }
    drawGoal({ leaveTheField: true });
  }, 4000);
}

async function drawGoal({ leaveTheField = false } = {}) {
  if (!showing) return;
  let now;
  try {
    now = await invoke("goal_of", { id: showing });
  } catch (why) {
    el.goalSays.textContent = String(why);
    return;
  }
  if (!leaveTheField) el.goalWhat.value = now.goal || "";
  el.goalStop.hidden = !now.goal;
  el.goalSave.textContent = now.goal ? "Change it" : "Start";
  el.goalSays.dataset.over = String(!!now.over);

  if (!now.goal) {
    el.goalSays.textContent =
      "No goal. Say what being finished looks like, and it works out the steps itself.";
    return;
  }
  // Where it has got to, in numbers, said the same way whether it is going
  // well or badly. A progress line that only appears when something is wrong
  // is one nobody trusts when it does appear.
  const over = {
    done: "It finished.",
    "going round": "It stopped: it said the same thing was left twice running.",
    "out of turns": "It stopped: it used all its turns without finishing.",
    "stopped reporting": "It stopped: it stopped saying where it had got to.",
  };
  el.goalSays.textContent = [
    now.over ? over[now.over] || `It stopped: ${now.over}` : null,
    `${now.tries} of ${now.at_most} turns used.`,
    now.left ? `Last said what is left: ${now.left}` : null,
    now.over ? null : now.means,
  ]
    .filter(Boolean)
    .join(" ");
}

el.goalSave.addEventListener("click", async () => {
  if (!showing) return;
  const goal = el.goalWhat.value.trim();
  if (!goal) {
    el.goalSays.textContent = "It needs something to aim at.";
    return;
  }
  el.goalSave.disabled = true;
  try {
    await invoke("aim_at", { id: showing, goal });
  } catch (why) {
    el.goalSays.dataset.over = "true";
    el.goalSays.textContent = String(why);
    return;
  } finally {
    el.goalSave.disabled = false;
  }
  await drawGoal();
  keepGoalHonest();
});

el.goalStop.addEventListener("click", async () => {
  if (!showing) return;
  await invoke("aim_at", { id: showing, goal: null });
  await drawGoal();
});

el.watchSave.addEventListener("click", async () => {
  if (!showing) return;
  const watches = el.watchAt.value.trim();
  const what = el.watchWhat.value.trim();
  if (!watches || !what) {
    el.watchSays.textContent = "It needs something to watch and something to say.";
    return;
  }
  try {
    await invoke("watch_it", { id: showing, watches, what });
  } catch (why) {
    // Straight from the command, because it is the command that knows why.
    el.watchSays.dataset.paused = "true";
    el.watchSays.textContent = String(why);
    return;
  }
  await drawWatch();
});

el.watchStop.addEventListener("click", async () => {
  if (!showing) return;
  await invoke("watch_it", { id: showing, watches: null, what: null });
  await drawWatch();
});

el.watchAgain.addEventListener("click", async () => {
  if (!showing) return;
  await invoke("look_again", { id: showing });
  await drawWatch();
});


// ----------------------------------------------------------- which models --

/**
 * Which models show up.
 *
 * The picker used to be a search: every time it opened it probed this machine,
 * and offered to probe the network, and showed whatever answered. That is the
 * wrong shape in four ways at once. It is slow every time. It is different
 * every time. Most of what it finds is downloaded rather than loaded, so
 * choosing one means waiting without being told. And nothing anybody chose was
 * remembered, so the same search happened again tomorrow.
 *
 * Finding models is a thing you do once. This is the place to do it, and the
 * picker is only ever the result.
 */

/**
 * Addresses worth filling in for you.
 *
 * No two of these serve in the same place, which is the whole reason they are
 * here rather than in somebody's head, and getting it wrong gives a 404 that
 * reads exactly like a wrong key. So each one was checked rather than read off
 * a documentation page, by asking the host for both the path and a deliberately
 * nonsense one beside it: where the two answers differ, the path is proved.
 *
 *   Kimi        /v1/chat/completions answered 401 "Incorrect API key" -- the
 *               route is there and wants a key -- while /chat/completions
 *               answered 404 url.not_found. Proved, and the other one disproved.
 *   GLM         /api/paas/v4/chat/completions reached Z.ai's own auth and
 *               answered 1001; /v1/chat/completions was refused by nginx with a
 *               plain 404, so it is not served there at all.
 *   OpenRouter  /api/v1/models answered 200 with the real list, no key needed.
 *               /v1/... answered 404.
 *   DeepSeek    checks the key before it looks at the path, on every surface
 *               it has, so a nonsense path answers exactly the same 401 as a
 *               real one with a key and without: it cannot be proved from
 *               outside by anybody, and that is a fact about DeepSeek rather
 *               than a gap here. It is settled against the real key the moment
 *               somebody adds it, which is the one point at which the question
 *               can be answered at all, and the address that answered is what
 *               gets kept and shown.
 */
const KNOWN_PLACES = [
  {
    name: "DeepSeek",
    url: "https://api.deepseek.com",
    wire: "openai",
    sure: true,
    why: "Errand confirms the exact path against your key as you add it, because DeepSeek checks the key before it looks at the path.",
  },
  {
    // The same company, a second surface, a different protocol. Worth its own
    // button because it is not a variation on the address: it changes what is
    // sent and what comes back.
    name: "DeepSeek · Anthropic",
    url: "https://api.deepseek.com/anthropic",
    wire: "anthropic",
    sure: true,
    why: "DeepSeek's Anthropic-protocol surface, which speaks /v1/messages rather than /v1/chat/completions.",
  },
  {
    name: "Kimi",
    url: "https://api.moonshot.ai/v1",
    sure: true,
    why: "Checked: this is where it answers.",
  },
  {
    name: "GLM",
    url: "https://api.z.ai/api/paas/v4",
    sure: true,
    why: "Checked: this is where it answers. Not /v1, which is not served at all.",
  },
  {
    name: "OpenRouter",
    url: "https://openrouter.ai/api/v1",
    sure: true,
    why: "Checked: this is where it answers.",
  },
];

async function showModels() {
  el.models.hidden = false;
  el.presets.replaceChildren(
    ...KNOWN_PLACES.map((place) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = place.name;
      b.title = `${place.url} — ${place.why}`;
      if (!place.sure) b.dataset.unsure = "true";
      b.onclick = () => {
        editing = null;
        el.handSave.textContent = "Add";
        el.handLabel.value = place.name;
        el.handUrl.value = place.url;
        // Set with the address, because the two go together: the same host
        // serves both protocols at different paths, and either one alone is a
        // setup that cannot work.
        el.handWire.value = place.wire || "openai";
        // Said where the address is, rather than only in a tooltip nobody
        // hovers. Which of these was actually confirmed is the difference
        // between an address to trust and one to watch.
        el.handSays.dataset.wrong = "false";
        el.handSays.textContent = place.why;
        el.handKey.focus();
      };
      return b;
    }),
  );
  await Promise.all([drawChosen(), drawKept(), drawAtLogin()]);
}

/**
 * Whether Errand starts itself at login.
 *
 * Read every time the screen opens rather than remembered, because the thing
 * that decides is a file in a folder the system reads and anything could have
 * changed it: another copy of this app, a tidied folder, a restore from a
 * backup. A switch that shows what it remembers rather than what is true is
 * one somebody finds out about at a login.
 */
async function drawAtLogin() {
  let how;
  try {
    how = await invoke("opens_at_login");
  } catch (why) {
    el.atLogin.disabled = true;
    say(String(why), true);
    return;
  }
  el.atLogin.disabled = false;
  el.atLogin.checked = how === "yes";
  // The third answer, which is neither on nor off: something starts at login
  // and it is not this copy. Worth saying, because turning it on is what fixes
  // it and "it is already on" would be the one answer that does not.
  say(
    how === "something_else"
      ? "Another copy of Errand starts at login. Turning this on points it at this one."
      : "",
    how === "something_else",
  );
}

/** What the switch says under it, which is only ever about the switch. */
function say(words, wrong) {
  el.atLoginSays.textContent = words;
  el.atLoginSays.dataset.wrong = String(!!wrong);
}

el.atLogin.addEventListener("change", async () => {
  const wanted = el.atLogin.checked;
  try {
    const how = await invoke("open_at_login", { yes: wanted });
    el.atLogin.checked = how === "yes";
    // Said plainly, including the part somebody would otherwise find out at the
    // next restart: nothing starts a second copy now.
    say(
      how === "yes"
        ? "Errand will open at your next login. Nothing has started a second copy now."
        : "Errand will not open at login.",
      false,
    );
  } catch (why) {
    // Put back, because the switch showing one thing while the file says
    // another is the failure this whole screen is written to avoid.
    el.atLogin.checked = !wanted;
    say(String(why), true);
  }
});

el.setup.addEventListener("click", showModels);
el.modelsDone.addEventListener("click", () => {
  el.models.hidden = true;
});

/** Everything in the picker, with a way to take each one out. */
async function drawChosen() {
  let all;
  try {
    all = await invoke("whats_offered");
  } catch (why) {
    el.chosen.replaceChildren(note("li", String(why), "nothing"));
    return;
  }
  if (!all.length) {
    el.chosen.replaceChildren(
      note("li", "Nothing. The picker is empty, so no agent can be given anything to answer with.", "nothing"),
    );
    return;
  }
  el.chosen.replaceChildren(
    ...all.map((one, at) => {
      const row = document.createElement("li");
      const words = document.createElement("span");
      words.className = "grow";
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = one.label;
      words.append(name);
      if (one.settings && one.engine === "local") {
        const where = document.createElement("span");
        where.className = "where";
        try {
          where.textContent = JSON.parse(one.settings).base_url || "";
        } catch {
          where.textContent = "";
        }
        words.append(where);
      }
      // The order is what the dropdown shows, top to bottom, so it is the
      // point rather than a nicety: what somebody uses most belongs where their
      // eye lands.
      const shuffle = (up) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "nudge";
        b.textContent = up ? "↑" : "↓";
        b.title = up ? "Further up the picker" : "Further down";
        b.disabled = up ? at === 0 : at === all.length - 1;
        b.onclick = async () => {
          await invoke("move_it", { id: one.id, up });
          thePickerHasChanged();
          await drawChosen();
          const a = whose();
          if (a) await drawEngines(a);
        };
        return b;
      };

      // Named by somebody rather than by whatever it was seeded from. The ones
      // carried over from an agent already using them were called after the
      // agent, which is not what a model is called.
      const rename = document.createElement("button");
      rename.type = "button";
      rename.textContent = "Rename";
      rename.onclick = () => {
        const box = document.createElement("input");
        box.type = "text";
        box.className = "renaming";
        box.value = one.label;
        const keep = async () => {
          const wanted = box.value.trim();
          if (!wanted || wanted === one.label) return drawChosen();
          await invoke("call_it_something", { id: one.id, label: wanted });
          thePickerHasChanged();
          await drawChosen();
          const a = whose();
          if (a) await drawEngines(a);
        };
        box.onkeydown = (e) => {
          if (e.key === "Enter") keep();
          if (e.key === "Escape") drawChosen();
        };
        box.onblur = keep;
        name.replaceWith(box);
        box.focus();
        box.select();
      };

      const out = document.createElement("button");
      out.type = "button";
      out.textContent = "Remove";
      out.title = "Take it out of the picker. Anything already set to it goes on using it.";
      out.onclick = async () => {
        await invoke("stop_offering", { id: one.id });
        thePickerHasChanged();
        await drawChosen();
        const a = whose();
        if (a) await drawEngines(a);
      };
      row.append(words, shuffle(true), shuffle(false), rename, out);
      return row;
    }),
  );
}

/** Everywhere models come from that somebody has kept. */
async function drawKept() {
  let kept;
  try {
    kept = await invoke("backends");
  } catch (why) {
    el.found.replaceChildren(note("li", String(why), "nothing"));
    return;
  }
  drawPlaces(kept, { kept: true });
}

/**
 * One list of places, whether found by looking or kept from before.
 *
 * The same rows either way, because from here they are the same thing: an
 * address with models behind it. What differs is only what the buttons do.
 */
function drawPlaces(places, { kept = false } = {}) {
  if (!places.length) {
    el.found.replaceChildren(
      note("li", kept ? "Nothing kept yet. Look on this Mac, or add one by hand." : "Nothing answered.", "nothing"),
    );
    return;
  }
  el.found.replaceChildren(
    ...places.flatMap((place) => {
      const head = document.createElement("li");
      const words = document.createElement("span");
      words.className = "grow";
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = place.label;
      const where = document.createElement("span");
      where.className = "where";
      where.textContent =
        place.base_url +
        (place.wire === "anthropic" ? " · Anthropic protocol" : "") +
        (place.has_key ? " · key kept" : "");
      words.append(name, where);
      head.append(words);

      if (place.trouble) {
        const why = document.createElement("span");
        why.className = "where";
        why.textContent = place.trouble;
        words.append(why);
      }

      const look = document.createElement("button");
      look.type = "button";
      look.textContent = "What has it got?";
      look.onclick = async () => {
        look.disabled = true;
        look.textContent = "Asking…";
        try {
          const said = kept
            ? await invoke("models_at", { id: place.id })
            : { ...place };
          drawPlaces(
            places.map((p) => (p.id === place.id ? said : p)),
            { kept },
          );
        } catch (why) {
          look.textContent = String(why);
          look.disabled = false;
        }
      };
      head.append(look);

      if (kept) {
        // Changing one rather than forgetting it and starting again, which is
        // what somebody rotating a key would otherwise have to do -- and it
        // would take every model they had chosen from it with it.
        const change = document.createElement("button");
        change.type = "button";
        change.textContent = "Change";
        change.title = "Change its name, address or key.";
        change.onclick = () => {
          editing = place.id;
          el.handLabel.value = place.label;
          el.handUrl.value = place.base_url;
          el.handWire.value = place.wire || "openai";
          el.handKey.value = "";
          el.handSave.textContent = "Save";
          el.handSays.dataset.wrong = "false";
          el.handSays.textContent = place.has_key
            ? "Changing this one. Leave the key empty to keep the one it already has."
            : "Changing this one.";
          el.handUrl.scrollIntoView({ block: "center" });
          el.handLabel.focus();
        };
        head.append(change);

        const drop = document.createElement("button");
        drop.type = "button";
        drop.textContent = "Forget";
        drop.title = "Forget the address and its key, and take its models out of the picker.";
        drop.onclick = async () => {
          await invoke("forget_backend", { id: place.id });
          thePickerHasChanged();
          await Promise.all([drawKept(), drawChosen()]);
          const a = whose();
          if (a) await drawEngines(a);
        };
        head.append(drop);
      } else {
        const keep = document.createElement("button");
        keep.type = "button";
        keep.textContent = "Keep";
        keep.title = "Remember this address, so it is here next time without looking.";
        keep.onclick = async () => {
          keep.disabled = true;
          await invoke("remember_backend", {
            label: place.label,
            provider: place.provider,
            baseUrl: place.base_url,
            apiKey: null,
          }).catch(() => {});
          await drawKept();
        };
        head.append(keep);
      }

      const models = (place.models || []).map((m) => {
        const row = document.createElement("li");
        const lit = document.createElement("span");
        // Loaded and able to answer now, or downloaded and needing a wait.
        // A dot rather than a sentence, because this is the one thing worth
        // seeing at a glance down a list of twenty.
        lit.className = m.loaded ? "lit" : "lit cold";
        lit.title = m.loaded ? "Loaded and ready" : "Downloaded, but not loaded: the first errand waits";
        const words = document.createElement("span");
        words.className = "grow";
        const name = document.createElement("span");
        name.className = "name";
        name.textContent = m.model;
        words.append(name);

        const add = document.createElement("button");
        add.type = "button";
        add.textContent = "Show in picker";
        add.onclick = async () => {
          add.disabled = true;
          await invoke("offer_this", {
            engine: "local",
            label: `${m.model} · ${place.label}`,
            settings: JSON.stringify({
              provider: place.provider,
              base_url: place.base_url,
              model: m.model,
              // Carried onto the choice itself. Without it a model added from
              // an Anthropic backend is later asked in the other protocol,
              // which fails when somebody uses it rather than here.
              wire: place.wire || "openai",
            }),
            backend: kept ? place.id : null,
          });
          thePickerHasChanged();
          add.textContent = "In the picker";
          add.className = "on";
          await drawChosen();
          const a = whose();
          if (a) await drawEngines(a);
        };
        row.append(lit, words, add);
        return row;
      });
      return [head, ...models];
    }),
  );
}

/**
 * Which backend the form is editing, if it is editing one.
 *
 * Nothing means it is adding. The difference matters: without it, changing the
 * address of something already kept quietly makes a second one beside it.
 */
let editing = null;

/** Put the form back to adding rather than changing. */
function backToAdding() {
  editing = null;
  el.handSave.textContent = "Add";
  el.handLabel.value = "";
  el.handUrl.value = "";
  el.handKey.value = "";
  el.handWire.value = "openai";
}

/** Look, here or wider. */
async function goLooking(wider) {
  el.lookHere.disabled = true;
  el.lookWide.disabled = true;
  el.sweeping.hidden = false;
  el.findSays.textContent = wider
    ? "Looking across the network. This takes most of a minute."
    : "Looking on this Mac.";
  try {
    const places = await invoke("look_for_models", { wider });
    // Said where it was asked for. A sweep that finds nothing and a sweep that
    // never ran look exactly alike from a list that stays empty, and what
    // anybody concludes from that is that the button is broken.
    el.findSays.textContent = places.length
      ? `Found ${places.length} ${places.length === 1 ? "place" : "places"}.`
      : wider
        ? "Nothing on the network answered. A model listening only on 127.0.0.1 cannot be " +
          "seen from another machine: the server has to be bound to its network address, " +
          "for example OLLAMA_HOST=0.0.0.0 for Ollama."
        : "Nothing on this Mac answered. Is Ollama or LM Studio running?";
    drawPlaces(places);
  } catch (why) {
    el.findSays.textContent = String(why);
  } finally {
    el.lookHere.disabled = false;
    el.lookWide.disabled = false;
    el.sweeping.hidden = true;
  }
}

el.lookHere.addEventListener("click", () => goLooking(false));
el.lookWide.addEventListener("click", () => goLooking(true));

el.byHand.addEventListener("submit", async (e) => {
  e.preventDefault();
  const baseUrl = el.handUrl.value.trim();
  if (!baseUrl) {
    el.handSays.dataset.wrong = "true";
    el.handSays.textContent = "It needs an address.";
    return;
  }
  el.handSays.dataset.wrong = "false";
  el.handSays.textContent = "Asking it where it answers…";
  try {
    const kept = await invoke("remember_backend", {
      // The one being changed, where one is. Without this, changing an address
      // adds a second backend beside the first rather than changing it.
      id: editing,
      label: el.handLabel.value.trim(),
      provider: "openai-compat",
      baseUrl,
      apiKey: el.handKey.value.trim() || null,
      wire: el.handWire.value,
    });
    // The address it settled on, said out loud, because it is often not the one
    // that was typed and that is the whole point of asking: these providers do
    // not agree on where they serve, and a wrong path answers 404 in a way that
    // reads exactly like a wrong key.
    const moved = kept.base_url !== baseUrl;
    el.handSays.dataset.wrong = !!kept.trouble;
    el.handSays.textContent = kept.trouble
      ? `Kept, but nothing answered there yet, so the address is unchecked. ${kept.trouble}`
      : [
          moved ? `It answers at ${kept.base_url}, not quite what was typed.` : "Checked: it answers there.",
          kept.models.length
            ? `${kept.models.length} model${kept.models.length === 1 ? "" : "s"} to choose from below.`
            : "It offered no models, which is what a key with nothing enabled on it looks like.",
        ].join(" ");
  } catch (why) {
    el.handSays.dataset.wrong = "true";
    el.handSays.textContent = String(why);
  }
  // The key is not kept in the page for a moment longer than it takes to send.
  el.handKey.value = "";
  backToAdding();
  await drawKept();
});


// ------------------------------------------------------------- what it cost --

/**
 * What it has cost.
 *
 * The engine says on every turn and this app threw it away, so an errand that
 * ran every morning for a month had no answer at all to the one question
 * anybody running errands has. Kept per turn, so today and this month are both
 * askable rather than one running total answering neither.
 *
 * Nothing about a model on this machine appears here, because the answer for
 * one of those is not zero dollars, it is no dollars, and a row of zeroes would
 * make the total a lie about what it is a total of.
 */
/**
 * What this app is, for somebody who has just opened it.
 *
 * Not a carousel and not a sequence of things to dismiss. Each line names a
 * thing that is actually on screen and says what it is for, so it can be read
 * with the window behind it rather than instead of it.
 *
 * Written in what the thing does rather than what it is called. "Repeat" means
 * nothing to somebody who has not used it; "the same errand every morning"
 * means the thing they came here wanting.
 */
const WHAT_THIS_IS = [
  [
    "An agent, not a chat",
    "The list on the left is agents, not conversations. An agent is somebody you " +
      "come back to: it keeps what it learned, what it is allowed to do and what " +
      "it can reach. Starting again tomorrow is starting again with all of that.",
  ],
  [
    "Say what you want done",
    "Type it in the box at the bottom, in whatever words you would use to a " +
      "person. You can say something else while it is still working, and you can " +
      "change your mind halfway.",
  ],
  [
    "Repeat · the same errand every morning",
    "Give it a time and something to say, and it says it on its own. It runs " +
      "while Errand is open, which is what the switch under Settings is about.",
  ],
  [
    "Watch · wake it when something changes",
    "A folder that gets a file, a page that changes its mind. It says how often " +
      "it will look and what that comes to before you agree to it.",
  ],
  [
    "Goal · something to get to",
    "Repeat is something to do; a goal is something to reach. It keeps going " +
      "until it gets there, says so when it does, and stops itself if it is going " +
      "round in circles.",
  ],
  [
    "Allowed and Tools · what it may do",
    "It asks before it does anything to your machine. Allowed is what you have " +
      "said yes to for good, in words rather than in rules, and you can take any " +
      "of it back. Tools is what this thread can reach.",
  ],
  [
    "The gear, bottom left",
    "Which models show up in the picker, and nothing else is offered anywhere in " +
      "the app. Add hosted ones with a key, or find what is already running on " +
      "this machine.",
  ],
  [
    "From a terminal, too",
    "Errand ask \"Day Check\" \"what is the date?\" hands the job to that agent " +
      "and prints the answer. Add --json to get something a script can read.",
  ],
];

/**
 * Show it, or put it away.
 *
 * Opened by hand from the palette, and once on its own: the first time this app
 * is opened there is nothing in it, and an empty window that explains itself is
 * better than an empty window.
 */
function showTheTour() {
  if (!el.tour.hidden) {
    el.tour.hidden = true;
    return;
  }
  el.tour.hidden = false;
  // Counted rather than written down. It said seven while there were eight of
  // them, which is a small lie in the one part of the app whose whole job is
  // to be believed, and it would drift again the next time one was added.
  const head = note("p", `This is Errand. ${WHAT_THIS_IS.length} things and then you know it.`);
  const rows = WHAT_THIS_IS.map(([title, said]) => {
    const row = document.createElement("div");
    row.className = "one tour-one";
    const what = document.createElement("span");
    what.className = "who";
    what.textContent = title;
    const why = document.createElement("span");
    why.className = "where";
    why.textContent = said;
    row.append(what, why);
    return row;
  });
  const done = document.createElement("button");
  done.type = "button";
  done.className = "tour-done";
  done.textContent = "Got it";
  done.onclick = () => {
    el.tour.hidden = true;
    el.what.focus();
  };
  el.tour.replaceChildren(head, ...rows, done);
}

async function whatItCost() {
  if (!el.costing.hidden) {
    el.costing.hidden = true;
    return;
  }
  el.costing.hidden = false;
  el.costing.replaceChildren(note("p", "Adding it up…"));

  let spent;
  try {
    spent = await invoke("what_it_cost");
  } catch (why) {
    el.costing.replaceChildren(note("p", String(why)));
    return;
  }

  if (spent.nothing_yet) {
    el.costing.replaceChildren(
      note(
        "p",
        "Nothing has cost anything yet. Only Claude is paid for: a model running " +
          "on this machine costs no money, so it never appears here.",
      ),
    );
    return;
  }

  const money = (d) => `$${d.toFixed(2)}`;
  const total = (rows) => rows.reduce((sum, one) => sum + one.dollars, 0);

  const section = (title, rows) => {
    const head = document.createElement("p");
    head.className = "server-what";
    head.textContent = rows.length
      ? `${title}: ${money(total(rows))} across ${rows.length} ${rows.length === 1 ? "agent" : "agents"}.`
      : `${title}: nothing.`;
    const list = rows.map((one) => {
      const row = document.createElement("div");
      row.className = "one";
      const who = document.createElement("span");
      who.className = "who";
      who.textContent = one.who;
      const much = document.createElement("span");
      much.className = "where";
      // Turns as well as errands, because a single errand that went round
      // thirty times is the one worth looking at and its price alone does not
      // say that.
      much.textContent = `${money(one.dollars)} · ${one.errands} ${
        one.errands === 1 ? "errand" : "errands"
      }, ${one.turns} ${one.turns === 1 ? "turn" : "turns"}`;
      row.append(who, much);
      return row;
    });
    return [head, ...list];
  };

  el.costing.replaceChildren(
    ...section("Today", spent.today),
    ...section("This month", spent.this_month),
  );
}
