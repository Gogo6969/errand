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
import { render, reachTheAppWith } from "./markdown.js";
import { toSay } from "./speech.js";

const { invoke } = window.__TAURI__.core;
// The renderer draws pictures an agent made and reveals files it wrote, and
// both of those are the app's to do rather than the window's.
reachTheAppWith(invoke);
const { listen } = window.__TAURI__.event;

/**
 * A turn is over, however it ended.
 *
 * Two things stop being true at once and there is no ending where one stops
 * without the other: it is not working, and it is not part way through a
 * sentence. Written apart, the sentence was left behind by every ending the
 * engine does not send -- somebody pressing Stop, or changing the model
 * mid-turn -- because a killed engine sends nothing at all. What stayed on
 * screen was half an answer with a caret blinking under it, through every
 * redraw and every conversation switch, and the next turn's first word was
 * appended to it.
 */
function itHasStopped(talk) {
  if (!talk) return;
  talk.working = false;
  talk.writing = "";
}

function complain(why) {
  const t = talking();
  if (!t) {
    document.getElementById("thread-name").textContent = why;
    return;
  }
  itHasStopped(t);
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
  menu: document.getElementById("menu"),
  messages: document.getElementById("messages"),
  name: document.getElementById("thread-name"),
  engine: document.getElementById("engine"),
  sweeping: document.getElementById("sweeping"),
  setup: document.getElementById("setup"),
  models: document.getElementById("models"),
  modelsDone: document.getElementById("models-done"),
  reachableList: document.getElementById("reachable-list"),
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
  changed: document.getElementById("changed"),
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
  watchOften: document.getElementById("watch-often"),
  watchPlain: document.getElementById("watch-plain"),
  watchSaid: document.getElementById("watch-said"),
  watchTry: document.getElementById("watch-try"),
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
  allowAhead: document.getElementById("allow-ahead"),
  allowWhat: document.getElementById("allow-what"),
  allowTool: document.getElementById("allow-tool"),
  allowSays: document.getElementById("allow-says"),
  alsoAllowed: document.getElementById("also-allowed"),
  asksMeans: document.getElementById("asks-means"),
  routine: document.getElementById("routine"),
  routineAt: document.getElementById("routine-at"),
  routineWhat: document.getElementById("routine-what"),
  routineSaid: document.getElementById("routine-said"),
  routineTry: document.getElementById("routine-try"),
  routineSave: document.getElementById("routine-save"),
  finding: document.getElementById("finding"),
  findingWhat: document.getElementById("finding-what"),
  findingCount: document.getElementById("finding-count"),
  findingPrev: document.getElementById("finding-prev"),
  findingNext: document.getElementById("finding-next"),
  findingDone: document.getElementById("finding-done"),
  routinePause: document.getElementById("routine-pause"),
  routineStop: document.getElementById("routine-stop"),
  routineWent: document.getElementById("routine-went"),
  routineWentList: document.getElementById("routine-went-list"),
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
/**
 * What each agent has said that nobody has read yet, by agent id.
 *
 * Asked of the app rather than worked out here. The window is not present for
 * most of the reasons this changes: a routine fires at seven, a watch wakes
 * something at lunchtime, a delegated errand comes back. All of them write into
 * conversations this window has never loaded.
 */
let fresh = new Map();

async function whatIsNew() {
  try {
    fresh = new Map(Object.entries(await invoke("what_is_new")));
  } catch {
    // A count that could not be fetched is no count. Saying "3 new" from a
    // stale answer is worse than saying nothing, because somebody clicks it.
    fresh = new Map();
  }
  drawThreads();
}

/**
 * Say that what is on screen has been read.
 *
 * Only for the conversation actually in front of somebody, and only once it is
 * drawn. Marking every conversation of an agent read because one of them was
 * opened is how an unread briefing disappears without being seen.
 */
async function nowSeen(id) {
  if (!id) return;
  await invoke("seen", { conversation: id }).catch(() => {});
  await whatIsNew();
}

/** How long ago, in the fewest words that are still true. */
function howLongAgo(at) {
  const secs = Math.max(0, (Date.now() - at) / 1000);
  if (secs < 90) return "just now";
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  if (days < 7) return `${days}d ago`;
  return new Date(at).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/**
 * What was half typed in each conversation, kept for coming back to.
 *
 * In memory only, and deliberately. A draft is a thought somebody has not
 * finished having; writing it to disk makes it a thing that survives them
 * closing the app, which is a different promise and one nobody asked for.
 */
const halfTyped = new Map();

function putItDown(id) {
  if (!id || !el.what) return;
  const said = el.what.value;
  if (said.trim()) halfTyped.set(id, said);
  else halfTyped.delete(id);
}

function pickItBackUp(id) {
  if (!el.what) return;
  el.what.value = halfTyped.get(id) || "";
  el.what.style.height = "auto";
  el.what.style.height =
    Math.min(el.what.scrollHeight, window.innerHeight * 0.4) + "px";
}

/**
 * Go to one line, opening whatever has to be opened to reach it.
 *
 * The end of a search. Its agent, then its conversation, then the line itself,
 * scrolled to and marked, because a conversation opened at the bottom with the
 * words somewhere above is the same amount of scrolling somebody was doing
 * before they searched.
 */
async function goToTheLine(hit) {
  if (!talks.has(hit.conversation)) await openAgent(hit.agent);
  if (talks.has(hit.conversation)) await show(hit.conversation);
  markTheLine(hit.seq);
}

/**
 * Mark one line and put it on screen.
 *
 * Marked rather than only scrolled to. A conversation scrolled to the middle
 * with nothing picked out is a conversation somebody now has to read to find
 * out why they are there.
 */
function markTheLine(seq) {
  const t = talking();
  if (!t) return;
  const at = t.messages.findIndex((m) => m.seq === seq);
  if (at < 0) return;
  const drawn = el.messages.children[theRowFor(at)];
  if (!drawn) return;
  for (const was of el.messages.querySelectorAll(".found")) was.classList.remove("found");
  drawn.classList.add("found");
  drawn.scrollIntoView({ block: "center" });
}

/**
 * Which row on screen belongs to the nth message.
 *
 * They are not the same number: the day separators are rows of their own, and
 * counting past them lands on the line above or below the one somebody
 * searched for, which is worse than not scrolling at all.
 */
function theRowFor(nth) {
  let row = 0;
  const t = talking();
  let day = null;
  for (let i = 0; i <= nth && i < t.messages.length; i++) {
    const m = t.messages[i];
    if (m.at) {
      const its = whichDay(m.at);
      if (day !== null && its !== day) row += 1;
      day = its;
    }
    if (i < nth) row += 1;
  }
  return row;
}

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
  await whatIsNew();
  drawThreads();
  // A version somebody has not been told about yet, said once. After the tour,
  // because a brand new copy has nothing to have changed from.
  if (known.length) await whatChanged(false);
  if (known.length) await openAgent(known[0].id);
  else {
    await start();
    // Nothing has ever been done in this copy, so there is nothing on screen
    // to read and nothing to work out from. Shown once, here, rather than
    // remembered and shown again: the second time somebody opens this app they
    // have an agent, and this never runs.
    showTheTour();
    // And this copy has been told what this version is, by being told what the
    // app is. Without this, somebody who installed Errand today would be shown
    // "what changed in 0.1.0" tomorrow, having never run anything else, which
    // is an answer to a question they cannot have asked.
    invoke("seen_what_changed").catch(() => {});
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
  // Half a sentence belongs to the conversation it was being written into. It
  // used to follow whoever switched, which this app encourages constantly: the
  // sidebar row, the conversation picker and "New conversation with this agent"
  // are all one click, and every one of them carried an unsent errand into
  // somebody else's composer where Enter would send it. The only item on this
  // list that could lose work rather than merely fail to show it.
  putItDown(showing);
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
      // Before the lines are read, since reading one asks whether it is still
      // being waited on.
      whatIsStillWaiting(),
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
  pickItBackUp(id);
  // Which conversation somebody is actually reading, so a notification can be
  // held back for this one and shown for the thirty-nine that are not. The app
  // knows what is running; only the window knows what is being looked at.
  invoke("looking_at", { id }).catch(() => {});
  // Read, now that it is on screen. Not before: `show` is called for the
  // window's own reasons as well as somebody's, and marking a briefing read
  // that nobody has looked at is the one way this feature can do harm.
  nowSeen(id);

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
    // Carried like the rest of what is going on. Left out, clicking the agent
    // while an answer was arriving threw away the sentence being written and
    // put the working dots back under a half-finished line.
    writing: keeping?.writing ?? "",
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
/**
 * The handovers still being waited on, as of the last time anything asked.
 *
 * Read before a conversation is drawn rather than per line, because it is one
 * question about the app rather than a question about each line.
 */
let stillWaiting = new Set();

async function whatIsStillWaiting() {
  try {
    stillWaiting = new Set(await invoke("waiting_on_you"));
  } catch {
    // Nothing waiting is the safe answer: a card drawn without its buttons
    // says so plainly, where one drawn with buttons that answer nobody does
    // not.
    stillWaiting = new Set();
  }
}

function fromStore(line, live = false) {
  // Every branch below used to drop `at`, and the whole of a thread's sense of
  // when is in it. A conversation an agent works in overnight reads as one
  // unbroken block: yesterday's briefing sits directly above this morning's
  // with nothing between them, which is the shape this app is for and the one
  // it could not show.
  const one = fromStoreLine(line, live);
  return one && { ...one, at: line.at };
}

function fromStoreLine(line, live = false) {
  switch (line.kind) {
    case "mine":
    case "said":
      return {
        kind: line.kind,
        text: line.text,
        seq: line.seq,
        // Names, not bytes. The picture itself is fetched when the line is
        // drawn, so opening a conversation with forty screenshots in it does
        // not put forty screenshots in memory before a word is on screen.
        pictures: line.pictures || [],
      };
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
    // What somebody was asked to come and do. Read back without its buttons:
    // the agent that was waiting is long gone, so offering to tell it you are
    // finished would be offering to tell nobody.
    // Whether it still has its buttons depends on whether the agent that asked
    // is still sitting there, which a line on disk cannot say. `stillWaiting`
    // is what the app answered when this conversation was read back. Without
    // it, opening the conversation turned a question somebody was being asked
    // into a note about one, with the agent still waiting and nothing on
    // screen to answer it with.
    case "over_to_you": {
      const [what, ...rest] = String(line.text).split("\n");
      const where = rest.find((one) => /^https?:\/\//.test(one)) || "";
      const still = stillWaiting.has(line.call);
      return {
        kind: "over_to_you",
        seq: line.seq,
        handover: line.call || "",
        what,
        why: rest.filter((one) => one !== where).join(" "),
        where,
        // Not answered, and not a dead end either. Nobody is parked on it any
        // more, but the thing it asked for is still a thing somebody can go
        // and do, so the card keeps its buttons and they say it into the
        // conversation instead of into a call that is gone.
        answered: null,
        stillThere: still,
      };
    }
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
      return {
        kind: "ended",
        failed: line.kind === "ended",
        text: line.text,
        seq: line.seq,
        // A turn the app cut off by closing, rather than one that failed. It
        // is the only ending somebody can do anything about, so it is the only
        // one that offers to.
        cutOff: line.call === "cut-off",
      };
  }
}

/**
 * What can be done to an agent, without opening it.
 *
 * Right-clicking a thing and being offered what can be done to it is how every
 * list on this machine works, and this one answered with nothing at all. The
 * consequence was not a missing convenience: there was no way to delete an
 * agent from the window at all, so a thread somebody made by mistake stayed in
 * their list for good.
 *
 * Deliberately short. Everything here is something that can only be done to an
 * agent from outside it, or that somebody would look for here first.
 */
let menuIsFor = null;

function closeTheMenu() {
  el.menu.hidden = true;
  menuIsFor = null;
}

/**
 * @param {object} a the agent right-clicked
 * @param {number} x where the pointer was
 * @param {number} y
 */
/**
 * A name short enough to put inside a sentence.
 *
 * An agent that has not named itself is called after the first thing anybody
 * said to it, which is a whole request and sometimes a paragraph. Dropped into
 * "Delete X? Everything it said goes too" that made a button three lines long
 * ending in "hello.?", which is neither readable nor a question.
 */
function inAFewWords(name) {
  const said = String(name || "").trim().replace(/[.!?,;:]+$/, "");
  return said.length > 28 ? `${said.slice(0, 27).trimEnd()}\u{2026}` : said;
}

function openTheMenu(a, x, y) {
  menuIsFor = a.id;
  const items = [];

  const item = (label, run, how = "") => {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("role", "menuitem");
    if (how) b.className = how;
    b.textContent = label;
    b.onclick = async (e) => {
      e.stopPropagation();
      await run(b);
    };
    items.push(b);
    return b;
  };

  item(a.pinned ? "Unpin" : "Pin to the top", async () => {
    a.pinned = !a.pinned;
    closeTheMenu();
    drawThreads();
    if (a.id === showingAgent) drawPinned(a);
    await invoke("pin", { id: a.id, pinned: a.pinned });
  });

  item("Who this is", async () => {
    closeTheMenu();
    // Its own profile, which means opening it first: the panel edits whoever
    // is on screen, and editing one agent's name into another's is the one
    // mistake this must not make easy.
    if (a.id !== showingAgent) await openAgent(a.id);
    el.whois.hidden = true;
    el.name.click();
  });

  item("New conversation with this agent", async () => {
    closeTheMenu();
    if (a.id !== showingAgent) await openAgent(a.id);
    await alsoAsk();
  });

  item("Copy its id", async (b) => {
    try {
      await navigator.clipboard.writeText(a.id);
      b.textContent = "Copied";
    } catch {
      // A clipboard that refuses is not worth an error in the conversation,
      // and the id is on screen in the button either way.
      b.textContent = a.id;
    }
  });

  item(a.hidden ? "Show in the list" : "Hide from the list", async () => {
    a.hidden = !a.hidden;
    closeTheMenu();
    drawThreads();
    await invoke("hide", { id: a.id, hidden: a.hidden });
  });

  // Last, apart, and asked about twice. Everything it ever said goes with it.
  const remove = item("Delete", async (b) => {
    // The second press is the answer to a question the first press asked, so
    // the button becomes the question rather than a dialog appearing over it.
    if (b.dataset.sure !== "true") {
      b.dataset.sure = "true";
      b.textContent = `Delete ${inAFewWords(a.name)}? Everything it said goes too`;
      // The label just grew. Without this the menu keeps the height it was
      // measured at and the sentence saying what is about to be destroyed
      // hangs off the bottom of the window.
      placeTheMenu();
      return;
    }
    closeTheMenu();
    await forgetAgent(a);
  }, "danger");
  remove.dataset.sure = "false";

  el.menu.replaceChildren(...items);
  el.menu.hidden = false;
  placeTheMenu(x, y);
}

/**
 * Put the menu where it fits.
 *
 * Called again whenever anything in it changes size, which is not a nicety:
 * pressing Delete turns a short label into a long one, the menu grows
 * downwards, and the sentence saying what is about to be destroyed goes off
 * the bottom of the window. So the one line somebody most needs to read is the
 * one they cannot.
 */
let theMenuIsAt = { x: 0, y: 0 };

function placeTheMenu(x, y) {
  if (x !== undefined) theMenuIsAt = { x, y };
  const box = el.menu.getBoundingClientRect();
  const room = {
    x: window.innerWidth - box.width - 8,
    y: window.innerHeight - box.height - 8,
  };
  el.menu.style.left = `${Math.max(8, Math.min(theMenuIsAt.x, room.x))}px`;
  el.menu.style.top = `${Math.max(8, Math.min(theMenuIsAt.y, room.y))}px`;
}

/**
 * Delete an agent, and leave the window somewhere sensible.
 *
 * The files it made are not touched. They are in its own folder and they are
 * somebody's work, not the app's to throw away on the strength of a menu
 * click; what goes is everything Errand knows about it.
 */
async function forgetAgent(a) {
  try {
    await invoke("forget", { id: a.id });
  } catch (why) {
    complain(String(why));
    return;
  }
  agents.delete(a.id);
  for (const [id, t] of talks) if (t.agent === a.id) talks.delete(id);

  // Whatever was on screen has just been deleted, so something else has to be.
  if (showingAgent === a.id) {
    const next = [...agents.values()].find((other) => !other.hidden);
    showing = null;
    showingAgent = null;
    if (next) await openAgent(next.id);
    else await start();
  }
  drawThreads();
}

// Anywhere else, and the menu is not the thing being clicked any more.
document.addEventListener("click", () => {
  if (!el.menu.hidden) closeTheMenu();
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !el.menu.hidden) {
    e.stopPropagation();
    closeTheMenu();
  }
});
// A menu pinned to a pointer is wrong the moment anything moves under it.
window.addEventListener("resize", closeTheMenu);
el.threads.addEventListener("scroll", closeTheMenu);

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
      // Which agent this row is, on the row. Everything that acts on one had
      // to close over it, which is fine for a click and no use at all to
      // anything asking the list what it is showing.
      li.dataset.agent = a.id;
      li.onclick = () => {
        const found = hitsByAgent.get(a.id);
        // Arriving at the line rather than near it. Without this, a search hit
        // in a conversation from March opens whichever conversation this agent
        // spoke in most recently, which is the one place the words are not.
        return found ? goToTheLine(found) : openAgent(a.id);
      };
      li.oncontextmenu = (e) => {
        e.preventDefault();
        // Not opening it. Somebody asking what can be done to an agent has not
        // asked to go and look at it, and switching under them loses whatever
        // they were reading.
        openTheMenu(a, e.clientX, e.clientY);
      };
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
      const news = fresh.get(a.id);
      const hit = hitsByAgent.get(a.id);
      // While a search is on, the useful line under the name is the line that
      // matched. Everything else about the agent is still true and is not what
      // was asked.
      last.textContent = hit
        ? hit.snippet
        : waitingOn(a.id)
        ? "Waiting on you"
          : busy(a.id)
            ? "Working…"
            : // Something happened here and nobody has seen it. This takes the
            // line for as long as that is true, because it is the one thing
            // about an agent somebody cannot work out by looking at the list,
            // and it is the whole reason to leave errands running.
              news
              ? `${news.lines} new · ${howLongAgo(news.at)}`
              : a.about || "Nothing said yet";
      if (hit) last.classList.add("hit");
      if (waitingOn(a.id)) last.classList.add("waiting");
      if (news && !waitingOn(a.id) && !busy(a.id)) last.classList.add("new");

      words.append(name, last);
      li.append(words);
      // A mark on the row itself, not only in the line under the name, so a
      // list of forty can be skimmed rather than read.
      if (news && !waitingOn(a.id)) li.classList.add("has-new");
      return li;
    }),
  );
}

// ------------------------------------------------------------ messages --

/**
 * The day a line was said, as somebody would say it.
 *
 * Today and yesterday by name, because those are the two that matter and a
 * date beside them reads as older than it is. Everything before that is dated,
 * with the year only once it is a different year: "12 March" is unambiguous
 * within a year and misleading across one.
 */
function whichDay(at) {
  const then = new Date(at);
  const midnight = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const days = Math.round((midnight(new Date()) - midnight(then)) / 86400000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  const sameYear = then.getFullYear() === new Date().getFullYear();
  return then.toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: sameYear ? undefined : "numeric",
  });
}

/**
 * A line saying what day the next thing happened on.
 *
 * Only where the day changes. A separator on every message would be noise; the
 * one place it is worth an entire row of the window is the seam between a
 * conversation you had and one your agent had while you were asleep.
 */
function theDayChanged(day) {
  const li = document.createElement("li");
  li.className = "day";
  const said = document.createElement("span");
  said.textContent = day;
  li.append(said);
  return li;
}

function drawMessages() {
  const t = talking();
  if (!t) return;
  const drawn = [];
  let day = null;
  for (const m of t.messages) {
    const node = draw(m);
    if (!node) continue;
    // A line with no time is one that arrived this second and has not been
    // written down yet, which is today by definition and needs no announcing.
    if (m.at) {
      const its = whichDay(m.at);
      if (day !== null && its !== day) drawn.push(theDayChanged(its));
      day = its;
    }
    drawn.push(node);
  }
  el.messages.replaceChildren(...drawn);
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
      node.append(words);
      // The pictures themselves, rather than a note saying there were some.
      // A conversation that was about a picture used to read afterwards as a
      // conversation about nothing, and you could never see the one you sent.
      if (m.pictures?.length || m.showing?.length) showThePictures(node, m);
      node.append(doneWith(m));
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
    case "over_to_you":
      return handItOver(m);
    case "ended": {
      node.className = m.failed ? "ended failed" : "ended";
      node.append(note("span", m.text, "why"));
      // A turn cut off by the app closing is the one ending worth offering to
      // do again: nothing went wrong with it, it was simply never finished.
      // And there is no answer to hang the ordinary "Ask again" on, because
      // never getting one is the whole of what happened.
      if (m.cutOff) {
        const t = talking();
        const asked = t && [...t.messages].reverse().find((x) => x.kind === "mine");
        if (asked) {
          const again = document.createElement("button");
          again.type = "button";
          again.className = "again";
          again.textContent = "Run it again";
          again.onclick = () => sayIt(asked.text);
          node.append(again);
        }
      }
      return node;
    }
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
/**
 * Somebody is being asked to come and do one thing.
 *
 * The end of the road that is not really the end of one. An agent that meets a
 * sign-in, a code sent to a phone, or a card number has to stop, and stopping
 * used to be all it could do: it wrote a sentence about what somebody would
 * have to go and do, the errand ended, and whatever it had arranged half way
 * through stayed half arranged.
 *
 * What it never does is the thing itself. The page opens in their own browser
 * and they type into the real site, in a window this app is not driving and
 * cannot read.
 */
function handItOver(m) {
  const card = document.createElement("li");
  card.className = m.answered ? "handover done" : "handover";
  card.dataset.handover = m.handover || "";

  const words = document.createElement("div");
  words.className = "question";
  words.append(note("p", m.what, "wants"));
  if (m.why) words.append(note("p", m.why, "doing"));
  if (m.where) {
    const link = document.createElement("a");
    link.className = "detail";
    link.href = m.where;
    link.textContent = m.where;
    link.onclick = (e) => {
      e.preventDefault();
      invoke("show_in_browser", { url: m.where }).catch(() => {});
    };
    words.append(link);
  }

  if (m.answered) {
    words.append(note("p", m.answered, "answered"));
  } else {
    // Whether the agent is still parked on this, or gave up while somebody was
    // away doing it. The second is not rare and used to be the end of the
    // errand: granting an app Full Disk Access means quitting that app, so the
    // one permission somebody is most likely to be asked for is the one that
    // takes the waiting agent down with it.
    const waiting = m.stillThere !== false;
    if (!waiting) {
      words.append(
        note(
          "p",
          "It stopped waiting while you were away. Answer anyway and it will pick up where it left off.",
          "answered",
        ),
      );
    }
    const choices = document.createElement("div");
    choices.className = "choices";
    const press = (label, how, said, carryOn) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = label;
      b.onclick = async () => {
        m.answered = said;
        stillWaiting.delete(m.handover);
        drawMessages();
        // Into the call that is waiting, or into the conversation when there is
        // none. Said rather than dropped: the agent has the whole transcript
        // and knows what it asked for, so a sentence is enough to carry on.
        if (!waiting) return sayIt(carryOn);
        try {
          await invoke("handed_back", { handover: m.handover, how });
        } catch (why) {
          m.answered = String(why);
          drawMessages();
        }
      };
      return b;
    };
    choices.append(
      press(
        waiting ? "I have done it" : "I have done it, carry on",
        "done",
        "You said you had done it",
        "I have done what you asked. Carry on.",
      ),
      press("Skip this", "skipped", "You skipped it", "Skip that, and carry on without it."),
    );
    words.append(choices);
  }

  card.append(tile("person", true), words);
  return card;
}

/**
 * How often this same program has already been approved here.
 *
 * By the program rather than by the whole command line, because that is what
 * Always would allow and the thing somebody is actually tired of being asked
 * about: `curl -s one` and `curl -s another` are two questions and one
 * decision.
 */
function saidYesToThisBefore(m) {
  const program = (said) => String(said || "").trim().split(/\s+/)[0] || "";
  const mine = program(m.detail);
  const said = (one) =>
    one.kind === "asking" && one.answered && one !== m && !/^You said no/i.test(one.answered);
  return (talking()?.messages || []).filter((one) => {
    if (!said(one)) return false;
    // The command where both have one, which is the sharper answer: `curl -s a`
    // and `curl -s b` are two questions and one decision.
    //
    // The tool otherwise. A question read back off disk has no command against
    // it -- only a live one does -- so comparing commands alone would count
    // nothing the moment a conversation is reopened, which is exactly when
    // somebody has been asked the same thing often enough to be tired of it.
    const theirs = program(one.detail);
    return mine && theirs ? theirs === mine : one.tool === m.tool;
  }).length;
}

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
    // How many times this same program has already been approved here. The
    // whole complaint, in one number: somebody who has said yes to curl three
    // times is not weighing the fourth question, and the button that would end
    // it was the plain one beside the accented one they keep pressing.
    const overAndOver = m.can_remember ? saidYesToThisBefore(m) : 0;

    if (m.can_remember) {
      // What it will allow, on the button. "Always" on its own is not a choice
      // anybody can make: for a shell command it now allows every use of that
      // program, which is a real widening and has to be visible before it is
      // pressed rather than discoverable afterwards in a list.
      const always = say(
        m.allows ? `Always · ${m.allows}` : "Always",
        "always",
        // Made the obvious button once the same thing has been approved before.
        // First time round, Yes leading is right: nobody should be nudged into
        // a standing grant they have not thought about. By the second, the app
        // has watched somebody answer the same question twice and going on
        // pointing at Yes is the app knowing better and saying nothing.
        overAndOver > 0 ? "always leading" : "always",
      );
      always.title = m.allows
        ? `From now on this agent may do ${m.allows} without asking. You can take it back under Allowed.`
        : "";
      choices.append(always);
      choices.append(say("Just this once", "yes", overAndOver > 0 ? "yes plain" : "yes"));
    } else {
      choices.append(say("Yes", "yes", "yes"));
    }

    // Said, not just implied by a button changing colour. A number somebody can
    // check is the difference between an offer and a nag.
    if (overAndOver > 0) {
      const already = document.createElement("p");
      already.className = "over-and-over";
      already.textContent =
        overAndOver === 1
          ? `You allowed this once already in this conversation.`
          : `You have allowed this ${overAndOver} times already in this conversation.`;
      body.append(already);
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
  // The call was waiting on exactly this. Whichever way it was answered, it is
  // not waiting any more: the turn either carries on, and ends normally, or it
  // stops and the call picks the conversation back up.
  if (inACall && itsYourTurn === "waiting") {
    itsYourTurn = t.working ? "working" : "listening";
    if (!t.working) listenAgain();
  }
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
/**
 * Somebody clicked a notification.
 *
 * Which is the whole point of posting one. Before this it only brought the app
 * forward, and finding the conversation it was about was left to the person who
 * had just been told about it, which is the errand they were trying not to run.
 */
/**
 * A model turned out to hold something other than what was written down.
 *
 * Rare, and worth acting on when it happens: somebody restarted a server with
 * a bigger window, and until the picker is read again the Settings screen is
 * showing last week's number while the app has already stopped believing it.
 */
listen("models_changed", () => {
  thePickerHasChanged();
  // Only where somebody is looking at it. Redrawing a screen that is closed is
  // work nobody asked for, and the list is read fresh the next time it opens.
  if (!document.getElementById("models")?.hidden) drawChosen();
});

listen("go_to", async ({ payload }) => {
  const id = String(payload || "");
  if (!id) return;
  // It may belong to an agent this window has never loaded, which is the usual
  // case: a routine fired on one nobody has opened.
  if (!talks.has(id)) {
    try {
      const owner = await invoke("conversation_agent", { id });
      if (owner) await openAgent(owner);
    } catch {
      // Nothing to go to. Better to leave the window where it is than to jump
      // somewhere arbitrary on the strength of a notification.
      return;
    }
  }
  if (talks.has(id)) await show(id);
});

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

// Somebody is wanted at the keyboard. Its own listener rather than a kind
// inside `happened`, because it is the app asking rather than an engine
// saying: nothing about it came from the conversation's own stream.
listen("handing_over", ({ payload }) => {
  const t = talks.get(payload.conversation);
  if (!t) return;
  stillWaiting.add(payload.handover);
  t.working = false;
  t.messages.push({
    kind: "over_to_you",
    seq: payload.seq,
    handover: payload.handover,
    what: payload.what,
    why: payload.why,
    where: payload.where,
    answered: null,
  });
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
      // In a call this is the end of the loop unless something says so. It is
      // not an ending the call listens for, and a card cannot be answered by
      // voice, so it says what it is waiting for and waits: deaf on purpose,
      // because anything said now would be sent as the next errand rather than
      // as an answer to this.
      if (inACall && payload.conversation === showing) waitingOnYou(payload.asking);
      break;
    }

    case "done":
      // Half a sentence must not outlive the turn writing it. Normally the
      // settled line has already cleared it; a turn stopped mid-word has not.
      itHasStopped(t);
      // Either end of the turn can finish last: a short answer is read out
      // before the turn ends and a long one is still being read after it.
      if (inACall && payload.conversation === showing) listenAgain();
      // Naming is the agent's own job now, asked for after this by the app.
      break;

    case "failed":
      itHasStopped(t);
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
  // Sent is not half typed. Without this the draft comes back the next time
  // this conversation is opened, under the message it already became.
  halfTyped.delete(showing);
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

/**
 * One picture, as big as the window will allow.
 *
 * A thumbnail in a thread is enough to recognise a screenshot and not enough to
 * read one, and the thread is the wrong shape for reading one anyway.
 */
window.addEventListener("look-closer", (e) => lookCloser(e.detail.url, e.detail.name));

function lookCloser(url, name) {
  const over = document.createElement("div");
  over.className = "closer";
  const img = document.createElement("img");
  img.src = url;
  img.alt = name || "a picture you sent";
  over.append(img);
  const away = () => {
    over.remove();
    window.removeEventListener("keydown", onKey);
  };
  // Escape as well as a click, because a thing covering the window with no
  // visible way out is the one kind of overlay people get stuck in.
  const onKey = (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      away();
    }
  };
  over.onclick = away;
  window.addEventListener("keydown", onKey);
  document.body.append(over);
}

/**
 * The pictures on one line, drawn into it.
 *
 * `showing` is what was just sent and is already in hand as data URLs;
 * `pictures` is what was read back off disk and has to be asked for. Both end
 * up as the same row of thumbnails, so a message looks the same the moment it
 * is sent and a week later.
 */
function showThePictures(node, m) {
  const row = document.createElement("div");
  row.className = "pictures";
  node.append(row);

  const show = (url, name) => {
    const img = document.createElement("img");
    img.src = url;
    img.alt = name || "a picture you sent";
    img.loading = "lazy";
    // Bigger, here. Not handed to the system: `show_in_browser` takes http and
    // https and is right to refuse a data URL, and writing the bytes to a
    // temporary file to open in Preview would leave somebody's screenshot lying
    // about on disk for the sake of a click.
    img.onclick = () => lookCloser(url, img.alt);
    row.append(img);
  };

  for (const url of m.showing || []) show(url);
  for (const name of m.pictures || []) {
    invoke("a_picture", { conversation: showing, name })
      .then((url) => show(url, name))
      .catch(() => {
        // A picture that is no longer on disk is said rather than left as a
        // gap: an empty space where one used to be reads as the window being
        // broken, not as a file having gone.
        const gone = document.createElement("span");
        gone.className = "picture-gone";
        gone.textContent = "a picture that is no longer here";
        row.append(gone);
      });
  }
}

/** Say something to the thread that is open, from wherever it was typed. */
async function sayIt(text) {
  const t = talking();
  if (!t) return;
  const going = attached.map((one) => one.url);
  attached = [];
  drawAttached();
  // Shown from the moment it is sent, out of what is already in hand, rather
  // than waiting for a round trip to disk and back to see what was attached.
  const mine = { kind: "mine", text, showing: going };
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
  // Whatever was answering has been killed, and a killed engine says nothing
  // about having stopped.
  itHasStopped(talk);
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
/**
 * The best line found in each agent, while a search is on.
 *
 * One per agent, because the row is what somebody clicks: showing four lines
 * under one name makes the list a result page rather than a list of agents.
 * The conversation and line number on it are what turn a click into arriving
 * at the line instead of near it.
 */
let hitsByAgent = new Map();

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
    hitsByAgent = new Map();
    drawThreads();
    return;
  }
  // camelCase on the way over: the command takes `looking_for` and the bridge
  // renames it. Every other command here has single-word arguments, so this is
  // the first place it could show up, and it showed up as a red line in a
  // thread rather than as anything a stub would have caught.
  const [found, where] = await Promise.all([
    invoke("matching", { lookingFor }),
    // Where the words actually are. The expensive half of this was already
    // being done and thrown away: the query finds the exact line and returned
    // the agent, so somebody searching for a phrase they remember was dropped
    // into whichever conversation spoke most recently and scrolled by hand.
    invoke("hits", { lookingFor }).catch(() => []),
  ]);
  narrowedTo = new Set(found.map((t) => t.id));
  hitsByAgent = new Map();
  for (const hit of where) {
    if (!hitsByAgent.has(hit.agent)) hitsByAgent.set(hit.agent, hit);
  }
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

/**
 * What can be done to the conversation on screen.
 *
 * Errand shipped the better model -- several conversations to an agent, in a
 * picker -- and then gave the picker nothing to tell its entries apart. The
 * three names the app invents are "First", "New conversation" and "{name},
 * again", none of them chosen by a person, so after a week the list repeats one
 * word and the only way to find the right conversation is to open each of them.
 *
 * On the picker rather than in the header, because the picker is where somebody
 * is already looking when they cannot find the one they want.
 */
el.talks.addEventListener("contextmenu", (e) => {
  const t = talks.get(showing);
  if (!t) return;
  e.preventDefault();
  openTheTalkMenu(t, e.clientX, e.clientY);
});

function openTheTalkMenu(t, x, y) {
  menuIsFor = t.id;
  const items = [];
  const item = (label, run, how = "") => {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("role", "menuitem");
    if (how) b.className = how;
    b.textContent = label;
    b.onclick = async (e) => {
      e.stopPropagation();
      await run(b);
    };
    items.push(b);
    return b;
  };

  item("Rename this conversation…", async () => {
    closeTheMenu();
    const called = prompt("What is this conversation about?", t.name);
    // Cancelled is not "call it nothing". An empty name would leave a blank
    // row in the picker, which is worse than the name it already had.
    if (called === null || !called.trim()) return;
    t.name = called.trim();
    drawTalks();
    try {
      await invoke("call_it", { id: t.id, name: t.name });
    } catch (why) {
      complain(String(why));
    }
  });

  const remove = item(
    "Delete this conversation",
    async (b) => {
      // The second press answers the question the first press asked, the same
      // way an agent is deleted. A dialog over the menu would be a different
      // gesture for the same decision.
      if (b.dataset.sure !== "true") {
        b.dataset.sure = "true";
        b.textContent = `Delete ${inAFewWords(t.name)}? Everything said in it goes too`;
        placeTheMenu();
        return;
      }
      closeTheMenu();
      try {
        await invoke("forget_conversation", { id: t.id });
      } catch (why) {
        // The store refuses to delete the only conversation an agent has, and
        // says what to do instead. Worth showing rather than swallowing.
        complain(String(why));
        return;
      }
      const agent = t.agent;
      talks.delete(t.id);
      const left = [...talks.values()].find((other) => other.agent === agent);
      if (left) await show(left.id);
      else await openAgent(agent);
      drawTalks();
    },
    "danger",
  );
  remove.dataset.sure = "false";

  el.menu.replaceChildren(...items);
  el.menu.hidden = false;
  placeTheMenu(x, y);
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
  el.routinePause.textContent = mine?.off ? "Start again" : "Pause";
  el.routinePause.hidden = !mine;
  offerWhatWasAskedHere(el.routineSaid, el.routineWhat);
  el.routine.hidden = false;
  drawHowItWent(t.id);
  el.routineAt.focus();
});

/**
 * The errands actually asked here, to take rather than retype.
 *
 * This is the end of the loop the whole app is for: you say what you want, it
 * tries, you correct it, and the version that finally worked is the one worth
 * having every morning. Until now that version lived only in the conversation,
 * and setting it to repeat meant reading it off the screen and typing it out
 * again from memory -- which is how a routine ends up being a slightly
 * different job from the one that was tested.
 *
 * Offered rather than filled in. Neither the first thing asked nor the last is
 * reliably the right one: the first is usually the fullest and the last is
 * often "yes, that one". The person who refined it knows which, and nobody
 * else can.
 *
 * @param {HTMLElement} where the row to draw them in
 * @param {HTMLInputElement} into the box a chosen one goes into
 */
function offerWhatWasAskedHere(where, into) {
  const t = talking();
  const asked = t
    ? t.messages
        .filter((m) => m.kind === "mine" && !m.text.startsWith(NOT_ASKED_BY_ANYBODY))
        .map((m) => m.text)
    : [];
  // Newest first, because the refined one is nearer the bottom, and without
  // repeats: asking the same thing twice is ordinary and two identical chips
  // are not a choice.
  const distinct = [...new Set(asked.map((x) => x.trim()).filter(Boolean))].reverse();
  // Enough to find the one you mean, few enough to read at a glance. A long
  // conversation would otherwise put thirty of these across the panel.
  const few = distinct.slice(0, 4);
  where.hidden = few.length === 0;
  if (!few.length) return;

  const label = document.createElement("span");
  label.className = "from-here-what";
  label.textContent = "Asked here:";
  const chips = few.map((said) => {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "from-here-one";
    // Cut for the chip, whole into the box and whole in the tooltip: the end of
    // a long errand is often the part that made it work.
    chip.textContent = said.length > 52 ? `${said.slice(0, 52).trimEnd()}…` : said;
    chip.title = said;
    chip.onclick = () => {
      into.value = said;
      into.dispatchEvent(new Event("input"));
      into.focus();
    };
    return chip;
  });
  where.replaceChildren(label, ...chips);
}

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

el.watchAt.addEventListener("input", sayWhatItWouldDo);
el.watchWhat.addEventListener("input", sayWhatItWouldDo);
el.watchOften.addEventListener("change", sayWhatItWouldDo);

el.routineAt.addEventListener("input", sayIfSomethingElseAlreadyDoesThis);
el.routineWhat.addEventListener("input", sayIfSomethingElseAlreadyDoesThis);

/** When it next runs, in words, or what is wrong with what was typed. */
function sayWhen(routine) {
  if (!routine) return "This runs only when you ask it to.";
  // Off is not gone, and the line has to say which. A paused routine that read
  // as "next Tuesday" would be a promise the app has no intention of keeping.
  if (routine.off) {
    return `Paused. It would run ${routine.at}, and will again when you start it.`;
  }
  if (!routine.due) return `${routine.at} · nothing due`;
  const due = new Date(routine.due);
  const ran = routine.ran ? ` · last ran ${new Date(routine.ran).toLocaleString()}` : " · never run";
  return `Next ${due.toLocaleString()}${ran}`;
}

/**
 * How a routine has actually been going.
 *
 * The question people ask about a standing job is not when it is next but
 * whether it has been working. Failures do land in the conversation as red
 * lines, so the record existed; what did not was any way to see three of them
 * in a row without scrolling back through three mornings of transcript.
 */
async function drawHowItWent(id) {
  el.routineWent.hidden = true;
  el.routineWentList.replaceChildren();
  let went = [];
  try {
    went = await invoke("how_it_has_been_going", { id });
  } catch {
    return;
  }
  if (!went.length) return;
  el.routineWentList.replaceChildren(
    ...went.map((run) => {
      const li = document.createElement("li");
      // A run with nothing against it never came back: the app was quit, or
      // the machine slept. That is its own outcome and not a failure.
      li.className = !run.outcome ? "went unfinished" : run.outcome === "done" ? "went" : "went wrong";
      const when = document.createElement("span");
      when.className = "went-when";
      when.textContent = new Date(run.at).toLocaleString();
      const what = document.createElement("span");
      what.className = "went-what";
      what.textContent = !run.outcome
        ? "did not finish"
        : run.outcome === "done"
          ? startedBy(run.why)
          : run.outcome;
      li.append(when, what);
      return li;
    }),
  );
  el.routineWent.hidden = false;
}

/** What started a run, in a word somebody would use. */
function startedBy(why) {
  if (why === "watch") return "woken by a change";
  if (why === "hand") return "you asked";
  return "ran";
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

/**
 * Say it now, without changing when it next runs.
 *
 * The missing half of the loop this whole app is for. You could set something
 * to run every morning and there was no way to find out what it actually did
 * until a morning had gone past, so the first run of a routine was always in
 * front of nobody -- which is the one run you would most want to watch.
 *
 * What is in the box rather than what was saved, because the point is to try
 * the version you are about to keep. Nothing is saved by trying, and nothing
 * about the schedule moves: a trial that counted as the morning's run would
 * take away the run it was supposed to be rehearsing.
 *
 * @param {HTMLInputElement} box where the words are
 * @param {HTMLElement} says the line under the panel
 * @param {HTMLElement} panel the panel to put away, so the errand can be seen
 */
async function tryItNow(box, says, panel) {
  const what = box.value.trim();
  if (!what) {
    says.textContent = "There is nothing to try yet. Say what it should do first.";
    return;
  }
  panel.hidden = true;
  await sayIt(what);
}

el.routineTry.addEventListener("click", () =>
  tryItNow(el.routineWhat, el.routineSays, el.routine),
);
el.watchTry.addEventListener("click", () => tryItNow(el.watchWhat, el.watchSays, el.watching));

// Nothing the app writes on somebody's behalf is something they asked for. The
// goal instruction is sent as though typed, because that is what the engine has
// to receive, and it turned up in this list as an errand to repeat.
const NOT_ASKED_BY_ANYBODY = "This conversation has a goal";

el.routinePause.addEventListener("click", async () => {
  const t = talking();
  if (!t || !theRoutineShown) return;
  const off = !theRoutineShown.off;
  try {
    await invoke("routine_off", { id: t.id, off });
  } catch (why) {
    el.routineSays.textContent = String(why);
    return;
  }
  theRoutineShown = { ...theRoutineShown, off };
  el.routinePause.textContent = off ? "Start again" : "Pause";
  el.routineSays.textContent = sayWhen(theRoutineShown);
  // The clock on the conversation goes with it: a paused routine is not one
  // the picker should still be advertising as scheduled.
  t.repeats = !off;
  drawTalks();
});

el.routineStop.addEventListener("click", async () => {
  const t = talking();
  if (!t) return;
  await invoke("runs", { id: t.id, at: null, what: null });
  theRoutineShown = null;
  el.routinePause.hidden = true;
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

/**
 * Allow something before it has interrupted anybody.
 *
 * Narrowed by the app in exactly the way pressing Always narrows it, and said
 * back in the same words, so that a rule written here and one granted on a
 * card are visibly the same kind of thing rather than two systems that happen
 * to share a table.
 */
el.allowAhead?.addEventListener("submit", async (e) => {
  e.preventDefault();
  const a = whose();
  const what = el.allowWhat.value.trim();
  if (!a || !what) {
    el.allowSays.textContent = "Say what it may do, like `curl` or `git status`.";
    return;
  }
  try {
    const covers = await invoke("allow_in_advance", {
      agent: a.id,
      tool: el.allowTool.value,
      rule: what,
    });
    // What it actually allows, not what was typed. `git status` becomes any
    // git command, and somebody has to be told that rather than find out.
    el.allowSays.textContent = `Allowed: ${covers}.`;
    el.allowWhat.value = "";
    drawGranted();
  } catch (why) {
    el.allowSays.textContent = String(why);
  }
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
  await drawAlsoAllowed();
}

/**
 * What the engine allows on its own, which this app can show and cannot revoke.
 *
 * The list above is what somebody agreed to here and every line of it can be
 * taken back here. It was never the whole answer and this panel said it was:
 * Claude Code reads rules of its own out of its settings files, and they are in
 * force for every errand run through this app. An allowlist you cannot read is
 * not a boundary, which is a thing this app says out loud about somebody else's
 * arrangement while keeping half of one itself.
 *
 * Shown apart, and never with a Take back beside it. A button that edited
 * somebody's settings file from in here would be this app quietly writing rules
 * into the one place it cannot show them, which is the thing the whole
 * arrangement exists to avoid.
 */
async function drawAlsoAllowed() {
  const a = whose();
  let theirs;
  try {
    theirs = await invoke("also_allowed", { agent: a.id });
  } catch {
    // Somebody else's files, and not being able to read them says nothing
    // about this agent. Silence beats a red line about a file nobody here
    // wrote.
    el.alsoAllowed.hidden = true;
    return;
  }

  // Said by the app, which knows what this agent's posture puts on the command
  // line. Written here as well, it was the same sentence in two places, and one
  // of them did not know the thing that decides whether it is true.
  const mode = theirs.mode_says;
  const rows = [...theirs.allow, ...theirs.deny.map((d) => ({ ...d, refused: true }))];
  el.alsoAllowed.hidden = !rows.length && !mode;
  if (el.alsoAllowed.hidden) return;

  // Everything worth reading first, and the list last. Nineteen rules is an
  // ordinary number to have, and with the list in the middle the sentence under
  // it was pushed out of a panel that is a third of a short window: three rules
  // were visible and the rest of the answer was somewhere below the fold.
  const parts = [];
  parts.push(
    note("p", "Claude Code also allows these on its own. Errand cannot take them back here.", "also-what"),
  );
  // The loudest thing first where there is one: a mode that asks nothing makes
  // every list on this screen beside the point, and a list of careful rules
  // above it reads as a boundary that is not there.
  if (mode) {
    // Red only for the one that actually decides. A mode Errand overrules is
    // worth saying and is not an alarm, and colouring it like one is how a
    // screen full of red teaches somebody to ignore red.
    const loud = theirs.mode?.managed === true;
    parts.push(note("p", mode, loud ? "also-mode" : "also-what"));
  }
  // The other half of what runs without asking, and the half that is not a
  // list at all. Checked rather than assumed: `echo hello-from-errand` ran in
  // this app with nothing in either list covering it, and `ls -la /private/tmp`
  // in the same agent a minute later stopped and asked.
  parts.push(
    note(
      "p",
      "Some commands the engine judges harmless it runs without asking either list.",
      "also-what",
    ),
  );

  const list = document.createElement("ul");
  list.className = "also-list";
  for (const one of rows) {
    const row = document.createElement("li");
    const what = document.createElement("span");
    what.textContent = one.refused ? `Refused · ${one.rule}` : one.rule;
    const where = document.createElement("span");
    where.className = "where";
    where.textContent = one.whose;
    row.append(what, where);
    list.append(row);
  }
  if (rows.length) parts.push(list);
  el.alsoAllowed.replaceChildren(...parts);
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
  add("What changed in this one", "since the version before it", () => whatChanged(), true);
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
      itHasStopped(talks.get(stopping));
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

/**
 * Finding words in the conversation that is open.
 *
 * A different question from the search in the corner, which finds which
 * conversation had them. This one is the reflex every other Mac app answers
 * and this one silently ignored, which is the worst thing a keystroke can do:
 * nothing happening reads as broken rather than as absent.
 */
let foundHere = [];
let atHit = -1;

function findInHere() {
  const what = el.findingWhat.value.trim().toLowerCase();
  foundHere = [];
  atHit = -1;
  for (const was of el.messages.querySelectorAll(".found")) was.classList.remove("found");
  if (!what) {
    el.findingCount.textContent = "";
    return;
  }
  foundHere = [...el.messages.children].filter(
    (li) => !li.classList.contains("day") && li.textContent.toLowerCase().includes(what),
  );
  // Said as a count rather than left to be counted. "No matches" and "one of
  // forty" are different situations and only one of them is worth stepping
  // through.
  el.findingCount.textContent = foundHere.length ? `1 of ${foundHere.length}` : "none";
  if (foundHere.length) stepTo(0);
}

function stepTo(nth) {
  if (!foundHere.length) return;
  const at = (nth + foundHere.length) % foundHere.length;
  for (const was of el.messages.querySelectorAll(".found")) was.classList.remove("found");
  atHit = at;
  foundHere[at].classList.add("found");
  foundHere[at].scrollIntoView({ block: "center" });
  el.findingCount.textContent = `${at + 1} of ${foundHere.length}`;
}

function openFinding() {
  el.finding.hidden = false;
  el.findingWhat.focus();
  el.findingWhat.select();
  findInHere();
}

function closeFinding() {
  el.finding.hidden = true;
  foundHere = [];
  atHit = -1;
  for (const was of el.messages.querySelectorAll(".found")) was.classList.remove("found");
  el.what?.focus();
}

el.findingWhat.addEventListener("input", findInHere);
el.findingNext.addEventListener("click", () => stepTo(atHit + 1));
el.findingPrev.addEventListener("click", () => stepTo(atHit - 1));
el.findingDone.addEventListener("click", closeFinding);
el.findingWhat.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    stepTo(atHit + (e.shiftKey ? -1 : 1));
  } else if (e.key === "Escape") {
    e.preventDefault();
    closeFinding();
  }
});

window.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
    e.preventDefault();
    el.finding.hidden ? openFinding() : closeFinding();
    return;
  }
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

      // What it is actually printing. Until now only the model could see this:
      // it reaches the kept output through check_command and nothing else did,
      // which is the wrong way round for the one person who can decide to stop
      // it. Shown here rather than taken, so watching a build does not steal
      // the lines the model is about to be given.
      if (one.tail) {
        const printing = document.createElement("pre");
        printing.className = "tail";
        printing.textContent = one.tail.trimEnd();
        row.append(printing);
      }

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

/** Which button is showing that the microphone is on. */
let lit = null;

function stopListening() {
  if (!ears) return;
  const going = ears;
  ears = null;
  lit?.setAttribute("aria-pressed", "false");
  lit = null;
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
function startListening({ sendOnPause = false, lights = el.speak } = {}) {
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
    if (inACall && itsYourTurn === "listening") {
      startListening({ sendOnPause: true, lights: el.call });
    }
  };
  hearing.onerror = (e) => {
    // Only the ones that mean the ears are actually gone. `no-speech` is
    // raised as a matter of course after a stretch of quiet, and ending a call
    // on it meant that thinking for a few seconds about what to ask hung up on
    // you: the mic went off, the placeholder went back, and the next thing you
    // said went nowhere. That silence is what `onend` restarts from, eight
    // lines above, and this was taking the call away before it could.
    const gone = e.error === "not-allowed" || e.error === "service-not-allowed" || e.error === "audio-capture";
    if (inACall && gone) endTheCall();
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
    // Whichever button asked for the ears is the one that shows they are on.
    // Both lit at once, the composer had two identical pulsing circles beside
    // each other and nothing to say which of the two things was happening.
    lit = lights;
    lit?.setAttribute("aria-pressed", "true");
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


/**
 * Say what it has stopped to ask, and wait to be told.
 *
 * Where it can be answered as well as what it is: somebody in a call is by
 * definition not looking at the window, and "it needs permission" without
 * "in the Errand window" leaves them waiting for a question they cannot hear
 * the rest of.
 */
function waitingOnYou(asking) {
  sayOutLoud(
    `It stopped to ask permission to ${asking}. Answer it in the Errand window.`,
    "waiting",
  );
}

/** How long a silence means somebody has finished a sentence. */
const ENOUGH_OF_A_PAUSE = 1400;

let inACall = false;
/**
 * What the call is doing: listening, working, speaking, or waiting on you.
 *
 * The last is the one that is not obvious and is also the common case: the
 * default posture is to ask before touching the machine, so an errand said out
 * loud stops on a permission card more often than not. That is not an ending
 * the call listens for, and a card cannot be answered by voice, so it says what
 * it is waiting for and then waits, deaf on purpose. Without this it went deaf
 * silently and never listened again, which from across a room is a call that
 * hung up for no reason.
 */
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
  if (!startListening({ sendOnPause: true, lights: el.call })) endTheCall();
});

// The way out that somebody reaches for without thinking. A call is the one
// state in this window where the keyboard is not where their hands are, and
// the one they most want to be able to leave quickly.
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || !inACall) return;
  // Not while something else is in front of it. Escape closes the thing you
  // are looking at, and dismissing the palette while ending a call as a side
  // effect is one keypress doing two things, only one of which anybody meant.
  if (!el.palette.hidden || !el.whois.hidden) return;
  endTheCall();
});

/**
 * Read a line of the answer out, and listen again when there is nothing left.
 *
 * Said line by line as they settle rather than all at once at the end, so the
 * first paragraph is being read while the second is still being written. The
 * browser queues them in order, which is the whole of the ordering logic here.
 */
function sayOutLoud(text, thenWhat = "speaking") {
  const saying = toSay(text);
  if (!saying) {
    // Nothing worth saying, and still something worth remembering: a call that
    // is waiting must not go back to listening because the sentence about it
    // happened to be empty.
    itsYourTurn = thenWhat === "waiting" ? "waiting" : itsYourTurn;
    return;
  }
  itsYourTurn = "speaking";
  stopListening();
  // What the call is doing once this has been said. Everything is back to
  // listening except a question, which is nobody's turn but yours.
  const after = thenWhat;

  const utterance = new SpeechSynthesisUtterance(saying);
  utterance.lang = navigator.language || "en-US";
  const done = () => {
    if (after === "waiting") {
      itsYourTurn = "waiting";
      return;
    }
    listenAgain();
  };
  utterance.onend = done;
  // A voice that fails silently leaves a call that never listens again, which
  // looks exactly like a call that hung up.
  utterance.onerror = done;
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
  // A question on screen is nobody's turn but yours, and listening through it
  // would send whatever was said as the next errand rather than as an answer.
  if (itsYourTurn === "waiting") return;
  if (Speaking.speaking || Speaking.pending) return;
  const t = talking();
  if (t && t.working) return;
  itsYourTurn = "listening";
  startListening({ sendOnPause: true, lights: el.call });
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
  // The same offer as Repeat, for the same reason: what a watch should say when
  // it wakes is nearly always something already worked out here.
  offerWhatWasAskedHere(el.watchSaid, el.watchWhat);
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

/** What the thing being watched and how often come to, as the app stores it. */
function whatIsBeingWatched() {
  const at = el.watchAt.value.trim();
  return at ? `${at} every ${el.watchOften.value}` : "";
}

/** How often, in the words the list uses rather than in `10m`. */
function howOftenInWords() {
  const chosen = el.watchOften.selectedOptions[0];
  return chosen ? chosen.textContent : "";
}

/**
 * What pressing Save would actually do, in the words of what has been typed.
 *
 * The rest of this panel describes the state it is already in. This one
 * describes the state it would be put into, which is the question somebody
 * filling in a form is actually asking, and the one nothing on the screen
 * answered: what is watched, where, how often, and what happens then.
 */
function sayWhatItWouldDo() {
  const at = el.watchAt.value.trim();
  const what = el.watchWhat.value.trim();
  if (!at && !what) {
    el.watchPlain.textContent =
      "Name a folder, a file or a web address, choose how often to look, and say what " +
      "this agent should do when it changes.";
    return;
  }
  if (!at) {
    el.watchPlain.textContent = "Name a folder, a file or a web address to watch.";
    return;
  }
  const page = /^https?:\/\//i.test(at);
  const looking = page ? `read ${at}` : `look at ${at}`;
  const changed = page ? "If the page has changed" : "If anything there has changed";
  const then = what
    ? `it will ask this agent to ${what.replace(/^(please\s+)?/i, "")}.`
    : "it will wake this agent. Say what it should do, above.";
  el.watchPlain.textContent =
    `It will ${looking} ${howOftenInWords()}, while Errand is open. ${changed}, ${then}`;
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
    // Back into the two controls it was typed into, rather than as the one
    // string it is stored as.
    const stored = now.watches || "";
    const split = stored.lastIndexOf(" every ");
    el.watchAt.value = split > 0 ? stored.slice(0, split) : stored;
    const often = split > 0 ? stored.slice(split + 7).trim() : "";
    if (often && [...el.watchOften.options].some((o) => o.value === often)) {
      el.watchOften.value = often;
    }
    el.watchWhat.value = now.what || "";
  }
  // Only where there is one to stop. Offering to stop something that was never
  // started is the panel asking a question about a state it is not in.
  el.watchStop.hidden = !now.watches;
  sayWhatItWouldDo();
  el.watchAgain.hidden = !now.paused;
  el.watchSays.dataset.paused = String(!!now.paused);

  // Stopped is the thing to say first, because it is the only state somebody
  // has to do something about.
  if (now.paused) {
    el.watchSays.textContent = now.paused;
    return;
  }
  if (!now.watches) {
    // Only the state. What to do about it is the line above, which says it in
    // the words of whatever is half-typed, and two lines telling somebody what
    // to type is one more than anybody reads.
    el.watchSays.textContent = "Nothing is being watched yet.";
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
  // The two controls, put back together into the one line the app stores.
  const watches = whatIsBeingWatched();
  const what = el.watchWhat.value.trim();
  if (!watches || !what) {
    el.watchSays.textContent =
      "It needs something to watch and something for this agent to do when it changes.";
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
      b.title = `${place.url} · ${place.why}`;
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
  await Promise.all([drawChosen(), drawKept(), drawAtLogin(), drawReachable()]);
}

/**
 * What agents can be let at, and whether they are.
 *
 * The switch is the whole of the permission, so what it says beside it has to
 * be enough to decide on: these run in the app rather than in the walled
 * engine, which means confining an agent to its own folder does not decide
 * whether it can read somebody's mail. Turning one on does.
 */
async function drawReachable() {
  let all;
  try {
    all = await invoke("connectors");
  } catch (why) {
    el.reachableList.replaceChildren(note("li", String(why), "nothing"));
    return;
  }
  el.reachableList.replaceChildren(
    ...all.map((one) => {
      const row = document.createElement("li");
      const label = document.createElement("label");
      label.className = "switch";

      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = one.on;
      box.onchange = async () => {
        const wanted = box.checked;
        try {
          await invoke("connect", { id: one.id, on: wanted });
          one.on = wanted;
        } catch (why) {
          // Put back, because a switch showing one thing while the app holds
          // another is worse than the thing not working.
          box.checked = !wanted;
          say(String(why), true);
        }
      };

      const words = document.createElement("span");
      const name = document.createElement("span");
      name.className = "who";
      name.textContent = one.name;
      const sees = document.createElement("span");
      sees.className = "where";
      sees.textContent = one.sees;
      words.append(name, sees);
      label.append(box, words);
      row.append(label);
      return row;
    }),
  );
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
          const kept = JSON.parse(one.settings);
          // How much it holds, said out loud. Errand asks every model this and
          // writes down the answer, and being wrong about it is expensive in
          // both directions: too small drops the conversation four times sooner
          // than it needs to, too large has the request refused outright and
          // reads as a broken model. Neither is visible anywhere else.
          const holds = kept.context_window
            ? ` · holds ${Math.round(kept.context_window / 1000)}k`
            : " · size unknown, assuming 32k";
          where.textContent = `${kept.base_url || ""}${holds}`;
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

/**
 * What changed in this one.
 *
 * Every copy of Errand is installed by hand over the top of the last, so the
 * only moment anybody knows a version has changed is the moment they see
 * something different and wonder whether they imagined it. Shown once for a
 * version and then only when asked for: a panel that comes back at every launch
 * is a panel people learn to close without reading.
 *
 * @param {boolean} asked whether somebody went looking for it, in which case it
 *   opens and closes like every other panel here
 */
async function whatChanged(asked = true) {
  if (asked && !el.changed.hidden) {
    el.changed.hidden = true;
    return;
  }
  let changed;
  try {
    changed = await invoke("what_changed");
  } catch {
    // Notes are not worth a red line in a conversation. Somebody who wanted
    // them and did not get them will ask again; somebody who did not ask
    // should not be told about it at all.
    return;
  }
  if (!changed.notes || (!asked && !changed.first_time)) return;

  // Written down before it is drawn, so a crash while drawing does not mean
  // being shown the same notes at every launch from now on.
  invoke("seen_what_changed").catch(() => {});

  el.changed.hidden = false;
  const head = note("p", `What changed in ${changed.notes.version}`, "changed-what");
  const list = document.createElement("ul");
  list.className = "changed-list";
  for (const line of changed.notes.lines) {
    const one = document.createElement("li");
    one.textContent = line;
    list.append(one);
  }
  const done = document.createElement("button");
  done.type = "button";
  done.className = "tour-done";
  done.textContent = "Got it";
  done.onclick = () => {
    el.changed.hidden = true;
  };
  el.changed.replaceChildren(head, list, done);
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
