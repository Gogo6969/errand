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
import { STATES, SHOWING, shown, stateOf, chipOf, byState as inStateOrder, stillInTheList, inOrder, byTeammate, aMadeUpName, askedBy, aNameFrom, answeredAs, askedForByOthers } from "./jobs.js";
import { render, reachTheAppWith } from "./markdown.js";
import { toSay, worthSaying } from "./speech.js";
import { wordsOf, fitOf, usualWordsFor, leaningWordsFor, theBestFit } from "./fit.js";

const { invoke: invokeTheApp } = window.__TAURI__.core;

/**
 * The commands that change what repeats: routines, watches, and anything that
 * makes or removes an agent that has them. After each, the list of what repeats
 * is read again, so the marks down the side and on the overview's tiles are
 * never a step behind what was just saved.
 */
const CHANGES_WHAT_REPEATS = new Set([
  "runs",
  "routine_off",
  // Finishing a task switches off what it runs, and reopening it switches
  // that back on.
  "finish_task",
  "watch_it",
  "look_again",
  "pause",
  "forget",
  "forget_conversation",
  "duplicate",
  "load_agent",
]);

async function invoke(name, args) {
  const answer = await invokeTheApp(name, args);
  if (CHANGES_WHAT_REPEATS.has(name)) setTimeout(readStanding, 0);
  return answer;
}
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
/**
 * The header on one row while everything in it fits, and on two once it does
 * not.
 *
 * Measured rather than set at a width, because the width it needs changes
 * with what the buttons say and what the pickers hold. Between 700 and 920
 * pixels Goal, Allowed and Tools sat past the edge. Put off until the observer
 * has finished, because changing the layout inside the observer that watches
 * it is a loop WebKit reports as an error, and errors here are said in the
 * conversation. A timer rather than the next frame, which never comes while
 * the window is not being drawn.
 */
function keepTheHeaderInside() {
  const title = document.getElementById("title");
  if (!title || typeof ResizeObserver === "undefined") return;
  let pending = false;
  const measure = () => {
    pending = false;
    title.classList.remove("wraps");
    if (title.scrollWidth > title.clientWidth + 1) title.classList.add("wraps");
  };
  const soon = () => {
    if (pending) return;
    pending = true;
    setTimeout(measure, 0);
  };
  const watching = new ResizeObserver(soon);
  watching.observe(title);
  for (const child of title.children) watching.observe(child);
  // And when what is in it changes: a name or a picker filled in can need more
  // room without any box the observer is watching changing size.
  new MutationObserver(soon).observe(title, {
    subtree: true,
    childList: true,
    characterData: true,
    attributes: true,
    attributeFilter: ["hidden"],
  });
}
keepTheHeaderInside();

function itHasStopped(talk) {
  if (!talk) return;
  talk.working = false;
  talk.writing = "";
  // A step that has not answered by the time its turn is over never will, and
  // a spinner beside it says something is still going on when nothing is.
  for (const m of talk.messages || []) {
    if (m.kind === "doing" && m.outcome == null) m.outcome = "stopped";
  }
}

function complain(why) {
  tellHere(why, true);
}

/**
 * A line from the window, rather than the agent, in the conversation on screen.
 *
 * Not the end of a turn. What it says about is something the window tried, a
 * rename, a link, the microphone, and the agent may be half way through an
 * answer that is still coming: ending the turn here drew the conversation as
 * stopped while it carried on. And not always a failure. "Saved to" was said
 * in red.
 */
function tellHere(text, failed = false) {
  const t = talking();
  if (!t) {
    document.getElementById("thread-name").textContent = text;
    return;
  }
  t.messages.push({ kind: "ended", failed, text });
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
let suggested = null; // the next thing to say, greyed, for Tab: { for, text }
/**
 * How many days a job marked finished stays in the list before it lives only
 * in the overview. A setting, because a week is a guess. Up here, with the
 * rest of what the page holds, because the list is drawn before the code
 * that reads the setting has run.
 */
/** How long a finished task stays in its teammate's task menu, in days. */
const FINISHED_KEPT_DAYS = 7;
/**
 * Every routine and watch, every agent's, as the app last said. Read when the
 * page opens and whenever one changes, and shown as a mark beside each agent
 * that has one, down the side and on the overview.
 */
let standingNow = [];
/** The answer being read aloud this moment, when one is. */
let listeningTo = null;
/** Which line of the picker every teammate works on: Errand's model, or none yet. */
let errandModel = null;
/** The line teammates kept local run on when Errand's model would send words elsewhere. */
let localModel = null;
/**
 * Every task, every teammate's, as the app last said: what matters and what
 * is finished is a task's. Read on opening and with the overview.
 */
let tasksNow = [];

const el = {
  threads: document.getElementById("threads"),
  menu: document.getElementById("menu"),
  messages: document.getElementById("messages"),
  name: document.getElementById("thread-name"),
  purpose: document.getElementById("purpose"),
  engine: document.getElementById("engine"),
  sweeping: document.getElementById("sweeping"),
  setup: document.getElementById("setup"),
  models: document.getElementById("models"),
  modelsDone: document.getElementById("models-done"),
  taskLearn: document.getElementById("task-learn"),
  learnAsks: document.getElementById("learn-asks"),
  learnSkill: document.getElementById("learn-skill"),
  learnSkillSays: document.getElementById("learn-skill-says"),
  learnSkillName: document.getElementById("learn-skill-name"),
  learnNote: document.getElementById("learn-note"),
  learnNoteText: document.getElementById("learn-note-text"),
  learnSays: document.getElementById("learn-says"),
  learnDone: document.getElementById("learn-done"),
  homeBox: document.getElementById("home-box"),
  homePath: document.getElementById("home-path"),
  homeShow: document.getElementById("home-show"),
  homeEdits: document.getElementById("home-edits"),
  checklist: document.getElementById("checklist"),
  checklistSummary: document.getElementById("checklist-summary"),
  checklistStarter: document.getElementById("checklist-starter"),
  checklistStarterSays: document.getElementById("checklist-starter-says"),
  checklistUse: document.getElementById("checklist-use"),
  checklistList: document.getElementById("checklist-list"),
  checklistNew: document.getElementById("checklist-new"),
  checklistPoint: document.getElementById("checklist-point"),
  checklistSays: document.getElementById("checklist-says"),
  teams: document.getElementById("teams"),
  teamsNew: document.getElementById("teams-new"),
  teamsNewTeammate: document.getElementById("teams-new-teammate"),
  teamsList: document.getElementById("teams-list"),
  teamsFree: document.getElementById("teams-free"),
  teamsFreeList: document.getElementById("teams-free-list"),
  overview: document.getElementById("overview"),
  mission: document.getElementById("mission"),
  missionOpen: document.getElementById("mission-open"),
  missionTasks: document.getElementById("mission-tasks"),
  missionTeams: document.getElementById("mission-teams"),
  missionTasksCount: document.getElementById("mission-tasks-count"),
  missionDone: document.getElementById("mission-done"),
  nowCount: document.getElementById("now-count"),
  newTask: document.getElementById("task-chooser"),
  newTaskTitle: document.getElementById("task-chooser-title"),
  newTaskWhat: document.getElementById("task-chooser-what"),
  newTaskSay: document.getElementById("task-chooser-say"),
  newTaskWho: document.getElementById("task-chooser-who"),
  overviewGroup: document.getElementById("overview-group"),
  overviewOrder: document.getElementById("overview-order"),
  overviewAway: document.getElementById("overview-away"),
  overviewTiles: document.getElementById("overview-tiles"),
  overviewFind: document.getElementById("overview-find"),
  overviewShow: document.getElementById("overview-show"),
  taskDone: document.getElementById("task-done"),
  taskCard: document.getElementById("task-card"),
  runningNote: document.getElementById("running-note"),
  runningStop: document.getElementById("running-stop"),
  errandModelSays: document.getElementById("errand-model-says"),
  reachableList: document.getElementById("reachable-list"),
  atLogin: document.getElementById("at-login"),
  atLoginSays: document.getElementById("at-login-says"),
  teammateLogins: document.getElementById("teammate-logins"),
  teammateLoginsList: document.getElementById("teammate-logins-list"),
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
  standing: document.getElementById("standing"),
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
  slash: document.getElementById("slash"),
  replies: document.getElementById("replies"),
  skillsSummary: document.getElementById("skills-summary"),
  skillsList: document.getElementById("skills-list"),
  skillsSays: document.getElementById("skills-says"),
  limitSummary: document.getElementById("limit-summary"),
  limitUsed: document.getElementById("limit-used"),
  limitForm: document.getElementById("limit-form"),
  limitUnit: document.getElementById("limit-unit"),
  limitValue: document.getElementById("limit-value"),
  limitSays: document.getElementById("limit-says"),
  trouble: document.getElementById("trouble"),
  palette: document.getElementById("palette"),
  paletteWhat: document.getElementById("palette-what"),
  paletteList: document.getElementById("palette-list"),
  what: document.getElementById("what"),
  tabHint: document.getElementById("tab-hint"),
  send: document.getElementById("send"),
  form: document.getElementById("composer"),
  new: document.getElementById("new"),
  mark: document.getElementById("mark"),
  find: document.getElementById("find"),
  reach: document.getElementById("reach"),
  talks: document.getElementById("talks"),
  members: document.getElementById("members"),
  rooming: document.getElementById("rooming"),
  roomingWho: document.getElementById("rooming-who"),
  roomingName: document.getElementById("rooming-name"),
  roomingStart: document.getElementById("rooming-start"),
  roomingCancel: document.getElementById("rooming-cancel"),
  roomingSays: document.getElementById("rooming-says"),
  repeat: document.getElementById("repeat"),
  granted: document.getElementById("granted"),
  granting: document.getElementById("granting"),
  asks: document.getElementById("asks"),
  allowed: document.getElementById("allowed"),
  allowAhead: document.getElementById("allow-ahead"),
  handModels: document.getElementById("hand-models"),
  allowWhat: document.getElementById("allow-what"),
  allowTool: document.getElementById("allow-tool"),
  allowChoice: document.getElementById("allow-choice"),
  allowMeans: document.getElementById("allow-means"),
  allowFewer: document.getElementById("allow-fewer"),
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
  routineWentMore: document.getElementById("routine-went-more"),
  routineSays: document.getElementById("routine-says"),
  pause: document.getElementById("pause"),
  more: document.getElementById("more"),
  schedule: document.getElementById("schedule"),
  whois: document.getElementById("whois"),
  whoisName: document.getElementById("whois-name"),
  whoisTitle: document.getElementById("whois-title"),
  whoisAbout: document.getElementById("whois-about"),
  whoisSave: document.getElementById("whois-save"),
  whoisNew: document.getElementById("whois-new"),
  whoisLocal: document.getElementById("whois-local"),
  whoisModel: document.getElementById("whois-model"),
  wordsGo: document.getElementById("words-go"),
  localModel: document.getElementById("local-model"),
  notificationsSays: document.getElementById("notifications-says"),
  notificationsOpen: document.getElementById("notifications-open"),
  sshKeySays: document.getElementById("ssh-key-says"),
  sshKeyLoad: document.getElementById("ssh-key-load"),
  sshKeyAtStart: document.getElementById("ssh-key-at-start"),
  sshKeyAtStartSays: document.getElementById("ssh-key-at-start-says"),
  keyNote: document.getElementById("key-note"),
  keyNoteSays: document.getElementById("key-note-says"),
  keyNoteLoad: document.getElementById("key-note-load"),
  keyNoteClose: document.getElementById("key-note-close"),
  localModelSays: document.getElementById("local-model-says"),
  notesSummary: document.getElementById("notes-summary"),
  notesList: document.getElementById("notes-list"),
  noteNew: document.getElementById("note-new"),
  noteAbout: document.getElementById("note-about"),
  noteText: document.getElementById("note-text"),
  notesSays: document.getElementById("notes-says"),
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

/**
 * Which models in the picker are answering, by the key each is chosen by.
 *
 * The picker offers only what can answer. A model on a server nearby that has
 * not answered a knock for a while is left out, rather than chosen and found
 * dead in a red line afterwards; one that answered a few minutes ago and missed
 * the last knock is kept, because a server restarting is not a server gone.
 * Hosted models and Claude are never knocked on, and are always offered.
 */
const answeredAt = new Map();
const answeredLast = new Map();
// Knocks in a row a server has not answered. One is not enough to call it
// down: the first knock after the app starts can go unanswered by a server
// that is perfectly well, and that turned a working model red on every launch
// it happened to. So a miss is knocked on again shortly, and only a second
// miss in a row counts.
const missed = new Map();
const QUIET_FOR = 10 * 60_000;
let knocking = false;

/** Where a local model is served from, from its settings. */
function addressOf(settings) {
  try {
    return JSON.parse(settings || "{}").base_url || null;
  } catch {
    return null;
  }
}

/** Knock on every model server nearby, and redraw the picker with what answered. */
async function knockOnTheModels() {
  if (knocking) return;
  knocking = true;
  try {
    const nearbyOnes = (await whatCouldAnswer().catch(() => []))
      .filter((c) => c.engine === "local")
      .map((c) => [c.id, addressOf(c.settings)])
      .filter(([, at]) => at);
    if (!nearbyOnes.length) return;
    const answers = await invoke("answering", { addresses: nearbyOnes.map(([, at]) => at) });
    if (!Array.isArray(answers)) return;
    const now = Date.now();
    let again = false;
    nearbyOnes.forEach(([key], i) => {
      // Nothing back means it is out on the internet and was not knocked on.
      if (typeof answers[i] !== "boolean") return;
      if (answers[i]) {
        answeredLast.set(key, true);
        answeredAt.set(key, now);
        missed.delete(key);
        return;
      }
      const times = (missed.get(key) || 0) + 1;
      missed.set(key, times);
      if (times >= 2) answeredLast.set(key, false);
      else again = true;
    });
    if (again) setTimeout(knockOnTheModels, 15_000);
  } catch {
    return;
  } finally {
    knocking = false;
  }
  // Not while somebody has it open: the list changing under the pointer is
  // worse than a list a minute out of date.
  if (document.activeElement !== el.engine) drawEngines();
}

/** Whether the picker offers this one: never knocked on, or answered lately. */
function answersLately(key) {
  if (!answeredLast.has(key)) return true;
  return Date.now() - (answeredAt.get(key) || 0) < QUIET_FOR;
}

// Whenever somebody comes back to the window, which is when they are about to
// choose, and every few minutes regardless.
window.addEventListener("focus", () => knockOnTheModels());
setInterval(knockOnTheModels, 3 * 60_000);

/**
 * Fill Errand's model's menu, in Settings, and mark which it is.
 *
 * One model for every teammate, so this no longer belongs to whichever agent
 * is on screen: the argument some callers still pass is ignored.
 */
async function drawEngines() {
  const mine = errandModel;
  const choices = await whatCouldAnswer();

  // Only what answers, and whatever is chosen whether it answers or not: a
  // menu that hides its own selection is claiming something else.
  const offered = choices.filter((c) => c.id === mine || answersLately(c.id));
  el.engine.replaceChildren(
    ...offered.map((c) => {
      const option = document.createElement("option");
      option.value = c.id;
      option.textContent =
        c.id === mine && answeredLast.get(c.id) === false ? `${c.name} · not answering` : c.name;
      option.selected = c.id === mine;
      return option;
    }),
  );
  const quietNow = Boolean(mine) && answeredLast.get(mine) === false;
  el.engine.classList.toggle("quiet", quietNow);
  el.engine.title = quietNow
    ? "The server for Errand's model is not answering. Choose another model, or start it."
    : "The model every teammate works on";

  if (!mine) {
    // Nothing chosen yet, so each teammate is still on whatever it was given
    // one at a time. Said, rather than showing the first model as though it
    // were chosen.
    const none = document.createElement("option");
    none.value = "__none__";
    none.disabled = true;
    none.selected = true;
    none.textContent = "Not chosen yet";
    el.engine.prepend(none);
  } else if (!choices.some((c) => c.id === mine)) {
    const gone = document.createElement("option");
    gone.value = mine;
    gone.selected = true;
    gone.textContent = "The chosen model · not in the list";
    el.engine.prepend(gone);
  }

  // Below the models: how many were left out and why, and the way to add one.
  el.engine.append(document.createElement("hr"));
  const quiet = choices.length - offered.length;
  if (quiet) {
    const left = document.createElement("option");
    left.disabled = true;
    left.value = "__quiet__";
    left.textContent = `${quiet} more not answering, so not shown`;
    el.engine.append(left);
  }
  const add = document.createElement("option");
  add.value = "__add__";
  add.textContent = "Add a model…";
  el.engine.append(add);
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

/**
 * The same, by task: which of a teammate's tasks the new lines are in, for the
 * dot on each task down the side. A teammate's count said one of its tasks had
 * something; finding which meant opening them in turn.
 */
let freshTasks = new Map();

async function whatIsNew() {
  try {
    const [byTeammate, byTask] = await Promise.all([
      invoke("what_is_new"),
      invoke("what_is_new_in_tasks"),
    ]);
    fresh = new Map(Object.entries(byTeammate || {}));
    freshTasks = new Map(Object.entries(byTask || {}));
  } catch {
    // A count that could not be fetched is no count. Saying "3 new" from a
    // stale answer is worse than saying nothing, because somebody clicks it.
    fresh = new Map();
    freshTasks = new Map();
  }
  drawThreads();
}

/**
 * Whether somebody is looking at the task on screen: the window in front,
 * and not covered by Now or Settings. Only then is something arriving in it
 * read as it arrives; otherwise it waits, marked, for them to come back.
 */
function lookedAt() {
  return (
    document.visibilityState === "visible" &&
    document.hasFocus() &&
    el.overview.hidden &&
    el.models.hidden &&
    el.teams.hidden
  );
}

/** The dot that says something in it has not been read yet. */
function unreadDot(lines) {
  const dot = document.createElement("span");
  dot.className = "unread";
  dot.setAttribute("role", "img");
  const said = `${lines} new, not read yet`;
  dot.setAttribute("aria-label", said);
  dot.title = said;
  return dot;
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

// Coming back to the window is looking at the task on screen: what arrived in
// it while somebody was elsewhere is read the moment they are back.
window.addEventListener("focus", () => {
  if (showing && lookedAt()) nowSeen(showing);
});
document.addEventListener("visibilitychange", () => {
  if (showing && lookedAt()) nowSeen(showing);
});

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

/**
 * The pictures waiting to go, per conversation, for the same reason.
 *
 * They were kept for the window rather than for a conversation, so a
 * screenshot pasted for one agent went to whichever agent was spoken to next,
 * and on to its provider.
 */
const picturesWaiting = new Map();

/**
 * Which conversation the box is holding the words of, if any.
 *
 * Between leaving one conversation and the next one being drawn there can be
 * a round trip to the store, and a second switch in that gap put the first
 * conversation's draft down as the second's.
 */
let theBoxHolds = null;

/**
 * Where Up and Down have walked to through what was asked before. Here with
 * the rest of what the box is holding, because it is a place in one
 * conversation's list and meant nothing in the next one's.
 */
let walkedBack = null;

function putItDown(id) {
  if (!id || !el.what) return;
  const said = el.what.value;
  if (theBoxHolds === id) {
    if (said.trim()) halfTyped.set(id, said);
    else halfTyped.delete(id);
    if (attached.length) picturesWaiting.set(id, attached);
    else picturesWaiting.delete(id);
  } else {
    // Typed while it was still being opened, before its own draft was put
    // back. Kept, but never in place of the draft that was waiting for it.
    if (said.trim() && !halfTyped.has(id)) halfTyped.set(id, said);
    if (attached.length && !picturesWaiting.has(id)) picturesWaiting.set(id, attached);
  }
  theBoxHolds = null;
  el.what.value = "";
  attached = [];
  drawAttached();
  walkedBack = null;
  // Dictation writes into the box, and the box is about to be somebody
  // else's.
  stopListening();
}

function pickItBackUp(id) {
  if (!el.what) return;
  // Anything in the box now was typed or pasted while this was being opened,
  // since leaving the last conversation emptied it. The draft that was
  // waiting comes first; what arrived in between is not thrown away.
  el.what.value = halfTyped.get(id) || el.what.value;
  el.what.style.height = "auto";
  el.what.style.height =
    Math.min(el.what.scrollHeight, window.innerHeight * 0.4) + "px";
  attached = [...(picturesWaiting.get(id) || []), ...attached];
  drawAttached();
  theBoxHolds = id;
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
  if (at < drawnFrom(t)) {
    while (at < drawnFrom(t)) t.earlier = (t.earlier || 0) + 1;
    drawMessages();
  }
  const drawn = el.messages.querySelector(`:scope > [data-seq="${seq}"]`);
  if (!drawn) return;
  for (const was of el.messages.querySelectorAll(".found")) was.classList.remove("found");
  drawn.classList.add("found");
  drawn.scrollIntoView({ block: "center" });
}

function waitingOn(agent) {
  // A handover is waiting on somebody as much as a question is. Only questions
  // counted, so an agent that needed a sign-in said nothing down the side.
  return [...talks.values()].some(
    (t) =>
      t.agent === agent &&
      t.messages.some(
        (m) =>
          !m.answered &&
          (m.kind === "asking" ||
            ((m.kind === "over_to_you" || m.kind === "open_outside") && stillWaiting.has(m.handover))),
      ),
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
async function start({ introduce = false } = {}) {
  const id = uuid();
  agents.set(id, asAgent({ id, name: NOT_YET_NAMED }));
  talks.set(id, asTalk({ id, agent: id, name: "First" }, { loaded: true }));
  // Not written down and nothing started until something is said to it, or it
  // is given a name. A teammate somebody made and then thought better of should
  // not survive as a row in a list, and it certainly should not have cost a
  // process.
  await show(id);
  drawThreads();
  // Made with +, it is asked who it is first: a teammate is somebody's to name,
  // and its job is what it is for. Typing a task instead is fine too.
  if (introduce) {
    el.whois.hidden = true;
    el.name.click();
  } else {
    el.what.focus();
  }
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
  // Through the same box as +, aimed at this teammate: the task is made when
  // something is said in it. Made first, a task nobody went on to ask
  // anything sat on the list as "Nothing asked yet" for good.
  await openNewTask({ aim: { kind: "person", id: a.id } });
}

/* ------------------------------------------------------------ new task -- */

/**
 * A new task: what needs doing, then who does it, a team or one teammate.
 *
 * Who is chosen by the person, with the one whose role, job and skills share
 * the most words with what was written marked as the best fit. Marked and not
 * chosen: matching words is a hint, and a wrong guess sent on its own would
 * be a task in the wrong hands.
 */
let newTaskFor = { teams: [], people: [], brings: new Map() };

/** What a teammate is about, as words to match a task against. Its role
 * brings the usual words for that kind of role, matched as a whole word. */
function profileOf(a) {
  const brought = newTaskFor.brings.get(a.id);
  return [a.name, a.title || "", a.about || "", ...(brought?.skills || []), usualWordsFor(a.title)].join(" ");
}


/**
 * Open the box for a new task. With `aim`, a team or a teammate already
 * chosen: from its own New task button, so Enter gives it to them.
 */
async function openNewTask({ aim = null } = {}) {
  const [teams, brought] = await Promise.all([
    invoke("teams").catch(() => []),
    invoke("what_they_bring").catch(() => []),
  ]);
  const people = teammatesToChoose();
  // The one it is aimed at is listed even before it has a name of its own.
  if (aim?.kind === "person" && !people.some((a) => a.id === aim.id) && agents.has(aim.id)) {
    people.unshift(agents.get(aim.id));
  }
  newTaskFor = {
    teams: teams || [],
    people,
    brings: new Map((brought || []).map(([id, skills, checks, own]) => [id, { skills: skills || [], checks, own }])),
    aim,
  };
  focusBeforeNewTask = document.activeElement;
  el.newTaskWhat.value = "";
  sayWhoDoesIt();
  el.newTask.hidden = false;
  drawNewTask();
  el.newTaskWhat.focus();
}

/** The team or teammate the box is aimed at, with its name, or null. */
function aimedAt() {
  const aim = newTaskFor.aim;
  if (!aim) return null;
  const name =
    aim.kind === "team" ? newTaskFor.teams.find((t) => t.id === aim.id)?.name : agents.get(aim.id)?.name;
  return name ? { ...aim, name } : null;
}

/** The box's title and the line over the list, as they are while nothing is wrong. */
function sayWhoDoesIt() {
  const aimed = aimedAt();
  el.newTaskTitle.textContent = aimed ? `New task for ${aimed.name}` : "New task";
  el.newTaskWhat.placeholder = aimed ? `What should ${aimed.name} do?` : "What needs doing?";
  el.newTaskSay.textContent = aimed ? `Enter gives it to ${aimed.name}.` : "Who should do it?";
  el.newTaskSay.dataset.wrong = "false";
}

/** Asked to choose before anything was written: said once, where it was asked. */
function sayWhatFirst() {
  el.newTaskSay.textContent = "Say what needs doing first, then who does it.";
  el.newTaskSay.dataset.wrong = "true";
  el.newTaskWhat.focus();
}

/** What had the keyboard before the chooser opened. */
let focusBeforeNewTask = null;

/** Put the chooser away; dismissed, focus goes back where it was. */
function closeNewTask({ refocus = false } = {}) {
  el.newTask.hidden = true;
  const back = focusBeforeNewTask;
  focusBeforeNewTask = null;
  if (!refocus) return;
  if (back?.isConnected && back.offsetParent !== null && back !== document.body) back.focus();
  else if (el.mission.hidden) el.what.focus();
}

/** The rows that can be chosen, in the order they are shown. */
function newTaskRows() {
  return [...el.newTaskWho.querySelectorAll('.who:not([aria-disabled="true"])')];
}

function drawNewTask() {
  const text = el.newTaskWhat.value;
  const byId = new Map([...agents.values()].map((a) => [a.id, a]));
  const people = newTaskFor.people.map((a, at) => ({ a, at, fit: fitOf(text, profileOf(a), leaningWordsFor(a.title)) }));
  const fitOfPerson = new Map(people.map((x) => [x.a.id, x.fit]));
  // A team fits by its best member, and better only when several of its
  // people each fit a part: one member's words alone are that member's.
  const team = newTaskFor.teams.map((t) => {
    const lead = t.lead && byId.get(t.lead);
    const crew = [lead, ...t.members.map((m) => byId.get(m))].filter(Boolean);
    const fits = crew.map((a) => fitOfPerson.get(a.id) ?? fitOf(text, profileOf(a), leaningWordsFor(a.title)));
    // Only members that fit by a word of their own count as several.
    const several = fits.filter((f) => f >= 1).length;
    const fit = lead ? Math.max(0, ...fits) + (several >= 2 ? several - 1 : 0) : 0;
    return { t, lead, crew, fit };
  });
  // With something written, the closest first; otherwise in the list's order.
  if (wordsOf(text).length) people.sort((x, y) => y.fit - x.fit || x.at - y.at);
  // The single best, a teammate before a team that only ties with it.
  const ranked = [
    ...people.map((x) => ({ key: `person:${x.a.id}`, fit: x.fit, team: 0 })),
    ...team.filter((x) => x.lead).map((x) => ({ key: `team:${x.t.id}`, fit: x.fit, team: 1 })),
  ].sort((x, y) => y.fit - x.fit || x.team - y.team);
  const bestKey = theBestFit(ranked);
  const mark = (key) => {
    if (key !== bestKey) return null;
    const pill = document.createElement("span");
    pill.className = "fit";
    pill.textContent = "Best fit";
    return pill;
  };

  const heading = (words) => {
    const li = document.createElement("li");
    li.className = "heading";
    li.setAttribute("role", "presentation");
    li.textContent = words;
    return li;
  };
  const row = (kind, id, markEl, name, role, detail, pill, disabled) => {
    const li = document.createElement("li");
    li.className = "who";
    li.setAttribute("role", "option");
    li.setAttribute("aria-selected", "false");
    li.tabIndex = -1;
    li.dataset.kind = kind;
    if (id) li.dataset.id = id;
    if (disabled) li.setAttribute("aria-disabled", "true");
    const lines = document.createElement("div");
    lines.className = "lines";
    const n = document.createElement("span");
    n.className = "name";
    n.textContent = name;
    if (role) {
      const r = document.createElement("span");
      r.className = "role";
      r.textContent = role;
      n.append(" ", r);
    }
    const d = document.createElement("span");
    d.className = "who-line";
    d.textContent = detail;
    d.title = detail;
    lines.append(n, d);
    li.append(markEl, lines, pill || document.createElement("span"));
    if (!disabled) li.addEventListener("click", () => chooseWhoDoesIt(kind, id));
    return li;
  };
  const teamMark = () => {
    const m = document.createElement("span");
    m.className = "team-mark";
    m.innerHTML =
      '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="5.6" cy="6" r="2.2"/><circle cx="10.8" cy="6.8" r="1.7"/><path d="M1.8 13c0-2 1.7-3.2 3.8-3.2S9.4 11 9.4 13"/><path d="M10.6 10c1.9 0 3.6.9 3.6 3"/></svg>';
    return m;
  };

  const items = [];
  if (team.length) {
    items.push(heading("Teams"));
    for (const { t, lead, crew } of team.filter((x) => x.lead)) {
      const others = crew.length - 1;
      items.push(
        row("team", t.id, teamMark(), t.name, "", `Led by ${lead.name} · ${others} ${others === 1 ? "member" : "members"}`, mark(`team:${t.id}`)),
      );
    }
    for (const { t } of team.filter((x) => !x.lead)) {
      items.push(row("team", t.id, teamMark(), t.name, "", "No lead yet: choose one on the Teams tab of Mission Control", null, true));
    }
  }
  if (people.length) {
    items.push(heading("Teammates"));
    for (const { a } of people) {
      const brought = newTaskFor.brings.get(a.id);
      const detail = [
        a.about,
        brought?.own ? `On ${brought.own}` : "",
        brought?.skills?.length ? `Skills: ${brought.skills.join(", ")}` : "",
      ]
        .filter(Boolean)
        .join(" · ");
      items.push(row("person", a.id, tile(kindFor(a), busy(a.id), a.hue), a.name, a.title, detail || "Has not said what it handles", mark(`person:${a.id}`)));
    }
  }
  const plus = document.createElement("span");
  plus.className = "team-mark";
  plus.textContent = "+";
  const someone = row("new", "", plus, "New teammate", "", "Somebody new for this: you name it and give it a job first");
  someone.classList.add("someone-new");
  items.push(someone);

  // Aimed at one: that one, chosen, and a way to everybody else.
  const aimed = aimedAt();
  if (aimed) {
    const theOne = items.find((li) => li.dataset?.kind === aimed.kind && li.dataset.id === aimed.id);
    theOne?.querySelector(".fit")?.remove();
    theOne?.setAttribute("aria-selected", "true");
    const more = document.createElement("span");
    more.className = "team-mark";
    more.textContent = "…";
    const anyone = row("anyone", "", more, "Somebody else", "", "Every team and teammate, with the best fit marked");
    anyone.classList.add("someone-new");
    el.newTaskWho.replaceChildren(...[theOne, anyone].filter(Boolean));
    return;
  }
  el.newTaskWho.replaceChildren(...items);
}

/** Show a row as the one Enter would choose. */
function pickNewTaskRow(li) {
  for (const one of el.newTaskWho.querySelectorAll(".who")) one.setAttribute("aria-selected", String(one === li));
  li?.focus();
  li?.scrollIntoView({ block: "nearest" });
}

/** Full-window things a new task would otherwise start out of sight behind. */
function outOfTheWay() {
  if (!el.mission.hidden) closeMission({ refocus: false });
  if (!el.models.hidden) el.modelsDone.click();
  document.querySelector(".closer")?.remove();
}

/** A choice still being carried out: a second Enter or click waits for it. */
let choosingNow = false;

async function chooseWhoDoesIt(kind, id) {
  if (choosingNow) return;
  const text = el.newTaskWhat.value.trim();
  if (kind === "anyone") {
    newTaskFor.aim = null;
    sayWhoDoesIt();
    drawNewTask();
    el.newTaskWhat.focus();
    return;
  }
  // A task is made with what it is for, never empty: one made first and
  // never asked anything stayed on the list as "Nothing asked yet".
  if (kind !== "new" && !text) {
    sayWhatFirst();
    return;
  }
  choosingNow = true;
  try {
    await carryOutTheChoice(kind, id, text);
  } finally {
    choosingNow = false;
  }
}

async function carryOutTheChoice(kind, id, text) {
  if (kind === "new") {
    closeNewTask();
    outOfTheWay();
    await start({ introduce: true });
    // What was written waits in the box, to send once it has a name.
    if (text) {
      el.what.value = text;
      el.what.dispatchEvent(new Event("input"));
    }
    return;
  }
  let talk;
  try {
    if (kind === "team") {
      const team = newTaskFor.teams.find((t) => t.id === id);
      talk = await invoke("a_task_for_the_team", { team: id });
      talks.set(talk, asTalk({ id: talk, agent: team.lead, name: team.name }, { loaded: false }));
    } else {
      const first = talks.get(id);
      // Never one written down already: a saved agent still called that, with
      // its first conversation not read yet, would have the task said into
      // that conversation instead.
      if (
        agents.get(id)?.name === NOT_YET_NAMED && !agents.get(id)?.spoke &&
        first?.agent === id && first.loaded && !first.messages?.length
      ) {
        // Made in this window and not written down until something is said to
        // it: its first conversation is the task, and saying it writes both
        // down. A conversation of its own would have nobody to belong to yet.
        talk = id;
      } else {
        talk = uuid();
        await invoke("start_conversation", { id: talk, agent: id, name: "New task" });
        talks.set(talk, asTalk({ id: talk, agent: id, name: "New task" }, { loaded: true }));
      }
    }
    // The rest of whoever's task it is, as opening them would: a teammate
    // not opened yet in this window otherwise showed this task alone, in its
    // menu, under its row and on the card, until somebody clicked it.
    const owner = talks.get(talk)?.agent;
    for (const c of (await invoke("conversations", { agent: owner }).catch(() => [])) || []) {
      talks.set(c.id, asTalk(c, talks.get(c.id)));
    }
  } catch (why) {
    // Kept open, with what was written: said where it was asked.
    el.newTaskSay.textContent = `That could not be started: ${why}`;
    el.newTaskSay.dataset.wrong = "true";
    return;
  }
  closeNewTask();
  outOfTheWay();
  await show(talk);
  // Said in that task only. If something else was opened meanwhile, the
  // words wait in that task's box rather than going to whoever is on screen.
  // The keyboard in its box, as when a task was made there, for what comes next.
  if (showing === talk) {
    el.what.focus();
    await sayIt(text);
  } else halfTyped.set(talk, text);
}

el.newTaskWhat.addEventListener("input", () => {
  // Nobody is told off for a box they are still filling in.
  if (el.newTaskSay.dataset.wrong === "true") sayWhoDoesIt();
  drawNewTask();
});
el.newTaskWhat.addEventListener("keydown", (e) => {
  // A word still being composed takes its own Enter.
  if (e.isComposing || e.keyCode === 229) return;
  // Down at the very end, or Enter, to the list: to the best fit when there
  // is one. Down anywhere else moves the caret. Enter never sends from here,
  // so nothing goes to somebody not chosen.
  const atTheEnd =
    el.newTaskWhat.selectionStart === el.newTaskWhat.selectionEnd &&
    el.newTaskWhat.selectionEnd === el.newTaskWhat.value.length;
  if ((e.key === "ArrowDown" && atTheEnd) || (e.key === "Enter" && !e.shiftKey)) {
    e.preventDefault();
    if (e.key === "Enter" && e.repeat) return;
    if (e.key === "Enter" && !el.newTaskWhat.value.trim()) return sayWhatFirst();
    // Aimed at somebody, Enter gives it to them: they are already chosen.
    const aimed = aimedAt();
    if (e.key === "Enter" && aimed) return chooseWhoDoesIt(aimed.kind, aimed.id);
    const rows = newTaskRows();
    pickNewTaskRow(rows.find((r) => r.querySelector(".fit")) || rows[0]);
  }
});
el.newTaskWho.addEventListener("keydown", (e) => {
  const rows = newTaskRows();
  const at = rows.indexOf(document.activeElement);
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    const next = at + (e.key === "ArrowDown" ? 1 : -1);
    if (next < 0) {
      pickNewTaskRow(null);
      el.newTaskWhat.focus();
    } else pickNewTaskRow(rows[Math.min(next, rows.length - 1)]);
  } else if (e.key === "Enter" && !e.repeat && at >= 0) {
    // Not a held key's repeat: the Enter that came to the list is not the
    // one that chooses.
    e.preventDefault();
    chooseWhoDoesIt(rows[at].dataset.kind, rows[at].dataset.id);
  }
});
// Escape puts it away wherever focus is, and only it; Tab goes round inside
// it, between what needs doing and who, never out to what is behind.
document.addEventListener(
  "keydown",
  (e) => {
    if (el.newTask.hidden) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeNewTask({ refocus: true });
    } else if (e.key === "Tab") {
      e.preventDefault();
      const rows = newTaskRows();
      const onAList = rows.includes(document.activeElement);
      if (onAList) {
        pickNewTaskRow(null);
        el.newTaskWhat.focus();
      } else pickNewTaskRow(rows.find((r) => r.getAttribute("aria-selected") === "true") || rows.find((r) => r.querySelector(".fit")) || rows[0]);
    }
  },
  true,
);
// A press beside the box puts it away, the way the palette goes: on the
// press, so a drag that ends out there is not taken for one.
el.newTask.addEventListener("mousedown", (e) => {
  if (e.target === el.newTask) closeNewTask({ refocus: true });
});

/**
 * What was here before.
 *
 * The agents at the start, and a conversation's messages when it is opened --
 * lazily, because somebody with forty agents should not wait for thirty-nine of
 * them.
 */
async function catchUp() {
  await readTheSettings();
  notifyingNow = (await invoke("notifications").catch(() => null))?.state || null;
  await readTasks();
  standingNow = (await invoke("standing").catch(() => [])) || [];
  const known = await invoke("agents");
  for (const a of known) agents.set(a.id, asAgent(a, agents.get(a.id)));
  await whatIsNew();
  drawTrouble();
  drawThreads();
  knockOnTheModels();
  // A version somebody has not been told about yet, said once. After the tour,
  // because a brand new copy has nothing to have changed from.
  if (known.length) await whatChanged(false);
  if (known.length) {
    await openAgent(known[0].id);
    // Now is home: what needs you, what is running and what is next, before
    // any one teammate. The teammate opened under it is where Back goes. Not
    // under the window harness, whose checks start from a conversation.
    if (!window.__ERRAND_UNDER_TEST__) {
      showMission({ focus: false });
      // Nor in the box behind it, where typing would go to a conversation
      // nobody can see.
      el.what.blur();
    }
  } else {
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

/**
 * An agent this window has never heard of.
 *
 * Agents are made outside the window all the time: `errand-app ask` in a
 * terminal, a script, another agent starting a room, the clock. The app tells
 * the window what happens in their conversations the same as any other, but
 * the window only knew the agents it read when it opened, so the first sign
 * of life from a new one was dropped with `if (!t) return` and the agent did
 * not appear down the side until Errand was quit and opened again. Reading
 * the agents again instead, and drawing the side. Null when the app has no
 * such agent either.
 */
async function meetAgent(id) {
  let listed;
  try {
    listed = await invoke("agents");
  } catch {
    return null;
  }
  // In the app's order, which is what a relaunch would show: pinned first,
  // then whoever spoke most recently. The side is drawn in the order agents
  // were met, and merely adding the new one put it at the bottom, where in a
  // window the height of this one it is below the fold and might as well not
  // have appeared. Anything the app no longer lists is kept, at the end, so
  // whatever is on screen stays on screen.
  const known = new Map(agents);
  agents.clear();
  for (const a of listed) agents.set(a.id, asAgent(a, known.get(a.id)));
  for (const [theirId, a] of known) if (!agents.has(theirId)) agents.set(theirId, a);
  // What is new, read again too, and the side drawn from it: the row of an
  // agent that has just answered from a terminal said "Nothing said yet".
  await whatIsNew();
  return agents.get(id) || null;
}

/** The fetches out for conversations this window is meeting, by id. */
const meeting = new Map();

/**
 * A conversation this window has never heard of: whose it is, and every
 * conversation of theirs, so the picker is right as well as the side. One
 * fetch shared by every event that arrives while it is out, because the first
 * line of a new conversation is followed by the rest of it at once. Null when
 * the app has no such conversation either.
 */
function meet(conversation) {
  const known = talks.get(conversation);
  if (known) return Promise.resolve(known);
  if (!meeting.has(conversation)) {
    const fetching = (async () => {
      try {
        const agent = await invoke("conversation_agent", { id: conversation });
        if (!agent) return null;
        if (!agents.has(agent) && !(await meetAgent(agent))) return null;
        for (const c of await invoke("conversations", { agent })) {
          talks.set(c.id, asTalk(c, talks.get(c.id)));
        }
        drawThreads();
        if (agent === showingAgent) drawTalks();
        return talks.get(conversation) || null;
      } catch {
        return null;
      } finally {
        meeting.delete(conversation);
      }
    })();
    meeting.set(conversation, fetching);
  }
  return meeting.get(conversation);
}

/** Show a conversation, fetching what was said in it the first time. */
async function show(id) {
  const t = talks.get(id);
  if (!t) return;
  // What to keep from a finished task belongs to that task, not the next one.
  if (el.taskLearn.dataset.task !== id) el.taskLearn.hidden = true;
  // Half a sentence belongs to the conversation it was being written into. It
  // used to follow whoever switched, which this app encourages constantly: the
  // sidebar row, the conversation picker and "New conversation with this agent"
  // are all one click, and every one of them carried an unsent errand into
  // somebody else's composer where Enter would send it. The only item on this
  // list that could lose work rather than merely fail to show it.
  putItDown(showing);
  // An answer being read aloud stops with its conversation: its Stop button
  // goes with it, and a voice nobody can find the off switch for is worse
  // than one cut short.
  if (showing !== id) stopReadingAloud();
  if (showing !== id) dropTheSuggestion();
  showing = id;
  showingAgent = t.agent;

  if (!t.loaded) {
    // Whether the engine behind this is still there, which the stored lines
    // cannot say. A question with nothing written against it is either one
    // nobody will ever answer or one being waited on this second, and those are
    // the same row on disk. An errand started from outside stops at its first
    // question, and drawing that as expired made it unanswerable while the
    // engine sat there waiting.
    const [lines, live, , members] = await Promise.all([
      invoke("lines", { id }),
      invoke("still_going", { id }).catch(() => false),
      // Before the lines are read, since reading one asks whether it is still
      // being waited on.
      whatIsStillWaiting(),
      // Who is in it, when it is a room. Asked here rather than with every
      // conversation in the list, because it is one question about the one
      // being opened. Nothing, when the app cannot say: a conversation that
      // cannot be told to be a room is an ordinary one.
      invoke("members", { id }).catch(() => []),
    ]);
    t.members = members || [];
    t.messages = lines.map((line) => fromStore(line, live));
    t.loaded = true;
    // Somebody moved on while this was being read. What was read is kept for
    // when they come back; everything below is about the conversation on
    // screen, and that is now another one. Carried on, it put this one's
    // draft into the other's box and marked this one read, unseen.
    if (showing !== id) return;
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
  el.rooming.hidden = true;
  if (a) {
    drawMark(a);
    drawPaused(a);
    el.name.textContent = a.name;
    drawPurpose(a);
    drawWordsGo();
    drawEngines(a);
  }
  drawTalks();
  drawRoom(t);
  // What it is aiming at, for Runs on its card. Asked here rather than with
  // every task, since it is one question about the one on screen.
  invoke("goal_of", { id })
    .then((now) => {
      t.aim = now?.goal && !now.over ? now : null;
      if (showing === id) drawTaskCard();
    })
    .catch(() => {});
  drawThreads();
  t.earlier = 0;
  drawMessages({ follow: true });
  pickItBackUp(id);
  // Which conversation somebody is actually reading, so a notification can be
  // held back for this one and shown for the thirty-nine that are not. The app
  // knows what is running; only the window knows what is being looked at.
  invoke("looking_at", { id }).catch(() => {});
  drawTrouble();
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
  // Its tasks. A finished one stays, marked, for the days chosen in Settings,
  // then lives only in the overview; the one on screen is always there.
  const now = Date.now();
  const theirs = [...talks.values()].filter(
    (t) =>
      t.agent === showingAgent &&
      (t.id === showing || stillInTheList(t, now, FINISHED_KEPT_DAYS)),
  );
  el.talks.replaceChildren(
    ...theirs.map((t) => {
      const option = document.createElement("option");
      option.value = t.id;
      // A clock on the name, so a scheduled task is recognisable without
      // opening the panel that would tell you, and a tick on a finished one.
      option.textContent = `${t.name}${t.repeats ? " ⏱" : ""}${t.finished ? " ✓" : ""}`;
      option.selected = t.id === showing;
      return option;
    }),
  );
  const another = document.createElement("option");
  another.value = "+";
  another.textContent = "New task…";
  el.talks.append(another);
  // Beside it, because a room is a way of starting talking, and this is
  // where somebody goes to start.
  const room = document.createElement("option");
  room.value = "room";
  room.textContent = "New room…";
  el.talks.append(room);
  el.talks.hidden = !a;
  drawTaskDone();
  drawTaskCard();
  drawRunningNote();
}

/**
 * Stop what a conversation is doing.
 *
 * Nothing else will say it stopped. A turn ends in the window when an ending
 * arrives from the engine, and an engine that was killed never sends one, so
 * without this the conversation goes on saying "Running now" and offering to stop
 * something that stopped minutes ago.
 */
async function stopTheRun(id) {
  if (!id) return;
  await invoke("stop", { id });
  itHasStopped(talks.get(id));
  if (showing === id) drawMessages();
  drawThreads();
  drawTalks();
}

/** What this one task is waiting on somebody for: a question, or a handover. */
function theOpenQuestion(t) {
  return t?.messages?.find(
    (m) =>
      !m.answered &&
      (m.kind === "asking" ||
        ((m.kind === "over_to_you" || m.kind === "open_outside") && stillWaiting.has(m.handover))),
  );
}

/** Whether this one task is waiting on somebody: a question, or a handover. */
function waitingHere(t) {
  return !!theOpenQuestion(t);
}

/**
 * What the window knows is going on this minute, the way stateOf reads it:
 * what is working, and what is stopped on a question.
 */
function goingNow() {
  return [...talks.values()]
    .filter((x) => x.working || waitingHere(x))
    .map((x) => ({ conversation: x.id, waiting: !x.working && waitingHere(x), what: "" }));
}

/**
 * Where a task stands, read one way for every place that says it: the chip at
 * its top, its row down the side, the count on Now and its group in Now. The
 * chip had words of its own for a while, which is how one state came to be
 * called three things; and Now read the app's list of what is running, up to
 * ten seconds old, where the side read the window's own, so a task that had
 * just finished was running in one and answered in the other.
 *
 * @param going what goingNow says, read once by a caller reading many tasks
 */
function whereItStands(id, agent, going = goingNow()) {
  const task = tasksNow.find((x) => x.id === id) || {};
  const finished = talks.get(id)?.finished ?? task.finished ?? null;
  return stateOf({ id, finished }, agents.get(agent), going, standingNow);
}

/** Where a task stands, with its chip's words and what of it repeats. */
function taskState(t, going = goingNow()) {
  const task = tasksNow.find((x) => x.id === t.id) || {};
  const is = whereItStands(t.id, t.agent, going);
  const mine = standingNow.filter((s) => s.conversation === t.id);
  return {
    ...is,
    ...chipOf(is, { said: task.said !== false }, shortlyWhen),
    routine: mine.find((s) => s.kind === "routine"),
    watch: mine.find((s) => s.kind === "watch"),
  };
}

/**
 * When something next runs, short enough for a chip: the time today, the day
 * and the hour this week, the date after that.
 */
function shortlyWhen(due) {
  const at = new Date(due);
  const hour = at.getMinutes()
    ? at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
    : at.toLocaleTimeString([], { hour: "numeric" });
  const midnight = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((midnight(at) - midnight(new Date())) / 86_400_000);
  if (days <= 0) return hour;
  if (days < 7) return `${at.toLocaleDateString([], { weekday: "short" })} ${hour}`;
  return at.toLocaleDateString([], { day: "numeric", month: "short" });
}

/** The marks a state is said with, at the size of a chip. */
const STATE_MARKS = {
  running: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3.2v9.6L13 8z" fill="currentColor"/></svg>',
  "needs-you": '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M8 4.6v4.2" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="8" cy="11.3" r="1.1" fill="currentColor"/></svg>',
  scheduled: '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M8 4.8V8l2.4 1.6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  paused: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5.5 3.5v9M10.5 3.5v9" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/></svg>',
  finished: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  stopped: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9" rx="1.6" fill="currentColor"/></svg>',
  idle: '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="4" fill="none" stroke="currentColor" stroke-width="1.8"/></svg>',
};

/** A task's state as a chip. */
function stateChip(state) {
  const chip = document.createElement("span");
  chip.className = "state";
  chip.dataset.kind = state.kind;
  chip.innerHTML = STATE_MARKS[state.kind];
  chip.append(state.says);
  return chip;
}

/**
 * What the task on screen is, at the top of it: where it stands, what it
 * does, when it repeats and what came of it last, with the buttons that act
 * on it. Finding that out meant reading back through the conversation, or
 * opening Repeat, which only knew half of it.
 */
/**
 * A rename going on on the task's card: which task, the box being typed in,
 * and how it ends.
 */
let renaming = null;

function drawTaskCard() {
  const t = talking();
  if (!t || !whose()) {
    el.taskCard.hidden = true;
    // Kept in the card with nothing to say, so it has somewhere to come back to.
    el.taskDone.hidden = true;
    el.taskCard.replaceChildren(el.taskDone);
    return;
  }
  if (renaming) {
    // Mid-rename, the card waits: drawn again, it would take the box away
    // under somebody's typing. What changed meanwhile is drawn when they finish.
    if (renaming.id === t.id && renaming.box.isConnected) return;
    // Left without Enter, Escape or a click away: another task came on screen,
    // or the box went with the one it was in. Kept, the way a click away keeps
    // it; left going, it held this card still for good.
    renaming.finish(true);
    return;
  }
  // The task the conversation below is, first, and every other task of its
  // teammate that is doing something: needing you, running, stopped, or due.
  // Only the one on screen was here, so a second job set up from this very
  // conversation went on running with nothing above the chat to say so.
  const going = goingNow();
  const others = [...talks.values()].filter(
    (x) => x.agent === t.agent && x.id !== t.id && TASKS_SHOWN.has(taskState(x, going).kind),
  );
  const states = new Map(others.map((x) => [x.id, whereItStands(x.id, x.agent, going)]));
  const ordered = inStateOrder(
    others.map((x) => ({ ...x, spoke: tasksNow.find((y) => y.id === x.id)?.spoke || 0 })),
    (x) => states.get(x.id),
  );
  const alone = !ordered.length;
  el.taskCard.replaceChildren(
    taskRow(t, { here: true, going, alone }),
    ...ordered.map((x) => taskRow(talks.get(x.id), { going, alone })),
  );
  el.taskCard.dataset.state = taskState(t, going).kind;
  el.taskCard.dataset.tasks = String(ordered.length + 1);
  el.taskCard.hidden = false;
  fitTheFacts();
}

/**
 * Which of a task's long facts somebody opened with More, by task and fact,
 * so the redraws a running task makes every few seconds do not shut them.
 */
const factsOpen = new Set();

/**
 * More only where there is more: a Does that fits in its two lines has none.
 * Measured, because how much fits depends on the window's width.
 */
function fitTheFacts() {
  for (const dd of el.taskCard.querySelectorAll("dd.long")) {
    const words = dd.querySelector(".words");
    const more = dd.querySelector(".more");
    more.hidden = !dd.classList.contains("open") && words.scrollHeight <= words.clientHeight + 1;
  }
}
window.addEventListener("resize", () => {
  if (!el.taskCard.hidden) fitTheFacts();
});

/** The states a teammate's other tasks are shown above the chat in. */
const TASKS_SHOWN = new Set(["needs-you", "running", "stopped", "scheduled"]);

/**
 * Which tasks are folded to one line, by what somebody chose. A task nobody
 * chose for is open when it is the only one, and one line when there are
 * several, so the conversation keeps its room.
 */
const taskRowsOpen = (() => {
  try {
    return new Map(JSON.parse(localStorage.getItem("errand-task-rows") || "[]"));
  } catch {
    return new Map();
  }
})();

function rowIsOpen(id, alone) {
  return taskRowsOpen.has(id) ? taskRowsOpen.get(id) : alone;
}

function keepRowOpen(id, open) {
  taskRowsOpen.set(id, open);
  try {
    localStorage.setItem("errand-task-rows", JSON.stringify([...taskRowsOpen].slice(-200)));
  } catch {
    // Kept for as long as the window is open.
  }
}

/**
 * One task above the chat: a line that says where it stands, what it is and
 * how it runs, and, opened, what it does, how it runs, who asked and what came
 * of it last. The one the conversation below is has its controls on its line;
 * any other has Open, which brings its conversation up.
 *
 * @param {object} t the task, as the window holds it
 * @param {{here?: boolean, going: object[], alone: boolean}} how
 */
function taskRow(t, { here = false, going, alone }) {
  const state = taskState(t, going);
  const { routine, watch } = state;
  const task = tasksNow.find((x) => x.id === t.id) || { name: t.name, first: "" };
  const called = titleOf({ ...task, id: t.id, name: t.name });
  const open = rowIsOpen(t.id, alone);
  const row = document.createElement("article");
  row.className = here ? "task-row here" : "task-row";
  if (open) row.classList.add("open");
  row.dataset.task = t.id;
  row.dataset.kind = state.kind;

  const head = document.createElement("div");
  head.className = "head";
  const factsId = `task-facts-${t.id}`;
  const fold = document.createElement("button");
  fold.type = "button";
  fold.className = "fold";
  fold.setAttribute("aria-expanded", String(open));
  fold.setAttribute("aria-controls", factsId);
  fold.title = open ? "Fold it to one line" : "Show what it does, how it runs and what came of it last";
  fold.setAttribute("aria-label", `${open ? "Fold" : "Unfold"} ${called}`);
  fold.innerHTML =
    '<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M6 3.5 10.5 8 6 12.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>';
  fold.onclick = () => {
    keepRowOpen(t.id, !open);
    drawTaskCard();
  };

  // Its name. On the task the conversation is, a control: what a task is
  // called is somebody's to change, and the only way to was a right-click and
  // a dialog.
  let name;
  if (here) {
    name = document.createElement("button");
    name.type = "button";
    name.title = `${called}\nClick to rename this task`;
    name.onclick = () => renameTheTask(t);
  } else {
    name = document.createElement("span");
    name.title = called;
  }
  name.className = "name";
  name.textContent = called;

  const runs = routine
    ? `${routine.at}${routine.off ? ", switched off" : routine.paused ? ", its teammate is paused" : routine.due ? `, next ${whenNext(routine.due)}` : ""}`
    : watch
      ? `Watches ${watch.at}${watch.stopped ? ", stopped" : ""}`
      : t.aim
        ? `Toward a goal: ${t.aim.goal}, ${t.aim.tries} of ${t.aim.at_most} turns used`
        : "Only when you ask";
  // How it runs, on its line while it is folded: the line is all there is then.
  const summary = note("span", routine ? routine.at : watch ? `Watches ${watch.at}` : t.aim ? "Toward a goal" : "", "summary");

  const actions = document.createElement("span");
  actions.className = "actions";
  const button = (label, does, title) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = label;
    if (title) b.title = title;
    b.onclick = does;
    return b;
  };
  if (state.kind === "running") {
    actions.append(button("Stop", () => stopTheRun(t.id), "Stop what it is doing now"));
  }
  if (here) {
    // Its own schedule's switch. Not while its teammate is paused: that is the
    // teammate's Pause, in the header, and this switch would only turn the
    // routine off underneath it.
    if (routine && (routine.off || !routine.paused)) {
      actions.append(
        button(
          routine.off ? "Resume" : "Pause",
          async () => {
            await invoke("routine_off", { id: t.id, off: !routine.off });
            t.repeats = routine.off;
            drawTalks();
          },
          routine.off ? "Start its schedule again, counting from now" : "Hold its schedule without losing it",
        ),
      );
    }
    if ((routine || watch) && state.kind !== "running") {
      actions.append(
        button("Run now", () => sayIt((routine || watch).what), "Do it now, without changing when it next runs"),
      );
    }
    // Finished is the task's, so it is on the task: it was in the header, among
    // the teammate's buttons, where it read as finishing the teammate.
    drawTaskDone();
    actions.append(el.taskDone);
  } else {
    actions.append(button("Open", () => show(t.id), "Bring up its conversation"));
  }
  head.append(fold, stateChip(state), name, summary, actions);

  const said = (label, words, full) => {
    const dt = note("dt", label);
    const dd = document.createElement("dd");
    const text = note("span", words, "words");
    text.title = full || words;
    dd.append(text);
    return [dt, dd];
  };
  // What it does and what came of it last can be paragraphs: two lines of
  // each, and More for the rest. They were cut at their first line, with no
  // way to read the rest of a routine's steps anywhere on the card.
  const saidAtLength = (label, words) => {
    const dt = note("dt", label);
    const dd = document.createElement("dd");
    dd.className = "long";
    const key = `${t.id}:${label}`;
    const open = factsOpen.has(key);
    if (open) dd.classList.add("open");
    const more = document.createElement("button");
    more.type = "button";
    more.className = "more";
    more.textContent = open ? "Less" : "More";
    more.setAttribute("aria-expanded", String(open));
    more.title = open ? "Show only the start" : "Show all of it";
    more.onclick = (e) => {
      e.stopPropagation();
      if (open) factsOpen.delete(key);
      else factsOpen.add(key);
      drawTaskCard();
    };
    dd.append(note("span", words, "words"), more);
    return [dt, dd];
  };
  const does = (routine?.what || watch?.what || withoutWhoAsked(task.first || "")).trim() || "Nothing asked yet";
  const facts = document.createElement("dl");
  facts.id = factsId;
  facts.append(...saidAtLength("Does", does));
  // How it runs, and the one way to change that: on a schedule, when
  // something changes, or until a goal is met. Those were Repeat, Watch and
  // Goal in the header, three buttons nothing said were one question.
  const [runsLabel, runsWords] = said("Runs", runs);
  const change = document.createElement("button");
  change.type = "button";
  change.className = "change";
  change.textContent = "Change\u2026";
  change.title = "Change how this task runs: on a schedule, when something changes, or until a goal is met";
  if (here) {
    change.id = "runs-change";
    change.setAttribute("aria-controls", "schedule");
    change.setAttribute("aria-expanded", String(!el.schedule.hidden));
    change.onclick = () => changeHowItRuns(routine, watch, t.aim);
  } else {
    // Changed where its conversation is, which is where the three ways are.
    change.onclick = async () => {
      await show(t.id);
      changeHowItRuns(routine, watch, t.aim);
    };
  }
  runsWords.append(change);
  facts.append(runsLabel, runsWords);
  // Who asked for it, when that is all its stored name said.
  const from = askedBy(t.name);
  if (from) facts.append(...said("From", from));
  // What came of it last, when its lines have been read: a task never opened
  // in this window has none to say it from.
  if (here || t.loaded) {
    const last = [...t.messages].reverse().find((m) => m.kind === "said" || (m.kind === "ended" && m.failed));
    const lastWords = last ? `${stamped(last).textContent}: ${last.text.trim()}` : "Nothing yet";
    facts.append(...saidAtLength("Last", lastWords));
  }
  row.append(head, facts);
  // Answered, with nothing running from it: what to do with it, said. A card
  // that only said "Answered" left somebody asking whether anything still
  // relied on it.
  // Not while it works toward a goal: something does run from that.
  if (state.kind === "idle" && !t.aim) {
    let hint = "Nothing runs from this task, and nothing waits on it. Mark it finished once you have what you needed.";
    if (askedForByOthers({ ...task, name: t.name })) {
      const who = askedBy(t.name);
      const asker = who ? who[0].toUpperCase() + who.slice(1) : "Another teammate";
      hint = `${asker} asked for this and has had its answer. Nothing waits on it: mark it finished whenever you like.`;
    }
    row.append(note("p", hint, "hint"));
  }
  return row;
}


/**
 * Rename the task on screen, on its card, where its name is.
 *
 * Enter or leaving the box keeps it, Escape puts the old name back. It was a
 * right-click on the conversation and a dialog, which nobody found.
 */
function renameTheTask(t) {
  const name = el.taskCard.querySelector(".head .name");
  if (!t || !name || renaming) return;
  const task = tasksNow.find((x) => x.id === t.id) || { name: t.name, first: "" };
  const box = document.createElement("input");
  box.type = "text";
  box.className = "name-edit";
  box.value = titleOf({ ...task, name: t.name });
  box.setAttribute("aria-label", "Name of this task");
  box.autocomplete = "off";
  box.spellcheck = false;
  let done = false;
  const finish = async (keep) => {
    if (done) return;
    done = true;
    renaming = null;
    const called = box.value.trim();
    // An empty name is not a name: what it was called stays.
    if (keep && called && called !== titleOf({ ...task, name: t.name })) {
      const was = t.name;
      t.name = called;
      for (const one of tasksNow) if (one.id === t.id) one.name = called;
      drawTalks();
      drawThreads();
      try {
        await invoke("call_it", { id: t.id, name: called });
      } catch (why) {
        t.name = was;
        for (const one of tasksNow) if (one.id === t.id) one.name = was;
        drawTalks();
        drawThreads();
        complain(String(why));
      }
    } else {
      drawTaskCard();
    }
  };
  box.addEventListener("keydown", (e) => {
    if (e.isComposing) return;
    if (e.key === "Enter") {
      e.preventDefault();
      finish(true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      finish(false);
    }
  });
  box.addEventListener("blur", () => finish(true));
  renaming = { id: t.id, box, finish };
  name.replaceWith(box);
  box.focus();
  box.select();
}

/**
 * Open how the task on screen runs at the one it runs by, or put it away.
 *
 * At whichever it runs by, a schedule before a watch before a goal, and at a
 * schedule when it has none of them: the first answer most people want.
 */
function changeHowItRuns(routine, watch, aim) {
  const open = [el.routine, el.watching, el.aiming].find((panel) => !panel.hidden);
  if (open) {
    ({ routine: el.repeat, watching: el.watch, aiming: el.goal })[open.id].click();
    return;
  }
  (routine ? el.repeat : watch ? el.watch : aim ? el.goal : el.repeat).click();
}

/**
 * The one way a task runs that is open, marked among the three, and the row of
 * three shown only while one is: it is a heading for what is open under it,
 * not a fourth thing on screen. Kept true by watching the panels themselves,
 * since everything from Save to opening another task closes them.
 */
function drawHowItRuns() {
  const open = [el.routine, el.watching, el.aiming].find((panel) => !panel.hidden);
  el.schedule.hidden = !open;
  for (const [tab, panel] of [
    [el.repeat, el.routine],
    [el.watch, el.watching],
    [el.goal, el.aiming],
  ]) {
    tab.setAttribute("aria-selected", String(panel === open));
  }
  document.getElementById("runs-change")?.setAttribute("aria-expanded", String(!!open));
}
const panelsWatched = new MutationObserver(drawHowItRuns);
for (const panel of [el.routine, el.watching, el.aiming]) {
  panelsWatched.observe(panel, { attributes: true, attributeFilter: ["hidden"] });
}

/** One of the three open, and the other two put away: they answer one question. */
function onlyThisOneOpen(panel) {
  for (const other of [el.routine, el.watching, el.aiming]) {
    if (other !== panel) other.hidden = true;
  }
}

/** Said above the box while the task on screen is working, with a way to stop it. */
function drawRunningNote() {
  const t = talking();
  el.runningNote.hidden = !(t && t.working && whose());
}

el.runningStop.addEventListener("click", () => stopTheRun(showing));

// A task made by the app rather than in this window: a second schedule set
// alongside the one a task already had. Met, so it is in the menu and the
// list straight away rather than at the next start.
listen("task_made", async ({ payload }) => {
  await meet(payload.conversation);
  await readTasks();
  await readStanding();
  drawTalks();
});

/**
 * Mark finished, on the task's card, for the task on screen.
 *
 * Saying what pressing it does rather than what it is: the chip beside it
 * already says Finished, and "Finished ✓" on the button read as a second
 * label for the same thing rather than the way to open it again.
 */
function drawTaskDone() {
  const t = talking();
  el.taskDone.hidden = !t || !whose();
  const done = Boolean(t?.finished);
  el.taskDone.textContent = done ? "Reopen" : "Mark finished";
  el.taskDone.dataset.finished = String(done);
  el.taskDone.title = done
    ? "It is not done after all: open it again, and what it ran starts again"
    : "Mark this task done. Whatever it runs on its own stops; it stays in the list for a while, then only in Now";
}

/**
 * Who is in the room, in the header, and a composer that says how to speak to
 * one of them. Nothing and the ordinary words for a conversation that is not
 * a room, which is nearly all of them.
 */
function drawRoom(t) {
  const names = (t?.members || []).map((m) => m.name);
  const aRoom = names.length > 1;
  el.members.hidden = !aRoom;
  el.members.textContent = aRoom ? `Room: ${namedTogether(names)}` : "";
  askedForAnAnswer(t);
}

/**
 * The box, and the two buttons above it, for what the conversation is waiting
 * on: an answer to its last question, or something new to do.
 *
 * Agents are told to end a turn they cannot finish on one question a word
 * answers, and the box under it still said "What would you like done?", which
 * reads as starting something new. A call keeps its own words.
 */
function askedForAnAnswer(t) {
  const question = theQuestionLeftOpen(t);
  const aRoom = (t?.members || []).length > 1;
  const offered = suggested && suggested.for === t?.id && !el.what.value;
  el.tabHint.hidden = !offered;
  if (offered) {
    el.what.placeholder = suggested.text;
  } else if (!document.body.classList.contains("in-a-call")) {
    el.what.placeholder = question
      ? `Answer: ${question.length > 90 ? `${question.slice(0, 89)}…` : question}`
      : aRoom
        ? "Say it to everyone, or start with @Name, or several, to say it to only them"
        : "What would you like done?";
  }
  // Out of the way once somebody is answering in words of their own.
  el.replies.hidden = !question || !yesOrNo(question) || !!el.what.value.trim();
}

/**
 * The next thing to say, greyed in the box, which Tab takes, as in Claude.
 *
 * The box only repeated the teammate's question, so there was nothing to
 * take. Asked of the teammate's own model when a turn ends, for the
 * conversation on screen and only while the box is empty; gone the moment
 * somebody types, sends, or looks at another conversation, or a new turn
 * starts. Nothing at all for a teammate on Claude Code.
 */

async function offerTheNextThing(id) {
  if (inACall || el.what.value.trim()) return;
  let text = null;
  try {
    text = await invoke("suggest_next", { id });
  } catch {
    return;
  }
  if (!text || showing !== id || el.what.value.trim()) return;
  suggested = { for: id, text };
  askedForAnAnswer(talking());
}

function dropTheSuggestion() {
  if (!suggested) return;
  suggested = null;
  askedForAnAnswer(talking());
}

/**
 * The question its last answer left open, if it left one.
 *
 * Found in the last paragraph, because that is where agents are told to put
 * it, and as the last question there: "Have you granted Errand Full Access to
 * Calendars yet? A yes and I'll set the watch" ends on a sentence that is not
 * the question. Nothing while it is still working, or once somebody has said
 * something after it.
 */
function theQuestionLeftOpen(t) {
  if (!t || t.working || t.writing) return null;
  // Past any note the app added after it, such as notifications being off,
  // which the window keeps as an ending that did not fail.
  const last = [...(t.messages || [])].reverse().find((m) => !(m.kind === "ended" && !m.failed));
  if (!last || last.kind !== "said") return null;
  const paragraphs = String(last.text || "").trim().split(/\n\s*\n/);
  const end = (paragraphs[paragraphs.length - 1] || "")
    .replace(/[*_`#>]/g, "")
    .replace(/\s+/g, " ")
    .trim();
  const asked = end.match(/[^.!?]*\?/g);
  return asked ? asked[asked.length - 1].trim() : null;
}

/**
 * Whether a yes or a no answers a question.
 *
 * Only the kind that starts the way those questions do, and never one that
 * offers a choice: "Daily or weekly?" answered Yes is no answer at all.
 */
function yesOrNo(question) {
  if (/\bor\b/i.test(question)) return false;
  return /^(have|has|had|do|does|did|is|are|was|were|am|can|could|shall|should|will|would|may|might|must)\b/i.test(
    question,
  );
}

// Typing an answer of their own puts the two buttons away, and clearing the
// box brings them back.
el.what.addEventListener("input", () => askedForAnAnswer(talking()));

el.replies.addEventListener("click", (e) => {
  const say = e.target.closest("button")?.dataset.say;
  if (!say) return;
  el.what.value = say;
  el.form.requestSubmit();
});

/** Names the way somebody would say them: "A", "A and B", "A, B and C". */
function namedTogether(names) {
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** What an agent is called, by id, or something honest once it has gone. */
function nameOf(agent) {
  return agents.get(agent)?.name || "an agent no longer here";
}

/**
 * A room: one conversation with several agents in it.
 *
 * Ticked rather than typed, because a name is a thing somebody gets slightly
 * wrong and a box is not. Everybody with a name is offered: an agent that has
 * not settled on one is called by its first errand and is not written down
 * until something is said to it, so there is nothing yet to seat.
 */
function offerARoom() {
  const a = whose();
  el.roomingSays.textContent = "";
  el.roomingName.value = "";
  const named = [...agents.values()].filter((one) => one.name !== NOT_YET_NAMED && !one.hidden);
  el.roomingWho.replaceChildren(
    ...named.map((one) => {
      const label = document.createElement("label");
      const box = document.createElement("input");
      box.type = "checkbox";
      box.value = one.id;
      box.checked = one.id === a?.id;
      label.append(box, document.createTextNode(one.name));
      return label;
    }),
  );
  el.rooming.hidden = false;
  // The picker goes back to the conversation that is open: "New room…" is a
  // thing to do, not a place to be.
  el.talks.value = showing;
}

/**
 * The room open on screen, when the panel is changing who is in it rather
 * than making a new one.
 */
let roomBeingChanged = null;

/**
 * Change who is in the room that is open.
 *
 * Members were fixed when a room was made, so a room that needed one more
 * voice, or one fewer, had to be made again from nothing. The same ticks as
 * making one, with the ones in it already ticked.
 */
function changeTheRoom() {
  const t = talking();
  if (!t || (t.members || []).length < 2) return;
  if (!el.rooming.hidden && roomBeingChanged === t.id) {
    el.rooming.hidden = true;
    roomBeingChanged = null;
    return;
  }
  offerARoom();
  roomBeingChanged = t.id;
  const inIt = new Set(t.members.map((m) => m.agent));
  for (const box of el.roomingWho.querySelectorAll("input")) box.checked = inIt.has(box.value);
  // Every member gets a box, whether or not it would be offered for a new
  // room: one hidden from the list, or not yet named, had none, and saving
  // took it out of the room without anybody unticking it.
  const offered = new Set([...el.roomingWho.querySelectorAll("input")].map((box) => box.value));
  for (const m of t.members.filter((m) => !offered.has(m.agent))) {
    const label = document.createElement("label");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.value = m.agent;
    box.checked = true;
    label.append(box, document.createTextNode(m.name));
    el.roomingWho.append(label);
  }
  el.roomingName.closest("label").hidden = true;
  el.roomingStart.textContent = "Change who is in it";
}

el.members.addEventListener("click", changeTheRoom);

/** Start the room that was ticked, and open it; or change the one open. */
async function startTheRoom() {
  if (roomBeingChanged) return changeWhoIsInIt();
  const a = whose();
  const ticked = [...el.roomingWho.querySelectorAll("input:checked")].map((box) => box.value);
  // The open agent first, so the room is filed under it and turns up in the
  // picker somebody is already looking at.
  const chosen = [...new Set([...(a && ticked.includes(a.id) ? [a.id] : []), ...ticked])];
  if (chosen.length < 2) {
    el.roomingSays.textContent = "Pick at least two agents.";
    return;
  }
  const id = uuid();
  let room;
  try {
    room = await invoke("make_room", { id, agents: chosen, name: el.roomingName.value.trim() });
  } catch (why) {
    el.roomingSays.textContent = String(why);
    return;
  }
  talks.set(
    id,
    asTalk({ id, agent: chosen[0], name: room.name }, { loaded: true, members: room.members }),
  );
  el.rooming.hidden = true;
  await show(id);
  el.what.focus();
}

async function changeWhoIsInIt() {
  const t = talks.get(roomBeingChanged);
  if (!t) return;
  const ticked = [...el.roomingWho.querySelectorAll("input:checked")].map((box) => box.value);
  if (ticked.length < 2) {
    el.roomingSays.textContent = "A room needs at least two agents in it.";
    return;
  }
  let room;
  try {
    room = await invoke("set_members", { room: t.id, agents: ticked });
  } catch (why) {
    el.roomingSays.textContent = String(why);
    return;
  }
  t.members = room.members;
  leaveChangingTheRoom();
  drawRoom(t);
}

/** Back to making rooms, the panel's ordinary job. */
function leaveChangingTheRoom() {
  roomBeingChanged = null;
  el.rooming.hidden = true;
  el.roomingName.closest("label").hidden = false;
  el.roomingStart.textContent = "Start the room";
}

el.roomingStart.addEventListener("click", startTheRoom);
el.roomingCancel.addEventListener("click", () => {
  leaveChangingTheRoom();
  el.talks.value = showing;
});

/**
 * What an agent is for, on the line under its name: its role, and what it
 * handles in its own words, which its own model wrote after its first errand.
 * Before that it has nothing to say, and the line says when it will.
 */
function drawPurpose(a) {
  el.purpose.replaceChildren();
  if (!a) {
    el.purpose.hidden = true;
    return;
  }
  if (!a.title && !a.about) {
    el.purpose.textContent =
      a.name === NOT_YET_NAMED ? "It says what it is for once its first errand is done." : "";
    el.purpose.title = "";
    el.purpose.hidden = !el.purpose.textContent;
    return;
  }
  if (a.title) el.purpose.append(note("span", a.title, "role"));
  if (a.about) el.purpose.append(`${a.title ? " \u00b7 " : ""}${a.about}`);
  el.purpose.title = [a.title, a.about].filter(Boolean).join(": ");
  el.purpose.hidden = false;
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
    asks: a.asks || "auto",
    pinned: !!a.pinned,
    hidden: !!a.hidden,
    // Nothing runs on its own while this is set. Read from when it was set,
    // which is what the app keeps.
    paused: !!a.paused_at,
    engine: a.model || "",
    on: a.engine || "claude",
    onSettings: a.engine_settings || null,
    kind: keeping?.kind,
    // When it was last spoken to, for ordering the overview by what is recent.
    spoke: a.spoke_at || 0,
    // Its words stay on this network: it runs only on a model served here.
    keepLocal: !!a.keep_local,
    // A model of its own, a line of the picker, over Errand's. Nothing to
    // follow Errand's.
    ownModel: a.own_model || null,
  };
}

/** One conversation, as the page holds it. */
function asTalk(c, keeping) {
  return {
    id: c.id,
    agent: c.agent,
    name: c.name,
    // Switched off, it is not one the picker should advertise as scheduled.
    // Read without it, a routine paused under Repeat had its clock back after
    // the next relaunch.
    repeats: !!c.runs_at && !c.routine_off,
    // What matters and what is finished are a task's, and a task is this.
    priority: c.priority || 2,
    finished: c.finished_at || null,
    spoke: c.spoke_at || 0,
    messages: keeping?.messages ?? [],
    working: keeping?.working ?? false,
    // Who is in it, when it is a room. Empty for a conversation with one
    // agent, which is nearly all of them.
    members: keeping?.members ?? [],
    // Which member is answering this moment, in a room.
    answering: keeping?.answering ?? "",
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

/**
 * A note as it reads now. One saying notifications are off, written while
 * they were, stayed in the conversation looking like today's news after they
 * were switched on: it says what it was once they are.
 */
function settledNews(line) {
  const stale =
    line.kind === "note" && notifyingNow === "allowed" && line.text.startsWith("Notifications are off for Errand");
  return stale ? "Notifications were off for Errand when this was written. They are on now." : line.text;
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
        // Who said it, in a room. A line on disk carries the id; the name is
        // looked up once here, from the agents already loaded.
        who: line.kind === "said" && line.said_by ? nameOf(line.said_by) : "",
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
      // A pane of System Settings is where most handovers send somebody, and
      // reading one back used to fold its link into the words as text.
      const where = rest.find((one) => /^(https?:\/\/|x-apple\.systempreferences:)/.test(one)) || "";
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
    // A teammate asking to open something outside its wall. Kept as JSON, so a
    // name with a line break in it cannot add a line to the card. Once nobody
    // is waiting on it, it is history: nothing can be opened from it any more.
    case "open_outside": {
      let asked = {};
      try {
        asked = JSON.parse(line.text) || {};
      } catch {
        asked = {};
      }
      const still = stillWaiting.has(line.call);
      return {
        kind: "open_outside",
        seq: line.seq,
        handover: line.call || "",
        path: asked.path || "",
        team: asked.team || "",
        name: asked.name || "",
        what: `open ${asked.name || "something"} outside its wall`,
        opening: asked.kind || "",
        why: asked.why || "",
        at_login: !!asked.at_login,
        answered: still ? null : "It is no longer waiting. Nothing was opened from this card.",
        stillThere: still,
      };
    }
    // A teammate suggesting it keeps something it learned, and what the
    // person said to it, if anything yet.
    case "learning": {
      let card = {};
      try {
        card = JSON.parse(line.text) || {};
      } catch {
        card = {};
      }
      return { kind: "learning", seq: line.seq, card, answered: line.outcome || null };
    }
    // An email left to be checked: its words as they were last kept, and how
    // it ended if it has. "sending" on disk is a send the app did not see
    // finish, which is not a send anybody should be invited to repeat.
    case "draft": {
      let draft = {};
      try {
        draft = JSON.parse(line.text) || {};
      } catch {
        draft = {};
      }
      const [how, when] = String(line.outcome || "").split("|");
      return {
        kind: "draft",
        seq: line.seq,
        to: draft.to || "",
        subject: draft.subject || "",
        body: draft.body || "",
        how: how === "sending" ? "unsure" : how || "",
        when: Number(when) || null,
      };
    }
    case "doing":
      return {
        kind: "doing",
        seq: line.seq,
        text: line.text,
        tool: line.tool,
        call: line.call,
        // Nothing on disk is a step that never answered, which only a turn
        // still going can be in the middle of. An empty answer is a step that
        // finished and printed nothing: a search that found nothing, which is
        // an answer. The two used to read the same, and both spun for ever.
        outcome: line.outcome ?? (live ? null : "stopped"),
      };
    default:
      return {
        kind: "ended",
        failed: line.kind === "ended",
        text: settledNews(line),
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
  el.more.setAttribute("aria-expanded", "false");
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
    closeTheMenu();
    await setPinned(a, !a.pinned);
  });

  item(a.paused ? "Start again" : "Pause", async () => {
    closeTheMenu();
    await setPaused(a, !a.paused);
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

  item("New task for this teammate", async () => {
    closeTheMenu();
    await openNewTask({ aim: { kind: "person", id: a.id } });
  });

  item("Duplicate", async () => {
    closeTheMenu();
    let copy;
    try {
      copy = await invoke("duplicate", { id: a.id });
    } catch (why) {
      complain(String(why));
      return;
    }
    if (await meetAgent(copy)) await openAgent(copy);
  });

  item("Save to a file", async () => {
    closeTheMenu();
    try {
      const onto = await invoke("save_agent", { id: a.id });
      tellHere(`Saved ${a.name} to ${onto}. Drop it on Errand on another Mac to start one like it there.`);
    } catch (why) {
      complain(String(why));
    }
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
    closeTheMenu();
    await setHidden(a, !a.hidden);
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

/** Whether the agents somebody hid are shown, under the row that counts them. */
let showingTheHidden = false;

function drawThreads() {
  // Hidden ones are out of the way, not gone: a search still finds them, and
  // so does the row at the bottom that says how many there are. With only the
  // search, "where did it go" had no answer anybody could see.
  // Teammates, all of them: a teammate is a job that goes on and is never
  // finished. What finishes is a task, in its teammate's task menu.
  const listed = [...agents.values()].filter((a) =>
    narrowedTo ? narrowedTo.has(a.id) : !a.hidden,
  );
  const hiddenOnes = narrowedTo ? [] : [...agents.values()].filter((a) => a.hidden);
  if (!listed.length && !hiddenOnes.length) {
    const none = document.createElement("li");
    none.className = "nothing";
    none.textContent = "Nothing matches that.";
    el.threads.replaceChildren(none);
    return;
  }
  const rowFor = (a) => {
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
      const mark = tile(kindFor(a), busy(a.id), a.hue);
      mark.append(...onItsOwn(a));
      li.append(mark);
      if (a.pinned) li.classList.add("pinned");
      if (a.paused) li.classList.add("paused");

      const words = document.createElement("span");
      words.className = "words";

      const name = document.createElement("span");
      name.className = "name";
      name.textContent = a.name;
      name.append(...repeatMarks(a));
      if (a.title) {
        // The role, so a list of agents can be read at a glance rather than
        // deciphered from names somebody's agents chose for themselves.
        const role = document.createElement("span");
        role.className = "role";
        role.textContent = a.title;
        name.append(role);
      }
      // Framed when a task of its that matters most is still open.
      if (tasksNow.some((t) => t.agent === a.id && t.priority === 1 && !t.finished)) {
        li.dataset.priority = "1";
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
        : // Paused comes before everything else about it. A question it was
          // asking was asked by a turn that pausing stopped, what is new in it
          // can wait, and that nothing more will arrive cannot.
          a.paused
          ? "Paused"
          : waitingOn(a.id)
            ? "Needs you"
            : busy(a.id)
              ? "Running now"
              : // Something happened here and nobody has seen it. This takes
                // the line for as long as that is true, because it is the one
                // thing about an agent somebody cannot work out by looking at
                // the list, and it is the whole reason to leave errands running.
                news
                ? `${news.lines} new · ${howLongAgo(news.at)}`
                : a.about || "Nothing said yet";
      if (hit) last.classList.add("hit");
      if (a.paused && !hit) last.classList.add("paused");
      else if (waitingOn(a.id)) last.classList.add("waiting");
      if (news && !waitingOn(a.id) && !busy(a.id)) last.classList.add("new");

      // Something in one of its tasks nobody has read: a dot at the end of the
      // name's line, in the same place on every row so a list of forty can be
      // skimmed for it, and outside the name so a long one cannot cut it off.
      // Whatever else the row says: needing you is no reason to hide that
      // something new arrived.
      const top = document.createElement("span");
      top.className = "top";
      top.append(name);
      if (news) top.append(unreadDot(news.lines));
      // A file of its home the person changed, waiting to be taken or put
      // back: the same dot, saying something different.
      else if (homeEdited.has(a.id)) {
        const dot = unreadDot(0);
        const said = "You changed a file in its home: open its card to take the edit or put the file back";
        dot.setAttribute("aria-label", said);
        dot.title = said;
        dot.classList.add("home-edited");
        top.append(dot);
      }
      words.append(top, last);
      li.append(words);
      if (news) li.classList.add("has-new");
      if (a.hidden) li.classList.add("is-hidden");
      return li;
  };
  // The teammate on screen with its tasks under it, rather than in a menu in
  // the header. Not while searching: then the list is what was found.
  const withTasks = (a) => {
    const row = rowFor(a);
    if (a.id === showingAgent && !narrowedTo) row.append(tasksUnder(a));
    return row;
  };
  el.threads.replaceChildren(
    ...listed.map(withTasks),
    ...(hiddenOnes.length ? [theHiddenRow(hiddenOnes.length)] : []),
    ...(showingTheHidden ? hiddenOnes.map(withTasks) : []),
  );
  // How many tasks are stopped on a question, on the way into Now, read the
  // way Now reads them.
  const going = goingNow();
  const needing = [...talks.values()].filter(
    (t) => agents.has(t.agent) && whereItStands(t.id, t.agent, going).state === "waiting",
  ).length;
  for (const count of [el.nowCount, el.missionTasksCount]) {
    count.textContent = String(needing);
    count.hidden = !needing;
  }
  el.missionOpen.title = needing
    ? `Mission Control: ${needing} ${needing === 1 ? "task needs" : "tasks need"} you, and every task and every team`
    : "Mission Control: every task and every team";
  // Now is read the same way, so it moves when this does rather than on its
  // next tick: up to ten seconds of a finished task still under Running now.
  if (!el.overview.hidden && statesAt(going) !== nowDrawnAt) drawOverview();
}

/** Every task's state in one line, to tell whether Now is out of date. */
function statesAt(going) {
  return tasksNow.map((t) => `${t.id}:${whereItStands(t.id, t.agent, going).state}`).join(" ");
}

/** The states Now was last drawn from. */
let nowDrawnAt = "";

/**
 * The tasks of the teammate on screen, under its row down the side: each with
 * where it stands and when it next runs, the one on screen marked, and New task
 * and New room after them. They were a menu in the header, with New task at
 * its bottom, and what a teammate had going could not be seen without opening
 * that menu and reading every name in it.
 */
function tasksUnder(a) {
  const theirs = [...talks.values()].filter((t) => t.agent === a.id);
  const going = goingNow();
  const states = new Map(theirs.map((t) => [t.id, taskState(t, going)]));
  const ordered = inStateOrder(
    theirs
      .filter((t) => states.get(t.id).kind !== "finished")
      .map((t) => ({ ...t, spoke: tasksNow.find((x) => x.id === t.id)?.spoke || 0 })),
    (t) => states.get(t.id),
  );
  // Finished ones apart, newest first, folded away until somebody asks for
  // them: they were the end of the list for a week and then gone from it.
  const done = theirs
    .filter((t) => states.get(t.id).kind === "finished")
    .sort((x, y) => (y.finished || 0) - (x.finished || 0));
  // Inside the teammate's own row rather than rows of their own, so the list
  // down the side is still one row per teammate to everything that counts it.
  const list = document.createElement("div");
  list.className = "tasks-of";
  list.setAttribute("role", "list");
  list.setAttribute("aria-label", `Tasks of ${a.name}`);
  for (const t of ordered) list.append(aTaskRow(t, states.get(t.id)));
  if (done.length) list.append(theFinished(a, done, states));
  const more = document.createElement("div");
  more.className = "task-new";
  const add = document.createElement("button");
  add.type = "button";
  add.className = "add";
  add.textContent = "+ New task";
  add.title = "Start another task for this teammate, beside the ones it has";
  add.onclick = (e) => {
    e.stopPropagation();
    alsoAsk();
  };
  const room = document.createElement("button");
  room.type = "button";
  room.className = "room";
  room.textContent = "New room";
  room.title = "Several teammates on one problem, taking turns";
  room.onclick = (e) => {
    e.stopPropagation();
    offerARoom();
  };
  more.append(add, room);
  list.append(more);
  return list;
}

/** Which teammates' finished tasks are unfolded, by teammate. Folded to start. */
const finishedOpen = new Set();

/**
 * A teammate's finished tasks, behind one row that says how many: unfolded
 * only when somebody presses it, and each one there can be opened again or
 * deleted for good.
 */
function theFinished(a, done, states) {
  const open = finishedOpen.has(a.id);
  const group = document.createElement("div");
  group.className = "task-finished";
  group.setAttribute("role", "listitem");
  const head = document.createElement("button");
  head.type = "button";
  head.className = "finished-head";
  head.setAttribute("aria-expanded", String(open));
  head.title = open ? "Fold the finished tasks away" : "Show the finished tasks, to open one again or delete it";
  // The one on screen is in here: said on the row while it is folded.
  if (!open && done.some((t) => t.id === showing)) head.dataset.holdsCurrent = "true";
  head.innerHTML =
    '<svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true"><path d="M6 3.5 10.5 8 6 12.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>';
  head.append(note("span", "Finished", "label"), note("span", String(done.length), "count"));
  head.onclick = (e) => {
    e.stopPropagation();
    if (open) finishedOpen.delete(a.id);
    else finishedOpen.add(a.id);
    drawThreads();
  };
  group.append(head);
  if (open) {
    const rows = document.createElement("div");
    rows.className = "finished-rows";
    rows.setAttribute("role", "list");
    rows.setAttribute("aria-label", `Finished tasks of ${a.name}`);
    for (const t of done) {
      const row = aTaskRow(t, states.get(t.id));
      // Deleted for good, asked once on the button itself, the way a teammate
      // or a conversation is deleted: the second press answers the question
      // the first one asked.
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "forget";
      remove.dataset.sure = "false";
      remove.title = "Delete this task and everything said in it";
      remove.setAttribute("aria-label", `Delete ${inAFewWords(row.querySelector(".task-name").textContent)}`);
      remove.textContent = "\u00d7";
      remove.onclick = async (e) => {
        e.stopPropagation();
        if (remove.dataset.sure !== "true") {
          remove.dataset.sure = "true";
          remove.textContent = "Delete?";
          remove.title = "Press again to delete it and everything said in it";
          return;
        }
        await forgetTheTask(t);
      };
      row.append(remove);
      rows.append(row);
    }
    group.append(rows);
  }
  return group;
}

/**
 * Delete a task and everything said in it, and leave the window on another of
 * its teammate's, or on the teammate.
 */
async function forgetTheTask(t) {
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
  tasksNow = tasksNow.filter((x) => x.id !== t.id);
  if (showing === t.id) {
    const left = [...talks.values()].find((other) => other.agent === agent);
    if (left) await show(left.id);
    else await openAgent(agent);
  }
  drawTalks();
  drawThreads();
}

/** One task down the side: where it stands, its name, when it next runs. */
function aTaskRow(t, state) {
  const task = tasksNow.find((x) => x.id === t.id) || { first: "" };
  const name = titleOf({ ...task, name: t.name });
  const row = document.createElement("div");
  row.setAttribute("role", "listitem");
  row.className = "task";
  row.dataset.task = t.id;
  row.dataset.kind = state.kind;
  row.setAttribute("aria-current", String(t.id === showing));
  const go = document.createElement("button");
  go.type = "button";
  go.title = `${name}: ${state.says}`;
  const mark = document.createElement("span");
  mark.className = "task-mark";
  mark.innerHTML = STATE_MARKS[state.kind];
  // When it next runs, and a dot while something in it has not been read.
  const end = document.createElement("span");
  end.className = "task-end";
  end.append(note("span", sideways(state), "task-when"));
  const news = freshTasks.get(t.id);
  if (news) {
    end.append(unreadDot(news.lines));
    row.dataset.unread = "true";
    go.title += `\n${news.lines} new, not read yet`;
  }
  go.append(mark, note("span", name, "task-name"), end);
  go.onclick = (e) => {
    e.stopPropagation();
    show(t.id);
  };
  row.append(go);
  return row;
}

/**
 * What a task's row down the side says beside its name: when it next runs,
 * and that is all. Everything else is its mark and the colour of its name,
 * with the words on hover and on the card at the top of the task: a sidebar
 * is narrow, and "Needs you" beside every name cut each one to a word.
 */
function sideways(state) {
  if (state.kind === "scheduled") return state.next ? shortlyWhen(state.next.due) : "";
  return "";
}

/** The row at the bottom of the list that says how many are hidden, and shows them. */
function theHiddenRow(count) {
  const li = document.createElement("li");
  li.className = "the-hidden";
  const show = document.createElement("button");
  show.type = "button";
  show.textContent = showingTheHidden ? `Hidden (${count}), put away` : `Hidden (${count})`;
  show.title = "Agents you hid: out of the list, and still running whatever they run";
  show.setAttribute("aria-expanded", String(showingTheHidden));
  show.onclick = (e) => {
    e.stopPropagation();
    showingTheHidden = !showingTheHidden;
    drawThreads();
  };
  li.append(show);
  return li;
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
/**
 * When a line was said, for under it.
 *
 * Without one there was no telling when anything happened: Bell Ahead's
 * calendar answers read as one undated column, so which of them had failed,
 * and when, could only be worked out from the store. Today's lines say the
 * time alone. Anything older says its day as well, because the divider
 * between days is only drawn where the day changes and never above the first
 * line drawn, so a stamp has to make sense read on its own. The whole date is
 * on hover.
 *
 * A line that has only just arrived has not been written down yet and has no
 * time of its own, so it is stamped with the moment it is first drawn, which
 * is the moment it arrived. Kept beside `at` rather than in it: the dividers
 * between days read `at`, and treat a line without one as today's on purpose.
 */
function stamped(m) {
  if (!m.at && !m.arrived) m.arrived = Date.now();
  const ms = m.at || m.arrived;
  const at = new Date(ms);
  const stamp = document.createElement("time");
  stamp.className = "at";
  stamp.dateTime = at.toISOString();
  stamp.title = at.toLocaleString(undefined, { dateStyle: "full", timeStyle: "medium" });
  const time = at.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  const day = whichDay(ms);
  if (day === "Today") stamp.textContent = time;
  else if (day === "Yesterday") stamp.textContent = `Yesterday, ${time}`;
  else {
    const sameYear = at.getFullYear() === new Date().getFullYear();
    const date = at.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: sameYear ? undefined : "numeric",
    });
    stamp.textContent = `${date}, ${time}`;
  }
  return stamp;
}

function theDayChanged(day) {
  const li = document.createElement("li");
  li.className = "day";
  const said = document.createElement("span");
  said.textContent = day;
  li.append(said);
  return li;
}

/**
 * How many lines are drawn at a time, from the newest back.
 *
 * Every line was drawn on every redraw, answers put through markdown again
 * each time, and a conversation 4,775 lines long froze the window for minutes
 * per answer. The newest are what anybody is reading; the rest are a press
 * away, and a search that lands further back draws back to it.
 */
const LINES_AT_A_TIME = 200;

/** Where the drawn lines start in this conversation. */
function drawnFrom(t) {
  return Math.max(0, t.messages.length - LINES_AT_A_TIME * (1 + (t.earlier || 0)));
}

/** Whether somebody is reading at the bottom, rather than further up. */
function atTheBottom() {
  const box = el.messages;
  return box.scrollHeight - box.scrollTop - box.clientHeight < 80;
}

/**
 * Draw the conversation on screen.
 *
 * `follow` is for the moments the bottom is where somebody is going: the
 * conversation just opened, or they have just said something. Otherwise the
 * view follows only somebody already at the bottom. It used to jump there on
 * every redraw, so a line being read further up was taken away by the next
 * step of an errand running below it.
 */
function drawMessages({ follow = false } = {}) {
  const t = talking();
  if (!t) return;
  const box = el.messages;
  const following = follow || atTheBottom();
  const keptTop = box.scrollTop;
  const from = drawnFrom(t);
  const drawn = [];
  if (from > 0) drawn.push(earlierLines(t, from));
  let day = null;
  for (let i = from; i < t.messages.length; i++) {
    const m = t.messages[i];
    const node = draw(m);
    if (!node) continue;
    // Found by what it is rather than by counting rows, which a day's
    // separator or a line not drawn throws out by one.
    if (typeof m.seq === "number") node.dataset.seq = String(m.seq);
    // A line with no time is one that arrived this second and has not been
    // written down yet, which is today by definition and needs no announcing.
    if (m.at) {
      const its = whichDay(m.at);
      if (day !== null && its !== day) drawn.push(theDayChanged(its));
      day = its;
    }
    drawn.push(node);
  }
  box.replaceChildren(...drawn);
  drawTheTail(t);
  box.scrollTop = following ? box.scrollHeight : keptTop;
  askedForAnAnswer(t);
  // What the task is and whether it is running go with what it says: sending
  // something sets it working, and only this is redrawn when it does.
  drawTaskCard();
  drawRunningNote();
}

/**
 * What is being written this second, under everything already said. Dots while
 * there are no words yet, because dots say "working" and an empty box says
 * nothing.
 *
 * On its own, because words arrive many times a second and only this changes
 * with each of them. Redrawing the whole conversation for every one was most
 * of what froze a long one.
 */
function drawTheTail(t) {
  for (const old of el.messages.querySelectorAll(":scope > .writing, :scope > .thinking, :scope > .answering")) {
    old.remove();
  }
  if (t.writing) {
    const writing = document.createElement("li");
    writing.className = "said writing";
    writing.textContent = t.writing;
    el.messages.append(writing);
  } else if (t.working) {
    el.messages.append(thinking());
    // Which member, in a room: three agents are waiting to speak and dots
    // alone do not say whose turn it is.
    if (t.answering) el.messages.append(note("li", `${t.answering} is answering`, "answering"));
  }
}

/** The way to the lines before the ones drawn, keeping the same ones in view. */
function earlierLines(t, from) {
  const li = document.createElement("li");
  li.className = "earlier";
  const more = document.createElement("button");
  more.type = "button";
  more.textContent = `Show earlier lines (${from} more)`;
  more.onclick = () => {
    const box = el.messages;
    const fromTheBottom = box.scrollHeight - box.scrollTop;
    t.earlier = (t.earlier || 0) + 1;
    drawMessages();
    box.scrollTop = box.scrollHeight - fromTheBottom;
  };
  li.append(more);
  return li;
}

function draw(m) {
  const node = document.createElement("li");
  switch (m.kind) {
    case "said":
      node.className = "said";
      // Which member said it, in a room. Nowhere else: the header already
      // names the one agent every other answer is from.
      if (m.who) node.append(note("span", m.who, "who"));
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
      const going = m.outcome == null;
      node.className = going ? "doing running" : "doing";
      node.append(tile(forTool(m.tool), going));
      const what = document.createElement("span");
      what.className = "what";
      what.textContent = m.text;
      node.append(what);
      if (!going) {
        const out = document.createElement("span");
        out.className = "outcome";
        out.textContent = m.outcome || "no output";
        node.append(out);
      }
      return node;
    }
    case "asking":
      return asks(m);
    case "over_to_you":
      return handItOver(m);
    case "open_outside":
      return openItOutside(m);
    case "learning":
      return aSuggestion(m);
    case "draft":
      return aDraft(m);
    case "ended": {
      node.className = m.failed ? "ended failed" : "ended";
      node.append(note("span", m.text, "why"), stamped(m));
      // A turn cut off by the app closing is the one ending worth offering to
      // do again: nothing went wrong with it, it was simply never finished.
      // And there is no answer to hang the ordinary "Ask again" on, because
      // never getting one is the whole of what happened.
      if (m.cutOff) {
        // The request above this ending, not the newest in the conversation.
        // The newest could be anything asked since, "delete the drafts
        // folder" included, and that is what this button used to send.
        const t = talking();
        const at = t ? t.messages.indexOf(m) : -1;
        const asked = at > 0 && t.messages.slice(0, at).reverse().find((x) => x.kind === "mine");
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
 * An email an agent left to be checked before it goes.
 *
 * Nothing about it has gone anywhere. The addresses, the subject and the text
 * can all be changed here, and it is sent only when somebody presses Send,
 * through their own Mail. Afterwards it stays, as what was sent or as a draft
 * that was not: something that went out on somebody's behalf is worth being
 * able to look back at.
 */
function aDraft(m) {
  // Whose it is, taken now: the card only ever belongs to the conversation it
  // was drawn in, whatever is on screen by the time a button is pressed.
  const conversation = showing;
  const card = document.createElement("li");
  card.className = m.how ? "draft done" : "draft";
  card.dataset.seq = String(m.seq);
  card.append(tile("mail", Boolean(m.sending)));

  const words = document.createElement("div");
  words.className = "question";
  const at = m.when ? ` at ${new Date(m.when).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` : "";
  const headline = {
    sent: `Sent${at}`,
    kept: `Kept in Mail's Drafts${at}, not sent`,
    discarded: "Discarded, not sent",
    unsure: "Mail did not say whether it went. Look in Sent before sending it again.",
  }[m.how] || "An email to check before it goes";
  words.append(note("p", headline, "wants"));

  if (m.how) {
    // What went, or what did not: history now, not a form.
    words.append(note("p", `To ${m.to}`, "sent-to"));
    words.append(note("p", m.subject, "sent-subject"));
    words.append(note("p", m.body, "sent-body"));
    card.append(words);
    return card;
  }

  // Changes are kept on the message as they are typed, so that the card being
  // drawn again for any other reason does not take them back.
  const field = (label, name, multiline) => {
    const row = document.createElement("label");
    row.className = multiline ? "field whole" : "field";
    const input = document.createElement(multiline ? "textarea" : "input");
    input.name = name;
    input.value = m[name];
    input.spellcheck = multiline;
    input.oninput = () => {
      m[name] = input.value;
    };
    if (label) row.append(note("span", label, "label"));
    row.append(input);
    return row;
  };
  words.append(field("To", "to", false), field("Subject", "subject", false), field("", "body", true));
  if (m.problem) words.append(note("p", m.problem, "problem"));

  const choices = document.createElement("div");
  choices.className = "choices";
  const send = document.createElement("button");
  send.type = "button";
  send.className = "send-it";
  send.textContent = m.sending ? "Sending…" : "Send email";
  send.disabled = Boolean(m.sending);
  send.onclick = async () => {
    m.sending = true;
    m.problem = "";
    drawMessages();
    try {
      await invoke("send_draft", {
        conversation,
        seq: m.seq,
        to: m.to,
        subject: m.subject,
        body: m.body,
      });
      m.how = "sent";
      m.when = Date.now();
    } catch (why) {
      m.problem = String(why);
      // Gone already, from another window or a second press: say so, and
      // stop offering to send it.
      if (/already been sent or discarded/.test(m.problem)) m.how = "discarded";
      if (/did not answer within/.test(m.problem)) m.how = "unsure";
    }
    m.sending = false;
    drawMessages();
  };
  // Into Mail's own Drafts rather than out: to finish on the phone, or send
  // later from Mail, with nothing sent from here.
  const keep = document.createElement("button");
  keep.type = "button";
  keep.textContent = "Keep in Mail";
  keep.title = "Save it in Mail's Drafts, unsent";
  keep.disabled = Boolean(m.sending);
  keep.onclick = async () => {
    m.sending = true;
    m.problem = "";
    drawMessages();
    try {
      await invoke("draft_to_mail", {
        conversation,
        seq: m.seq,
        to: m.to,
        subject: m.subject,
        body: m.body,
      });
      m.how = "kept";
      m.when = Date.now();
    } catch (why) {
      m.problem = String(why);
      if (/already been sent or discarded/.test(m.problem)) m.how = "discarded";
    }
    m.sending = false;
    drawMessages();
  };
  const discard = document.createElement("button");
  discard.type = "button";
  discard.textContent = "Discard";
  discard.disabled = Boolean(m.sending);
  discard.onclick = async () => {
    try {
      await invoke("discard_draft", { conversation, seq: m.seq });
      m.how = "discarded";
      m.when = Date.now();
    } catch (why) {
      m.problem = String(why);
    }
    drawMessages();
  };
  choices.append(send, keep, discard);
  words.append(choices);
  card.append(words);
  return card;
}

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
      // Said on the link when it will not open. It used to be dropped, so a
      // link that did nothing looked exactly like one that had.
      invoke("show_in_browser", { url: m.where }).catch((why) => {
        link.title = String(why);
        link.classList.add("refused");
      });
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
        } catch {
          // It stopped waiting between the card being drawn and the press, so
          // the answer goes into the conversation instead of nowhere.
          m.stillThere = false;
          sayIt(carryOn);
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
 * A teammate asking to open something outside its wall.
 *
 * The app's own facts first, then the teammate's reason in its own words, and
 * only two answers. There is no Always: what was agreed to once is never agreed
 * to again by itself. And no falling back to saying it into the conversation,
 * as a handover does: "I opened it" said to a teammate that has stopped waiting
 * would be a teammate told something happened that did not.
 */
/**
 * A teammate suggesting it keeps something it learned: a point for how it
 * checks its work, or the task just done as a skill. Nothing is kept until
 * the person says so here, and the teammate does not wait for them.
 */
function aSuggestion(m) {
  const card = document.createElement("li");
  card.className = m.answered ? "handover learning done" : "handover learning";
  card.dataset.seq = String(m.seq);
  const t = talks.get(showing);
  const who = agents.get(t?.agent)?.name || "This teammate";
  const c = m.card || {};
  const words = document.createElement("div");
  words.className = "question";
  const aSkill = c.kind === "skill";
  const steps = Array.isArray(c.steps) ? c.steps.length : 0;
  words.append(
    note(
      "p",
      aSkill
        ? `${who} suggests keeping this task as a skill${c.replaces ? ", in place of the one it has by that name" : ""}`
        : `${who} suggests a point for how it checks its work`,
      "wants",
    ),
  );
  const what = note("p", "", "what");
  if (aSkill) {
    what.append(note("q", c.name || "a skill"), ` \u00b7 ${steps} ${steps === 1 ? "step" : "steps"}`);
  } else {
    what.append(note("q", c.point || ""));
  }
  words.append(what);
  if (c.why) {
    const why = note("p", "", "why");
    why.append(note("span", "Because: ", "label"), note("span", c.why));
    words.append(why);
  }
  if (m.answered) {
    words.append(note("p", m.answered, "answered"));
  } else {
    const choices = document.createElement("div");
    choices.className = "choices";
    const press = (label, keep, primary = false) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = label;
      if (primary) b.className = "yes";
      b.onclick = async () => {
        for (const one of choices.querySelectorAll("button")) one.disabled = true;
        try {
          m.answered = await invoke("take_learning", { conversation: t?.id || showing, seq: m.seq, keep });
        } catch (why) {
          m.answered = `Not kept: ${why}`;
        }
        drawMessages();
        if (keep && !el.whois.hidden) drawChecklist();
      };
      return b;
    };
    choices.append(press(aSkill ? "Keep it" : "Add it", true, true), press("Not now", false));
    words.append(choices);
  }
  card.append(tile("spark", false), words);
  return card;
}

function openItOutside(m) {
  const card = document.createElement("li");
  card.className = m.answered ? "handover open-outside done" : "handover open-outside";
  card.dataset.handover = m.handover || "";
  const who = agents.get(talks.get(showing)?.agent)?.name || "This teammate";
  const words = document.createElement("div");
  words.className = "question";
  words.append(note("p", `${who} asks you to open ${m.name || "something"} outside its wall`, "wants"));
  const facts = document.createElement("ul");
  facts.className = "facts";
  const fact = (text, how = "") => facts.append(note("li", text, how));
  // Whose folder it is in, because that is who could have changed it: a
  // team's folder is everybody's on the team, not the teammate's alone.
  const kindOf = m.opening ? m.opening[0].toUpperCase() + m.opening.slice(1) : "Something";
  fact(m.team ? `${kindOf} in ${m.team}'s shared folder, which everyone on that team can change:` : `${kindOf} in its own folder:`);
  facts.lastChild.append(" ", note("code", m.path || m.name, "path"));
  if (m.opening === "an app") {
    fact(
      m.team
        ? `It runs as you, outside the wall, and can do anything you can, including things anyone on ${m.team} sets up for it later.`
        : "It runs as you, outside the wall, and can do anything you can, including things the teammate sets up for it later.",
      "warning",
    );
  } else if (m.opening === "a folder") {
    fact("It is shown in Finder. Nothing runs.");
  } else {
    fact("It opens in the app it belongs to, from a copy taken when it asked.");
  }
  if (m.at_login) fact("It also asks to start every time you log in.", "warning");
  words.append(facts);
  if (m.why) {
    const why = note("p", "", "why");
    why.append(note("span", "It says: ", "label"), note("q", m.why));
    words.append(why);
  }
  if (m.answered) {
    words.append(note("p", m.answered, "answered"));
  } else {
    const choices = document.createElement("div");
    choices.className = "choices";
    const press = (label, how, said, primary = false) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = label;
      if (primary) b.className = "yes";
      b.onclick = async () => {
        try {
          await invoke("handed_back", { handover: m.handover, how });
          m.answered = said;
        } catch {
          m.answered = "It stopped waiting before you answered. Nothing was opened.";
        }
        m.stillThere = false;
        stillWaiting.delete(m.handover);
        drawMessages();
        drawThreads();
      };
      return b;
    };
    if (m.at_login) {
      choices.append(
        press("Open it and start it at login", "open_at_login", "You opened it and set it to start at every login", true),
        press("Just open it", "open", "You opened it, not at login"),
      );
    } else if (m.opening === "a folder") {
      choices.append(press("Show it in Finder", "open", "You had it shown in Finder", true));
    } else {
      choices.append(press("Open it", "open", "You opened it", true));
    }
    choices.append(press("Don't open it", "skip", "You chose not to open it"));
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
    // Never across two tools: "Page Smith: ..." to ask and to hand_out begin
    // alike, and are two different things to have said yes to.
    if (one.tool !== m.tool) return false;
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
      // Read back from the store, a question knows only its step, and that is
      // enough: both engines find the question by it. Sent as nothing, the
      // app could not take the answer at all.
      call: m.call || m.step,
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
  if (talks.has(id)) {
    // Out from under Mission Control, which the window may have opened on.
    if (!el.mission.hidden) closeMission();
    await show(id);
  }
});

listen("settled", async ({ payload }) => {
  const [id, on] = payload;
  const t = agents.get(id) || (await meetAgent(id));
  if (!t) return;
  t.name = on.name;
  t.title = on.title;
  t.about = on.about;
  t.mark = on.mark;
  t.hue = on.hue;
  if (id === showingAgent) {
    el.name.textContent = t.name;
    drawPurpose(t);
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
listen("noted", async ({ payload }) => {
  // A request can be the first line of a conversation this window has not
  // met: an agent asked from the terminal gets one of its own.
  const t =
    talks.get(payload.conversation) ||
    (payload.kind === "mine" ? await meet(payload.conversation) : null);
  if (!t) return;
  if (payload.kind === "mine") {
    // This window's own, back with where it landed. It is on screen already.
    const own = t.messages.find(
      (m) => m.kind === "mine" && (m.seq === payload.seq || (m.seq == null && m.text === payload.text)),
    );
    if (own) {
      own.seq ??= payload.seq;
      return;
    }
    // Anybody else's starts a turn the window did not start: the clock, a
    // watch, a goal, another agent or the terminal. Shown as working, so it
    // can be seen and stopped like any other.
    t.working = true;
  }
  // Through the same reader a reload goes through, so a line that arrives live
  // and the same line read back tomorrow are the same thing. Pushed straight in
  // as its own kind, it drew as nothing at all live and drew fine after a
  // reload, which is the worst way round.
  t.messages.push(fromStore(payload));
  if (showing === payload.conversation) drawMessages();
  drawThreads();
  // A line written by the app, not typed, is something to read.
  if (payload.kind !== "mine") {
    if (payload.conversation === showing && lookedAt()) nowSeen(showing);
    else whatIsNew();
  }
});

// Somebody is wanted at the keyboard. Its own listener rather than a kind
// inside `happened`, because it is the app asking rather than an engine
// saying: nothing about it came from the conversation's own stream.
// A handover answered by typing rather than by pressing its button. The app
// hands the words to the agent that was waiting; here the card closes, so the
// question does not go on looking open above the answer somebody just gave.
listen("handed_back", ({ payload }) => {
  const t = talks.get(payload.conversation);
  if (!t) return;
  stillWaiting.delete(payload.handover);
  const m = t.messages.find((one) => one.handover === payload.handover);
  if (m && m.answered == null) {
    // Words typed in place of a button: for a request to open something, they
    // are never a yes.
    m.answered = m.kind === "open_outside" ? "It was answered in words. Nothing was opened." : payload.how;
  }
  if (showing === payload.conversation) drawMessages();
});

// A handover that stopped waiting on its own, or whose conversation was
// stopped. Its card keeps its buttons, and they now say the answer into the
// conversation: pressing one used to tell a call that had already gone.
listen("handover_ended", ({ payload }) => {
  stillWaiting.delete(payload.handover);
  const t = talks.get(payload.conversation);
  if (!t) return;
  const m = t.messages.find((one) => one.handover === payload.handover);
  if (m) m.stillThere = false;
  if (m?.kind === "open_outside" && !m.answered) m.answered = "It stopped waiting. Nothing was opened.";
  if (showing === payload.conversation) drawMessages();
  drawThreads();
});

// An agent paused or started again from somewhere other than this window: by
// asking it to, or from a terminal.
listen("paused", ({ payload }) => {
  const a = agents.get(payload.agent);
  if (!a || a.paused === payload.paused) return;
  a.paused = payload.paused;
  if (a.id === showingAgent) drawPaused(a);
  drawThreads();
});

// A schedule set or switched off by the agent itself, because it was asked to.
listen("repeats", async ({ payload }) => {
  readStanding();
  const t = talks.get(payload.conversation);
  if (!t) return;
  t.repeats = payload.repeats;
  drawTalks();
  // Repeat, if it is open on this conversation, says what is now true. Only the
  // sentence and the switch: the boxes may be half typed in.
  if (el.routine.hidden || talking() !== t) return;
  const mine = (await invoke("routines")).find((r) => r.conversation === t.id);
  theRoutineShown = mine || null;
  el.routineSays.textContent = sayWhen(mine);
  el.routinePause.textContent = mine?.off ? "Start again" : "Pause";
  el.routinePause.hidden = !mine;
});

// A suggestion arrives mid-turn and the teammate carries on, so the task is
// left working: the card waits without stopping anything.
listen("learning_suggested", async ({ payload }) => {
  const t = talks.get(payload.conversation) || (await meet(payload.conversation));
  if (!t) return;
  t.messages.push({ kind: "learning", seq: payload.seq, card: payload.card || {}, answered: null });
  if (showing === payload.conversation) drawMessages();
  drawThreads();
});

listen("asking_to_open", async ({ payload }) => {
  const t = talks.get(payload.conversation) || (await meet(payload.conversation));
  if (!t) return;
  stillWaiting.add(payload.handover);
  t.working = false;
  t.messages.push({
    kind: "open_outside",
    seq: payload.seq,
    handover: payload.handover,
    path: payload.path,
    team: payload.team || "",
    name: payload.name,
    what: `open ${payload.name} outside its wall`,
    opening: payload.kind,
    why: payload.why,
    at_login: !!payload.at_login,
    answered: null,
  });
  if (showing === payload.conversation) drawMessages();
  drawThreads();
});

listen("handing_over", async ({ payload }) => {
  const t = talks.get(payload.conversation) || (await meet(payload.conversation));
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

// An email an agent left to be checked, the moment it is written down.
listen("drafted", async ({ payload }) => {
  const t = talks.get(payload.conversation) || (await meet(payload.conversation));
  if (!t) return;
  t.messages.push({
    kind: "draft",
    seq: payload.seq,
    to: payload.draft.to,
    subject: payload.draft.subject,
    body: payload.draft.body,
    how: "",
    when: null,
  });
  if (showing === payload.conversation) drawMessages();
  drawThreads();
});

// A room's turn, which no engine reports: who is answering this moment, and
// when the round is over. A room has no engine, so none of the seven events
// ever arrive for it, and without this the dots stayed under a room for ever.
listen("room_turn", ({ payload }) => {
  const t = talks.get(payload.conversation);
  if (!t) return;
  t.working = !payload.over;
  t.answering = payload.over ? "" : payload.who || "";
  if (showing === payload.conversation) drawMessages();
  drawThreads();
});

listen("happened", async ({ payload }) => {
  const t = talks.get(payload.conversation) || (await meet(payload.conversation));
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
      // Words mean a turn is going, whoever started it. A window reopened in
      // the middle of one never saw it begin.
      if (!payload.settled) {
        const was = t.working;
        t.working = true;
        t.writing = (t.writing || "") + payload.text;
        // Only what they change: the words at the bottom, and the side the
        // first time they say it is working. The whole conversation and the
        // whole side were drawn again for every few words.
        if (payload.conversation === showing) {
          const following = atTheBottom();
          drawTheTail(t);
          if (following) el.messages.scrollTop = el.messages.scrollHeight;
        }
        if (!was) {
          const a = agents.get(t.agent);
          if (a && payload.conversation === showing) drawMark(a);
          drawThreads();
          // Running, at the top of the task and above the box, from its
          // first words rather than its last.
          if (payload.conversation === showing) {
            drawTaskCard();
            drawRunningNote();
          }
        }
        return;
      }
      t.working = true;
      t.writing = "";
      t.messages.push({ kind: "said", text: payload.text, seq: payload.seq });
      // In a call it is also read out, as each line settles rather than all at
      // once at the end: the first paragraph is spoken while the second is
      // still being written.
      if (inACall && payload.conversation === showing) sayOutLoud(payload.text);
      break;

    case "doing":
      t.working = true;
      if (payload.conversation === showing) dropTheSuggestion();
      // Words still unsettled when a step begins were not kept: an engine
      // settles what it says before it acts, so these are an answer taken
      // back, like one sent back for claiming work nothing did. Left on
      // screen, they were read as said.
      t.writing = "";
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
      if (payload.conversation === showing) offerTheNextThing(showing);
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
  // A turn that ended somewhere nobody is looking has left something unread,
  // and the count under the agent's name is the app's, not this window's:
  // asked for again, rather than left at whatever it was when the window last
  // had a reason to ask. Without this, an agent asked from a terminal sat
  // under "Nothing said yet" with its answer on disk.
  if (payload.kind === "done" || payload.kind === "failed") {
    // Watched as it arrived, it has been read. Arriving in the task on screen
    // while nobody was looking, it waits with a dot like any other.
    if (payload.conversation === showing && lookedAt()) nowSeen(showing);
    else whatIsNew();
  }
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
  // When it was said, on the outer edge of the bubble: left under an answer,
  // right under your own words, where the row is pushed to. Always in view;
  // the buttons beside it come on hover.
  if (m.kind !== "mine") row.append(stamped(m));

  const copy = document.createElement("button");
  copy.type = "button";
  copy.textContent = "Copy";
  copy.onclick = async () => {
    await navigator.clipboard.writeText(m.text);
    copy.textContent = "Copied";
    setTimeout(() => (copy.textContent = "Copy"), 1400);
  };
  row.append(copy);

  // Read aloud, the way an answer is read in a call. Kept in view while it is
  // being read, so Stop is there without hovering for it.
  if (m.kind === "said") {
    const on = listeningTo === m;
    const listen = document.createElement("button");
    listen.type = "button";
    listen.textContent = on ? "Stop" : "Listen";
    if (on) {
      listen.className = "on";
      row.classList.add("listening");
    }
    listen.onclick = () => readAloud(m);
    row.append(listen);
  }

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
  if (m.kind === "mine") row.append(stamped(m));
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

/**
 * What is waiting to go with the next message.
 *
 * The picture, not its name. A pasted screenshot is called `image.png` by the
 * system, so a row of chips reading "image.png, image.png" is the app knowing
 * exactly what somebody attached and showing them the least useful fact about
 * it -- and there is no way to tell two of them apart, or to notice that the
 * wrong one was pasted, until after it has been sent.
 */
/**
 * What is stopping errands from working, above the box.
 *
 * Asked of the app rather than remembered here, because the window is not
 * present for most of the ways it is discovered: a routine failing at seven, a
 * watch waking something at lunchtime. Shown before anything is typed, which is
 * the whole point -- the failure that put this here arrived after somebody had
 * written a paragraph, attached a screenshot and asked for a daily errand, all
 * of which was spent before the app admitted it could not sign in.
 */
async function drawTrouble() {
  let wrong = null;
  try {
    wrong = await invoke("whats_wrong");
  } catch {
    // Nothing said is better than a warning the app cannot stand behind.
  }
  if (!wrong) {
    el.trouble.hidden = true;
    el.trouble.replaceChildren();
    return;
  }
  el.trouble.replaceChildren(note("p", wrong.said, "what"), note("p", wrong.fix, "fix"));
  el.trouble.hidden = false;
}

listen("trouble", ({ payload }) => {
  if (!payload?.until_somebody_acts) return;
  el.trouble.replaceChildren(note("p", payload.said, "what"), note("p", payload.fix, "fix"));
  el.trouble.hidden = false;
});

// Anything getting through means whatever was wrong is not wrong any more.
listen("trouble_over", () => {
  el.trouble.hidden = true;
  el.trouble.replaceChildren();
});

// A way in for the window harness, which cannot paste or drop. Named like the
// other stand-in seams so it reads as one: the alternative is a check that
// tests a function rather than the strip somebody looks at.
window.__ATTACH__ = (one) => {
  attached.push(one);
  drawAttached();
};

function drawAttached() {
  el.attached.hidden = attached.length === 0;
  el.attached.replaceChildren(
    ...attached.map((one, at) => {
      const held = document.createElement("figure");
      held.className = "attached-one";

      const img = document.createElement("img");
      img.alt = one.name || "a picture";
      // A pasted picture is already a data URL and needs nothing. A dropped one
      // is a path, and the window cannot read a file: the app does, under the
      // same rules it applies a moment later when it sends the same bytes.
      if (/^data:/.test(one.url)) {
        img.src = one.url;
      } else {
        invoke("a_picture_to_send", { path: one.url })
          .then((url) => {
            img.src = url;
          })
          .catch((why) => {
            // Named instead, which is where this started. Better than an empty
            // box, and it says why rather than looking broken.
            held.classList.add("unshown");
            img.replaceWith(note("span", one.name || "a picture", "what"));
            held.title = String(why);
          });
      }
      img.onclick = () => img.src && lookCloser(img.src, one.name);
      held.append(img);

      // Its own control rather than the picture itself, because clicking a
      // picture means "show me it" everywhere else and taking something away
      // is not a thing to do by accident.
      const off = document.createElement("button");
      off.type = "button";
      off.className = "take-off";
      off.textContent = "\u{00d7}";
      off.title = `Take ${one.name || "this"} off again`;
      off.setAttribute("aria-label", off.title);
      off.onclick = () => {
        attached.splice(at, 1);
        drawAttached();
      };
      held.append(off);
      return held;
    }),
  );
}

/** What the window says when something is said into a room mid-round. */
const ROOM_STILL_ANSWERING =
  "The room is still answering. It takes one thing round at a time; say it again when the round is over.";

el.form.addEventListener("submit", async (e) => {
  e.preventDefault();
  dropTheSuggestion();
  const text = el.what.value.trim();
  // A picture on its own is a question: "what is this". So something has to be
  // said, but it does not have to be typed.
  if (!text && !attached.length) return;
  el.slash.hidden = true;
  // A line naming one of its skills runs it, with anything after the name as
  // what to do differently. Anything else starting with / is somebody's own
  // words, a path included, and goes as it is.
  const a = whose();
  // Something new asked in a task marked finished is that task going again.
  const going = talking();
  if (going?.finished) markTaskFinished(going, false);
  if (a && text.startsWith("/")) {
    const called = aSkillCalledFor(text, await readSkills(a.id));
    if (called) {
      halfTyped.delete(showing);
      el.what.value = "";
      el.what.style.height = "auto";
      await runTheSkill(a, called.one.name, called.differently);
      return;
    }
  }
  // A room takes one thing round at a time. Refused here, with the words left
  // in the box, rather than sent: the app refuses it too, but by then the box
  // is empty and the line is on screen as though it went.
  const t = talking();
  if (t && t.members.length > 1 && t.working) {
    const last = t.messages[t.messages.length - 1];
    if (!(last?.kind === "ended" && last.text === ROOM_STILL_ANSWERING)) {
      t.messages.push({ kind: "ended", failed: false, text: ROOM_STILL_ANSWERING });
      drawMessages();
    }
    return;
  }
  // Sent is not half typed. Without this the draft comes back the next time
  // this conversation is opened, under the message it already became.
  halfTyped.delete(showing);
  el.what.value = "";
  el.what.style.height = "auto";
  sayIt(text || "What is this?", takeThePictures());
});

/**
 * The pictures waiting in the box, taken so they go once.
 *
 * Only sending from the box takes them. "Ask again", "Run it again" and
 * trying a routine say words of their own, and took whatever screenshot was
 * waiting for the next thing somebody meant to type.
 */
function takeThePictures() {
  const going = attached.map((one) => one.url);
  attached = [];
  picturesWaiting.delete(showing);
  drawAttached();
  return going;
}

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
async function sayIt(text, going = []) {
  const t = talking();
  if (!t) return;
  // Shown from the moment it is sent, out of what is already in hand, rather
  // than waiting for a round trip to disk and back to see what was attached.
  const mine = { kind: "mine", text, showing: going };
  t.messages.push(mine);
  // Working from the moment it is sent, not from the moment something comes
  // back: the gap between the two is exactly when a person wonders whether the
  // thing they typed went anywhere.
  t.working = true;
  drawMessages({ follow: true });
  drawThreads();

  try {
    // Stamped with where it landed, so it can be carried on from without
    // waiting for a reload. Optimistic on the way out and corrected on the way
    // back, because the alternative is a message that sits there unshown until
    // the store has answered.
    mine.seq = await invoke("say", { id: t.id, text, attached: going.length ? going : null });
    drawMessages();
    // The first thing asked is what the task says it does. The list was read
    // only at the start and in Mission Control, so a new task's card said
    // "Nothing asked yet" through the whole of its first run.
    if (!tasksNow.find((x) => x.id === t.id)?.first) {
      await readTasks();
      if (showing === t.id) drawTaskCard();
    }
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
// Where the walk has got to is kept with the box, beside `halfTyped`.
el.what.addEventListener("keydown", (e) => {
  // Tab takes the suggestion, and only into an empty box, so it never stands
  // in for anything else Tab does.
  if (e.key === "Tab" && !e.shiftKey && suggested && suggested.for === showing && !el.what.value) {
    e.preventDefault();
    el.what.value = suggested.text;
    suggested = null;
    askedForAnAnswer(talking());
    el.what.style.height = "auto";
    el.what.style.height = Math.min(el.what.scrollHeight, window.innerHeight * 0.4) + "px";
    el.what.setSelectionRange(el.what.value.length, el.what.value.length);
    return;
  }
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
  if (el.what.value) dropTheSuggestion();
  el.what.style.height = "auto";
  el.what.style.height = Math.min(el.what.scrollHeight, window.innerHeight * 0.4) + "px";
});

el.engine.addEventListener("change", async () => {
  // Not a model: the way to where models are added, which is this same screen.
  if (el.engine.value === "__add__") {
    drawEngines();
    const hand = document.getElementById("hand-label");
    hand?.scrollIntoView({ block: "center" });
    hand?.focus();
    return;
  }
  const choice = (await whatCouldAnswer()).find((c) => c.id === el.engine.value);
  if (!choice) return;
  try {
    await invoke("set_setting", { key: "errand_model", value: choice.id });
    errandModel = choice.id;
    drawWordsGo();
    // When it takes effect, said plainly: nobody is cut off mid-task by it.
    el.errandModelSays.textContent = `Every teammate now works on ${choice.name}, except one given a model of its own. One in the middle of something finishes on the model it started with.`;
  } catch (why) {
    el.errandModelSays.textContent = String(why);
  }
  drawEngines();
  if (!el.whois.hidden) {
    drawOwnModel();
    drawLimit();
  }
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
/** Start an agent from a file one was saved to, and go to it. */
async function loadAnAgent(path) {
  let id;
  try {
    id = await invoke("load_agent", { path });
  } catch (why) {
    complain(String(why));
    return;
  }
  if (await meetAgent(id)) await openAgent(id);
}
// For the window harness, which cannot drop a file.
window.__LOAD_AGENT__ = loadAnAgent;

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

    // An agent saved to a file starts one like it here.
    const anAgent = /\.errand\.json$/i;
    for (const path of payload.paths.filter((p) => anAgent.test(p))) loadAnAgent(path);

    // A picture is attached; anything else is still a path in the box, which
    // is what dropping a file did before pictures were understood and is
    // still the right thing for a spreadsheet or a folder.
    const looksLikeAPicture = /\.(png|jpe?g|gif|webp)$/i;
    const pictures = payload.paths.filter((p) => looksLikeAPicture.test(p));
    const rest = payload.paths.filter((p) => !looksLikeAPicture.test(p) && !anAgent.test(p));

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
  await drawTheServers();
});

async function drawTheServers() {
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
  const engine = whose()?.runsOn || whose()?.on || "claude";
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
    note(
      "p",
      engine === "local"
        ? "MCP servers, read from ~/.claude.json. Errand starts only the ones you allow here, as they were when you allowed them: they run as you, outside every wall."
        : "MCP servers, read from ~/.claude.json. Claude Code starts these itself, as its own children: inside the wall when it never asks, and asking first otherwise. What you allow here is for agents on other models.",
    ),
    ...servers.map((s) => {
      const box = document.createElement("div");
      // Not allowed is a choice, not a fault, so it is not drawn in red.
      const chosen = s.standing && s.standing !== "allowed";
      box.className = s.trouble && !chosen ? "server broken" : chosen ? "server held" : "server";
      box.dataset.server = s.name;

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
      // What to do about it, under what went wrong. Red alone told somebody
      // something was broken and nothing about whether or how to mend it.
      if (s.trouble && s.fix) box.append(note("p", s.fix, "server-fix"));
      // What it runs, and the person's say over whether Errand starts it.
      if (s.standing === "not yet" || s.standing === "changed") {
        box.append(note("code", s.shown, "server-runs"));
        if (s.screen) {
          box.append(
            note(
              "p",
              "It drives your screen: it can click, type and look at anything, including Errand's own cards and a terminal, which runs outside every wall. Teammates on auto use it without asking.",
              "server-warning",
            ),
          );
        }
        // Not the inviting button for one that drives the screen: allowing that
        // should be a decision, not the obvious next step.
        box.append(serverButton("Allow", s.screen ? "" : "yes", () => invoke("allow_server", { name: s.name, fingerprint: s.fingerprint })));
      } else if (s.standing === "allowed") {
        box.append(serverButton("Stop allowing", "", () => invoke("stop_allowing_server", { name: s.name })));
      }
      return box;
    }),
  );
}

/** A button on a server's row that changes what Errand starts, then shows the list again. */
function serverButton(label, kind, act) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = kind ? `server-act ${kind}` : "server-act";
  b.textContent = label;
  b.onclick = async () => {
    b.disabled = true;
    try {
      await act();
    } catch (why) {
      b.disabled = false;
      b.after(note("p", String(why), "server-fix"));
      return;
    }
    await drawTheServers();
  };
  return b;
}

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
  el.whoisNew.hidden = t.name !== NOT_YET_NAMED;
  el.whoisLocal.checked = !!t.keepLocal;
  drawOwnModel();
  el.whois.hidden = false;
  el.whoisName.focus();
  drawHome();
  drawChecklist();
  drawNotes();
  drawSkills();
  drawLimit();
});

/**
 * Teammates whose home has an edit of the person's waiting, by id. Told by the
 * app when one appears or is settled, so the list down the side can say so
 * without anybody opening each card to look.
 */
const homeEdited = new Set();

/**
 * Its home: where it is, and any file the person changed there that has not
 * been taken yet, with what changed in words and the choice of taking it or
 * putting the file back. Nothing is taken until they say so: these files are
 * the teammate's instructions.
 */
async function drawHome() {
  const a = whose();
  if (!a) return;
  let home = { path: "", edits: [] };
  try {
    home = (await invoke("home_of", { agent: a.id })) || home;
  } catch {
    // Not made yet, for a teammate that has only just been named.
  }
  if (whose()?.id !== a.id) return;
  el.homePath.textContent = home.path || "being made";
  el.homePath.title = home.path || "";
  el.homeEdits.replaceChildren(
    ...(home.edits || []).map((one) => {
      const li = document.createElement("li");
      li.dataset.path = one.path;
      li.append(note("p", `You changed ${one.path}`, "edit-what"));
      const changes = document.createElement("ul");
      changes.className = "edit-changes";
      for (const change of one.changes || []) changes.append(note("li", change, ""));
      li.append(changes);
      const choices = document.createElement("div");
      choices.className = "edit-choices";
      const said = note("p", "", "edit-said");
      const press = (label, primary, act) => {
        const b = document.createElement("button");
        b.type = "button";
        b.textContent = label;
        if (primary) b.className = "yes";
        b.onclick = async () => {
          for (const each of choices.querySelectorAll("button")) each.disabled = true;
          try {
            const refused = await act();
            if (refused?.length) {
              said.textContent = `Taken, except: ${refused.join(" ")}`;
              li.replaceChildren(said);
              setTimeout(() => drawAfterTheHomeChanged(a), 4000);
              return;
            }
          } catch (why) {
            said.textContent = String(why);
            for (const each of choices.querySelectorAll("button")) each.disabled = false;
            return;
          }
          await drawAfterTheHomeChanged(a);
        };
        return b;
      };
      if (one.takeable) {
        choices.append(press("Take these edits", true, () => invoke("take_home_edit", { agent: a.id, path: one.path })));
      }
      choices.append(press("Put the file back", !one.takeable, () => invoke("put_home_back", { agent: a.id, path: one.path })));
      li.append(choices, said);
      return li;
    }),
  );
  if ((home.edits || []).length) homeEdited.add(a.id);
  else homeEdited.delete(a.id);
  drawThreads();
}

/** After an edit was taken or put back: who it is, its notes and its checklist may all have changed. */
async function drawAfterTheHomeChanged(a) {
  await meetAgent(a.id);
  const now = agents.get(a.id);
  if (now && whose()?.id === a.id) {
    el.whoisName.value = now.name === NOT_YET_NAMED ? "" : now.name;
    el.whoisTitle.value = now.title;
    el.whoisAbout.value = now.about;
    drawPurpose(now);
  }
  await drawHome();
  drawChecklist();
  drawNotes();
}

el.homeShow.addEventListener("click", () => {
  const a = whose();
  if (a) invoke("show_home", { agent: a.id }).catch(() => {});
});

listen("home_edited", async ({ payload }) => {
  const agent = String(payload || "");
  if (!agent) return;
  let home = null;
  try {
    home = await invoke("home_of", { agent });
  } catch {
    return;
  }
  if ((home?.edits || []).length) homeEdited.add(agent);
  else homeEdited.delete(agent);
  if (whose()?.id === agent && !el.whois.hidden) await drawHome();
  else drawThreads();
});

/**
 * How this teammate checks its work: the points it goes through before it
 * may say a task is done, read into every conversation it has.
 *
 * Its role was a word on a card. A role the app recognises offers a starter
 * list, and the person keeps, changes or drops each point.
 */
let checklistNow = { agent: null, points: [], starter: null };

async function drawChecklist() {
  const a = whose();
  if (!a) return;
  el.checklistSays.textContent = "";
  let got = { points: [], starter: null };
  try {
    got = (await invoke("checklist_of", { agent: a.id })) || got;
  } catch {
    // Drawn empty: the list can still be written.
  }
  checklistNow = { agent: a.id, points: got.points || [], starter: got.starter || null };
  const points = checklistNow.points;
  el.checklistSummary.textContent = points.length
    ? `How it checks its work (${points.length} ${points.length === 1 ? "point" : "points"})`
    : "How it checks its work";
  // Offered only to an empty list: a starter is a start, not a second list.
  const [called, starts] = checklistNow.starter || [];
  el.checklistStarter.hidden = !(points.length === 0 && starts?.length);
  if (!el.checklistStarter.hidden) {
    el.checklistStarterSays.textContent = `Its role has a starter list, the ${called} checklist: ${starts.join("; ")}.`;
  }
  el.checklistList.replaceChildren(
    ...(points.length
      ? points.map((point, i) => {
          const li = document.createElement("li");
          const row = document.createElement("div");
          row.className = "point";
          const out = document.createElement("button");
          out.type = "button";
          out.textContent = "Take out";
          out.onclick = () => keepChecklist(points.filter((_, j) => j !== i));
          row.append(note("span", point, "point-text"), out);
          li.append(row);
          return li;
        })
      : [
          note(
            "li",
            "Nothing yet. Each point is something it goes through before it says a task is done; a point that fails means not done yet.",
            "point-none",
          ),
        ]),
  );
}

async function keepChecklist(points) {
  const agent = checklistNow.agent;
  if (!agent) return;
  try {
    checklistNow.points = await invoke("set_checklist", { agent, points });
  } catch (why) {
    el.checklistSays.textContent = String(why);
    return;
  }
  if (whose()?.id === agent) await drawChecklist();
}

el.checklistUse.addEventListener("click", () => {
  const [, starts] = checklistNow.starter || [];
  if (starts?.length) keepChecklist([...checklistNow.points, ...starts]);
});
el.checklistNew.addEventListener("submit", async (e) => {
  e.preventDefault();
  const point = el.checklistPoint.value.trim();
  if (!point) return;
  el.checklistPoint.value = "";
  await keepChecklist([...checklistNow.points, point]);
});

/**
 * How much this agent may use in a month, beside what it has used this one.
 *
 * Dollars for Claude, which says what each errand cost, and tokens for a model
 * paid for by the token, which is what is counted for those. Past it, the
 * agent is paused: nothing of it runs on its own until it is started again.
 */
async function drawLimit() {
  const a = whose();
  if (!a) return;
  el.limitSays.textContent = "";
  let seen;
  try {
    seen = await invoke("limits", { agent: a.id });
  } catch (why) {
    el.limitSays.textContent = String(why);
    return;
  }
  // By the model it actually runs on, which its own or Errand's may have
  // chosen, rather than the engine it was first put on.
  const where = await invoke("where_words_go", { id: a.id }).catch(() => null);
  if (where?.engine) a.runsOn = where.engine;
  const inDollars = (a.runsOn || a.on) === "claude";
  // The unit the form shows, kept with the form: Save reads it from here, not
  // from the teammate, which is rebuilt whenever the list is read again.
  el.limitForm.dataset.agent = a.id;
  el.limitForm.dataset.unit = inDollars ? "dollars" : "tokens";
  const set = inDollars ? seen.dollars : seen.tokens;
  const said = set == null ? "none" : inDollars ? `$${set}` : tokensSaid(set);
  el.limitSummary.textContent = `Monthly limit: ${said}`;
  el.limitUnit.textContent = inDollars ? "Dollars a month" : "Tokens a month";
  el.limitValue.value = set == null ? "" : inDollars ? String(set) : tokensSaid(set);
  el.limitUsed.textContent = inDollars
    ? `Spent $${seen.spent_dollars.toFixed(2)} this month.`
    : `Used ${tokensSaid(seen.used_tokens)} tokens this month.`;
}

/** "5M", "500k", "2,000,000" or "$20" as a number; nothing for no limit. */
function anAmount(text) {
  const said = String(text).trim().replace(/[$,\s]/g, "").toLowerCase();
  if (!said) return null;
  const read = said.match(/^(\d+(?:\.\d+)?)([km]?)$/);
  if (!read) return NaN;
  return parseFloat(read[1]) * (read[2] === "m" ? 1e6 : read[2] === "k" ? 1e3 : 1);
}

el.limitForm.addEventListener("submit", async (e) => {
  e.preventDefault();
  const a = whose();
  if (!a) return;
  const inDollars =
    el.limitForm.dataset.agent === a.id ? el.limitForm.dataset.unit === "dollars" : (a.runsOn || a.on) === "claude";
  const amount = anAmount(el.limitValue.value);
  if (Number.isNaN(amount)) {
    el.limitSays.textContent = inDollars ? "Say it in dollars, like 20." : "Say it in tokens, like 5M or 500k.";
    return;
  }
  try {
    await invoke("set_limits", {
      agent: a.id,
      tokens: inDollars || amount === null ? null : Math.round(amount),
      dollars: inDollars ? amount : null,
    });
  } catch (why) {
    el.limitSays.textContent = String(why);
    return;
  }
  await drawLimit();
  el.limitSays.textContent =
    amount === null ? "No limit." : "Kept. Past it, it is paused, and its conversation says why.";
});

/** Each agent's skills, as last read from the app. */
const skillsOf = new Map();

async function readSkills(agent) {
  try {
    const all = await invoke("skills_of", { agent });
    skillsOf.set(agent, all);
    return all;
  } catch {
    return skillsOf.get(agent) || [];
  }
}

/**
 * What this agent has been taught, each with a way to run it again or take it
 * back. They ran only when asked for in words, and nothing could delete one.
 */
async function drawSkills() {
  const a = whose();
  if (!a) return;
  el.skillsSays.textContent = "";
  const all = await readSkills(a.id);
  el.skillsSummary.textContent = all.length ? `What it has been taught (${all.length})` : "What it has been taught";
  el.skillsList.replaceChildren(
    ...(all.length
      ? all.map((one) => aSkill(a, one))
      : [
          note(
            "li",
            "Nothing yet. Once it has done something you want again, ask it to keep that as a skill, " +
              "and it can be run from here or by typing / in the box.",
          ),
        ]),
  );
}

function aSkill(a, one) {
  const li = document.createElement("li");
  const steps = one.steps?.length || 0;
  const run = document.createElement("button");
  run.type = "button";
  run.textContent = "Run";
  run.onclick = () => runTheSkill(a, one.name, "");
  const forget = document.createElement("button");
  forget.type = "button";
  forget.textContent = "Forget";
  forget.onclick = async () => {
    try {
      await invoke("forget_skill", { agent: a.id, name: one.name });
    } catch (why) {
      el.skillsSays.textContent = String(why);
      return;
    }
    drawSkills();
  };
  li.append(
    note("span", one.name, "skill-name"),
    note("span", `${one.request} · ${steps} ${steps === 1 ? "step" : "steps"}`, "skill-what"),
    run,
    forget,
  );
  return li;
}

/**
 * Run a skill in a conversation of its own, and go to it.
 *
 * The same arrangement as an agent running one, so the run is a record that
 * can be opened afterwards under the skill's name, and it is watched as it
 * goes rather than announced when it is over.
 */
async function runTheSkill(a, name, differently) {
  let talk;
  try {
    talk = await invoke("run_a_skill", { agent: a.id, name, differently: differently || null });
  } catch (why) {
    complain(String(why));
    return;
  }
  el.whois.hidden = true;
  if (await meet(talk)) await show(talk);
}

/** The skill a line starting with / names, and what else it says. */
function aSkillCalledFor(text, all) {
  if (!text.startsWith("/")) return null;
  const said = text.slice(1);
  const lower = said.toLowerCase();
  const one = all.find((s) => lower === s.name.toLowerCase() || lower.startsWith(`${s.name.toLowerCase()} `));
  return one ? { one, differently: said.slice(one.name.length).trim() } : null;
}

/**
 * Its skills, offered as soon as a line starts with /, the way Grok Bot puts
 * them behind it. Only the ones whose names start with what has been typed.
 */
async function offerSkills() {
  const typed = el.what.value;
  if (typed.startsWith("@")) return offerMembers(typed);
  const a = whose();
  if (!a || !typed.startsWith("/") || typed.includes("\n")) {
    el.slash.hidden = true;
    return;
  }
  const all = skillsOf.get(a.id) ?? (await readSkills(a.id));
  const word = typed.slice(1).toLowerCase();
  const fits = all.filter((one) => one.name.toLowerCase().startsWith(word));
  if (!fits.length) {
    el.slash.hidden = true;
    return;
  }
  el.slash.replaceChildren(
    ...fits.map((one) => {
      const li = document.createElement("li");
      li.append(note("span", `/${one.name}`, "skill-name"), note("span", one.request, "skill-what"));
      li.onclick = () => {
        el.what.value = `/${one.name} `;
        el.slash.hidden = true;
        el.what.focus();
      };
      return li;
    }),
  );
  el.slash.hidden = false;
}

el.what.addEventListener("input", offerSkills);

/**
 * In a room, the members, as soon as a line starts with @. A name typed
 * slightly wrong went to nobody, and the room said so after the fact.
 */
function offerMembers(typed) {
  const t = talking();
  const members = t?.members || [];
  const word = typed.slice(1).toLowerCase();
  const fits = members.filter((m) => m.name.toLowerCase().startsWith(word) && !typed.includes(" "));
  if (members.length < 2 || !fits.length) {
    el.slash.hidden = true;
    return;
  }
  el.slash.replaceChildren(
    ...fits.map((m) => {
      const li = document.createElement("li");
      li.append(note("span", `@${m.name}`, "skill-name"), note("span", "just this one", "skill-what"));
      li.onclick = () => {
        el.what.value = `@${m.name} `;
        el.slash.hidden = true;
        el.what.focus();
      };
      return li;
    }),
  );
  el.slash.hidden = false;
}

/**
 * What this agent has written down, and a way to correct or take back each.
 *
 * Read into every conversation it has, so a note that is wrong is wrong every
 * time, and nothing on screen used to show one.
 */
async function drawNotes() {
  const a = whose();
  if (!a) return;
  el.notesSays.textContent = "";
  let notes = [];
  try {
    notes = await invoke("notes", { agent: a.id });
  } catch (why) {
    el.notesSays.textContent = String(why);
    return;
  }
  el.notesSummary.textContent = notes.length ? `What it remembers (${notes.length})` : "What it remembers";
  el.notesList.replaceChildren(
    ...(notes.length
      ? notes.map((one) => aNote(a, one))
      : [note("li", "Nothing yet. It writes things down when it learns something worth keeping, and you can too.")]),
  );
}

function aNote(a, one) {
  const li = document.createElement("li");
  const about = note("span", one.about, "note-about");
  const text = note("span", one.note, "note-text");
  const change = document.createElement("button");
  change.type = "button";
  change.textContent = "Change";
  change.onclick = () => {
    const box = document.createElement("input");
    box.type = "text";
    box.value = one.note;
    box.setAttribute("aria-label", `What it remembers about ${one.about}`);
    const keep = document.createElement("button");
    keep.type = "button";
    keep.textContent = "Keep";
    keep.onclick = async () => {
      if (await writeItDown(a, one.about, box.value)) drawNotes();
    };
    box.addEventListener("keydown", (e) => {
      if (e.key === "Enter") keep.click();
      if (e.key === "Escape") drawNotes();
    });
    text.replaceWith(box);
    change.replaceWith(keep);
    box.focus();
  };
  const forget = document.createElement("button");
  forget.type = "button";
  forget.textContent = "Forget";
  forget.onclick = async () => {
    try {
      await invoke("unnote", { agent: a.id, about: one.about });
    } catch (why) {
      el.notesSays.textContent = String(why);
      return;
    }
    drawNotes();
  };
  li.append(about, text, change, forget);
  return li;
}

/** Write a note down, saying in the panel why not when the app refuses it. */
async function writeItDown(a, about, text) {
  try {
    await invoke("note_down", { agent: a.id, about, note: text });
    return true;
  } catch (why) {
    // The same rules as a note the agent writes, a key included, and the
    // reason is worth reading: it says what to leave out.
    el.notesSays.textContent = String(why);
    return false;
  }
}

el.noteNew.addEventListener("submit", async (e) => {
  e.preventDefault();
  const a = whose();
  if (!a) return;
  const about = el.noteAbout.value.trim();
  const text = el.noteText.value.trim();
  if (!about || !text) {
    el.notesSays.textContent = "Say what it is about and what to remember.";
    return;
  }
  if (await writeItDown(a, about, text)) {
    el.noteAbout.value = "";
    el.noteText.value = "";
    drawNotes();
  }
});

el.whoisSave.addEventListener("click", async () => {
  const t = whose();
  if (!t) return;
  t.name = el.whoisName.value.trim() || NOT_YET_NAMED;
  t.title = el.whoisTitle.value.trim();
  t.about = el.whoisAbout.value.trim();
  el.whois.hidden = true;
  el.name.textContent = t.name;
  drawPurpose(t);
  drawThreads();
  // Ready for its first task, or its next one.
  el.what.focus();
  await invoke("rename", { id: t.id, name: t.name, title: t.title, about: t.about });
});

// Enter in any of the three saves, the way a form does.
for (const field of [el.whoisName, el.whoisTitle, el.whoisAbout]) {
  field.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" || e.isComposing) return;
    e.preventDefault();
    el.whoisSave.click();
  });
}

/** Keep a teammate at the top of the list, or let it take its place again. */
async function setPinned(a, pinned) {
  a.pinned = pinned;
  drawThreads();
  await invoke("pin", { id: a.id, pinned });
}

/** Put a teammate under Hidden at the bottom of the list, or bring it back. */
async function setHidden(a, hidden) {
  a.hidden = hidden;
  drawThreads();
  await invoke("hide", { id: a.id, hidden });
}

// Everything done to a teammate once in a while, behind the last button in the
// header: the same menu as a right-click on it in the list, so there is one
// place for it rather than two that drift.
el.more.addEventListener("click", (e) => {
  // The document closes the menu on any click, this one included.
  e.stopPropagation();
  const a = whose();
  if (!a) return;
  if (!el.menu.hidden && menuIsFor === a.id) {
    closeTheMenu();
    return;
  }
  const box = el.more.getBoundingClientRect();
  openTheMenu(a, box.right, box.bottom + 6);
  el.more.setAttribute("aria-expanded", "true");
});

el.pause.addEventListener("click", async () => {
  const t = whose();
  if (!t) return;
  await setPaused(t, !t.paused);
});

/**
 * Pause an agent, or start it again.
 *
 * One switch for everything it does on its own. There was a Pause under
 * Repeat, for one routine at a time, and stopping a bot with three routines
 * and a watch meant finding and switching off four things and then finding
 * them again. Paused, the clock walks past all of it, a goal stops carrying
 * on, and whatever it is in the middle of is stopped. Spoken to, it still
 * answers. Nothing it has is thrown away.
 */
async function setPaused(a, paused) {
  const was = a.paused;
  a.paused = paused;
  // Shown as paused at once, and its conversations as stopped: the app stops
  // them and says so, but the row should not go on saying "Running now" for the
  // half second that takes.
  if (paused) {
    for (const t of talks.values()) if (t.agent === a.id && t.working) itHasStopped(t);
  }
  if (a.id === showingAgent) {
    drawPaused(a);
    drawMark(a);
    drawMessages();
  }
  drawThreads();
  try {
    await invoke("pause", { id: a.id, paused });
  } catch (why) {
    a.paused = was;
    if (a.id === showingAgent) drawPaused(a);
    drawThreads();
    complain(String(why));
  }
}

/** Pause, saying which way it is. */
function drawPaused(t) {
  el.pause.textContent = t.paused ? "Paused" : "Pause";
  el.pause.setAttribute("aria-pressed", String(t.paused));
  el.pause.title = t.paused
    ? "Paused. Nothing runs on its own until you start it again"
    : "Nothing runs on its own until you start it again";
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

  // On its card, where its name is. This was a dialog, which a window on a Mac
  // does not always show.
  item("Rename this task…", async () => {
    closeTheMenu();
    if (showing !== t.id) await show(t.id);
    renameTheTask(t);
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
      await forgetTheTask(t);
    },
    "danger",
  );
  remove.dataset.sure = "false";

  el.menu.replaceChildren(...items);
  el.menu.hidden = false;
  placeTheMenu(x, y);
}

el.taskDone.addEventListener("click", () => {
  const t = talking();
  if (t) markTaskFinished(t, !t.finished);
});

el.talks.addEventListener("change", async () => {
  if (el.talks.value === "+") {
    // The menu goes back to the task on screen; the new one is made once
    // something is said in it.
    el.talks.value = showing || "";
    return alsoAsk();
  }
  if (el.talks.value === "room") return offerARoom();
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
  onlyThisOneOpen(el.routine);
  const mine = (await invoke("routines")).find((r) => r.conversation === t.id);
  theRoutineShown = mine || null;
  el.routineAt.value = mine?.at || "";
  // Filled in with this conversation's task, so that saving without typing
  // anything simply repeats it.
  el.routineWhat.value = mine?.what || theTaskHere(t);
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
 * Offered beside the one Repeat fills in, which is the first: neither the
 * first thing asked nor the last is reliably the right one, the first is
 * usually the fullest and the last is often "yes, that one", and the person
 * who refined it is the one who knows which.
 *
 * @param {HTMLElement} where the row to draw them in
 * @param {HTMLInputElement} into the box a chosen one goes into
 */
/**
 * What this conversation was for, the way a routine is told it: the first
 * thing asked here.
 *
 * Filled in when Repeat opens on a conversation with nothing repeating yet.
 * Asked what a routine should do each time, with an empty box, somebody who
 * only wanted this task again had nothing to go on: a preset that simply
 * repeats it is what they expected, and the chips beside it were not read as
 * one.
 */
function theTaskHere(t) {
  const first = t?.messages.find(
    (m) => m.kind === "mine" && !m.text.startsWith(NOT_ASKED_BY_ANYBODY),
  );
  return first ? withoutWhoAsked(first.text) : "";
}

/**
 * A request without the words saying who asked it: "something outside asks:"
 * for the terminal, or an agent's name and "asks:". They say who, in the
 * conversation; in a routine, repeated, they would be the routine asking.
 */
function withoutWhoAsked(text) {
  const outside = "something outside asks: ";
  if (text.startsWith(outside)) return text.slice(outside.length);
  for (const a of agents.values()) {
    const by = `${a.name} asks: `;
    if (text.startsWith(by)) return text.slice(by.length);
  }
  return text;
}

function offerWhatWasAskedHere(where, into) {
  const t = talking();
  const asked = t
    ? t.messages
        .filter((m) => m.kind === "mine" && !m.text.startsWith(NOT_ASKED_BY_ANYBODY))
        .map((m) => withoutWhoAsked(m.text))
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
  el.routineWentMore.hidden = true;
  const went = await runsOf(id);
  if (!went.length) return;
  showTheRuns(id, went);
  el.routineWent.hidden = false;
}

/** How many runs the app hands back at a time, and so whether there are more. */
const RUNS_AT_A_TIME = 20;

/** Which conversation the history is of, and the oldest run in it so far. */
let runsShown = { of: null, oldest: null };

/** A page of runs, newest first, or nothing when the app cannot say. */
async function runsOf(id, olderThan = null) {
  try {
    return await invoke("how_it_has_been_going", { id, olderThan });
  } catch {
    return [];
  }
}

/**
 * Runs added to the bottom of the history, and the way to the ones before.
 *
 * The newest twenty were all there was to see, which for a routine every five
 * minutes is under two hours: the night it failed ninety-five times in a row
 * was out of reach by breakfast.
 */
function showTheRuns(id, went) {
  el.routineWentList.append(...went.map(aRun));
  runsShown = { of: id, oldest: went.length ? went[went.length - 1].id : runsShown.oldest };
  el.routineWentMore.hidden = went.length < RUNS_AT_A_TIME;
}

function aRun(run) {
  const li = document.createElement("li");
  // A run with nothing against it never came back: the app was quit, or the
  // machine slept. That is its own outcome and not a failure.
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
}

el.routineWentMore.addEventListener("click", async () => {
  const { of, oldest } = runsShown;
  if (!of || oldest == null) return;
  el.routineWentMore.disabled = true;
  const older = await runsOf(of, oldest);
  el.routineWentMore.disabled = false;
  // Somebody moved to another conversation while these were on their way.
  if (runsShown.of !== of) return;
  showTheRuns(of, older);
});

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
  // Read back from the app rather than worked out here. Started again, it
  // counts from now, and the run it would have had before it was paused is
  // long gone: kept from when the panel opened, the line said the next run
  // was at 12:34 at a quarter past five.
  const now = (await invoke("routines").catch(() => [])).find((r) => r.conversation === t.id);
  theRoutineShown = now || { ...theRoutineShown, off };
  el.routinePause.textContent = theRoutineShown.off ? "Start again" : "Pause";
  el.routineSays.textContent = sayWhen(theRoutineShown);
  // The clock on the conversation goes with it: a paused routine is not one
  // the picker should still be advertising as scheduled.
  t.repeats = !theRoutineShown.off;
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
  const rule = ruleChosen();
  if (!a || rule === null) {
    el.allowSays.textContent = el.allowWhat.hidden
      ? "Choose what it may do."
      : "Say what it may do, like `curl` or `git status`.";
    return;
  }
  try {
    const covers = await invoke("allow_in_advance", {
      agent: a.id,
      tool: el.allowTool.value,
      rule,
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

/**
 * What can be allowed for this teammate, with the choices filled in.
 *
 * "Let it" was an empty box with `curl` in it whatever "Using" said, so
 * somebody letting a teammate write into Downloads had nothing to tell them it
 * wanted `/Users/them/Downloads`, whole. The app knows what each kind wants,
 * which folders and disks are on this Mac, and which programs are, so it says.
 */
let offeredToAllow = null;
const SOMETHING_ELSE = "something-else";
// A place is chosen with the Mac's own folder chooser, never typed: a path
// typed from memory is a path that can be wrong by a letter.
const ANOTHER_FOLDER = "another-folder";
const CHOSEN_FOLDER = "chosen-folder";
let folderChosen = null;

async function drawAllowing(a) {
  try {
    offeredToAllow = await invoke("allowing_choices", { agent: a.id });
  } catch {
    // Without the choices the form still works as it did: typed.
    offeredToAllow = null;
  }
  const kinds = offeredToAllow?.kinds || [];
  if (kinds.length) {
    const was = el.allowTool.value;
    el.allowTool.replaceChildren(
      ...kinds.map((k) => {
        const one = document.createElement("option");
        one.value = k.kind;
        one.textContent = k.using;
        return one;
      }),
    );
    if (kinds.some((k) => k.kind === was)) el.allowTool.value = was;
  }
  el.allowFewer.textContent = offeredToAllow?.fewer || "";
  el.allowFewer.hidden = !offeredToAllow?.fewer;
  drawLetIt();
}

function kindShown() {
  return (offeredToAllow?.kinds || []).find((k) => k.kind === el.allowTool.value);
}

function drawLetIt() {
  const kind = kindShown();
  const choices = kind?.choices || [];
  const place = !!kind?.a_place;
  folderChosen = null;
  // Nothing chosen to begin with: the list ends with the widest choice, and
  // that should never be the one picked by somebody who did not pick.
  const pick = document.createElement("option");
  pick.value = "";
  pick.textContent = choices.length || place ? "choose one" : "type it below";
  pick.disabled = true;
  const last = document.createElement("option");
  last.value = place ? ANOTHER_FOLDER : SOMETHING_ELSE;
  last.textContent = place ? "choose another folder…" : "something else…";
  el.allowChoice.replaceChildren(
    pick,
    ...choices.map((c, i) => {
      const one = document.createElement("option");
      one.value = String(i);
      one.textContent = c.said;
      return one;
    }),
    last,
  );
  el.allowChoice.value = choices.length || place ? "" : SOMETHING_ELSE;
  el.allowWhat.placeholder = kind?.to_type || "a program, like curl";
  el.allowWhat.hidden = place || choices.length > 0;
  el.allowMeans.textContent = "";
}

/** The rule chosen or typed: "" is a real one, the whole of a kind. */
function ruleChosen() {
  const picked = el.allowChoice.value;
  if (picked === CHOSEN_FOLDER) return folderChosen;
  if (picked && picked !== SOMETHING_ELSE && picked !== ANOTHER_FOLDER) {
    return kindShown()?.choices?.[Number(picked)]?.rule ?? null;
  }
  // A place is never typed.
  if (kindShown()?.a_place) return null;
  const typed = el.allowWhat.value.trim();
  return typed || null;
}

/** The Mac's own folder chooser, and the folder it gave, added and chosen. */
async function chooseAnotherFolder() {
  const kind = kindShown();
  const who = whose()?.name || "it";
  const why =
    kind?.kind === "folder"
      ? `Choose a folder ${who} may write in.`
      : `Choose a folder for ${who}: ${kind?.using || "this"}.`;
  let chosen = null;
  try {
    chosen = await invoke("choose_a_folder", { why });
  } catch (err) {
    el.allowMeans.textContent = String(err);
  }
  if (!chosen) {
    // Cancelled: back to whatever was chosen before, which may be nothing.
    el.allowChoice.value = folderChosen ? CHOSEN_FOLDER : "";
    sayWhatItWouldAllow();
    return;
  }
  folderChosen = chosen;
  let one = el.allowChoice.querySelector(`option[value="${CHOSEN_FOLDER}"]`);
  if (!one) {
    one = document.createElement("option");
    one.value = CHOSEN_FOLDER;
    el.allowChoice.insertBefore(one, el.allowChoice.lastElementChild);
  }
  const name = chosen.split("/").filter(Boolean).pop() || chosen;
  one.textContent = `${name} (${chosen})`;
  el.allowChoice.value = CHOSEN_FOLDER;
  sayWhatItWouldAllow();
}

let meaningAsked = 0;
async function sayWhatItWouldAllow() {
  const rule = ruleChosen();
  const asked = ++meaningAsked;
  if (rule === null) {
    el.allowMeans.textContent = "";
    return;
  }
  let said;
  try {
    said = `This would allow ${await invoke("what_allowing_means", { tool: el.allowTool.value, rule })}.`;
  } catch (why) {
    said = String(why);
  }
  if (asked === meaningAsked) el.allowMeans.textContent = said;
}

el.allowTool?.addEventListener("change", () => {
  el.allowSays.textContent = "";
  drawLetIt();
});
el.allowChoice?.addEventListener("change", () => {
  el.allowSays.textContent = "";
  if (el.allowChoice.value === ANOTHER_FOLDER) {
    el.allowWhat.hidden = true;
    chooseAnotherFolder();
    return;
  }
  const typing = el.allowChoice.value === SOMETHING_ELSE;
  el.allowWhat.hidden = !typing;
  if (typing) el.allowWhat.focus();
  sayWhatItWouldAllow();
});
let meaningSoon = null;
el.allowWhat?.addEventListener("input", () => {
  clearTimeout(meaningSoon);
  meaningSoon = setTimeout(sayWhatItWouldAllow, 200);
});

async function drawGranted() {
  const a = whose();
  if (!a) return;
  el.asks.value = a.asks || "auto";
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
          // nothing about the line says which of the two it is. The words say
          // what it is for, so the tool's own name is only in the tooltip.
          what.textContent = one.covers;
          what.title = one.rule ? `${one.tool}: ${one.rule}` : one.tool;
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
  await drawAllowing(a);
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
  // What is worth allowing depends on how much it asks: one that never asks
  // has nothing to allow but somewhere else to write.
  await drawAllowing(a);
});

el.new.addEventListener("click", () => openNewTask());

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
    tellHere(`Saved to ${onto}`);
  }, !!showing);
  add("New task for this teammate", a?.name || "", () => alsoAsk(), !!a);
  add("New agent", "", () => start());
  add("Search everything", "", () => el.find.focus());
  add(a?.pinned ? "Unpin this agent" : "Pin this agent", "", () => setPinned(a, !a.pinned), !!a);
  add(a?.hidden ? "Show this agent" : "Hide this agent", "", () => setHidden(a, !a.hidden), !!a);
  add("What it may do without asking", "", () => el.granted.click(), !!a);
  add("What this thread can reach", "MCP servers", () => el.reach.click(), !!showing);
  add("Make this run on a schedule", "", () => el.repeat.click(), !!showing);
  add("Which models show up", "", () => showModels(), true);
  add("What Errand is", `the whole thing, in ${WHAT_THIS_IS.length} lines`, () => showTheTour(), true);
  add("What changed in this one", "since the version before it", () => whatChanged(), true);
  add("What it has cost", "today and this month", () => whatItCost(), true);
  add("Everything that runs on its own", "every agent's routines and watches", () => whatRunsOnItsOwn(), true);
  add("Check this setup", "what is wrong, and what to do", () => checkup());
  add("Mission Control", "every task and every team", () => showMission(), true);
  add("New task", "for a team or one teammate", () => openNewTask(), true);
  add("Teams", "who leads, and who they hand work to", () => showTeams());
  add(
    whose()?.paused ? "Start this agent again" : "Pause this agent",
    "nothing runs on its own until you start it again",
    () => setPaused(whose(), !whose().paused),
    !!whose(),
  );
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
    () => stopTheRun(showing),
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
  // While a new task is being given, it has the keyboard: nothing behind it
  // opens, closes or acts on a key meant for it.
  if (!el.newTask.hidden) {
    if (e.metaKey && ["1", "n", "f", "k"].includes(e.key.toLowerCase())) e.preventDefault();
    return;
  }
  // Mission Control, and a new task, from anywhere. Command only: Control-N
  // is the next line in every text box on a Mac.
  if (e.metaKey && e.key === "1") {
    e.preventDefault();
    el.mission.hidden ? showMission() : closeMission();
    return;
  }
  if (e.metaKey && e.key.toLowerCase() === "n") {
    e.preventDefault();
    // Over the palette or the finder, which would otherwise go on taking
    // the keys the new task is typed with.
    el.palette.hidden = true;
    el.finding.hidden = true;
    openNewTask();
    return;
  }
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
      // Where the fix is done, when that is a pane of System Settings. Opened
      // for them rather than described: "Notifications, then Errand" is four
      // levels down a screen most people have never opened.
      if (f.settings) {
        const open = document.createElement("button");
        open.type = "button";
        open.className = "finding-open";
        open.textContent = "Open System Settings";
        open.addEventListener("click", () =>
          invoke("open_settings", { pane: f.settings }).catch((why) => complain(String(why))),
        );
        box.append(open);
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
        ? `${going.length} running, ${waiting} ${waiting === 1 ? "needs" : "need"} you.`
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
      const task = tasksNow.find((x) => x.id === one.conversation);
      where.textContent = task ? titleOf(task) : one.talk;
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
  askedForAnAnswer(talking());
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
 * Read one answer out, or stop reading it.
 *
 * Grok Bot answers with voice memos; a call here already read every answer out
 * while it lasted. This is the same voice and the same reading, for one answer
 * somebody asked to hear. All of it, rather than a call's mouthful: somebody
 * who asks to hear an answer has asked for the whole answer. A call owns the
 * voice while it lasts, so this does nothing during one.
 */
function readAloud(m) {
  const was = listeningTo;
  stopReadingAloud();
  if (was !== m && !inACall) {
    const saying = worthSaying(m.text);
    if (saying) {
      const utterance = new SpeechSynthesisUtterance(saying);
      utterance.lang = navigator.language || "en-US";
      const over = () => {
        if (listeningTo !== m) return;
        listeningTo = null;
        drawMessages();
      };
      utterance.onend = over;
      utterance.onerror = over;
      listeningTo = m;
      Speaking.speak(utterance);
    }
  }
  drawMessages();
}

/** Stop an answer being read aloud, when one is. */
function stopReadingAloud() {
  if (!listeningTo) return;
  listeningTo = null;
  Speaking.cancel();
}

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
  onlyThisOneOpen(el.watching);
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
      "Name a folder, a file or a web address, or type mail or calendar, choose how often " +
      "to look, and say what this agent should do then.";
    return;
  }
  if (!at) {
    el.watchPlain.textContent = "Name a folder, a file or a web address to watch, or type mail or calendar.";
    return;
  }
  const then = what
    ? `it will ask this agent to ${what.replace(/^(please\s+)?/i, "")}.`
    : "it will wake this agent. Say what it should do, above.";
  const kind = whatKindOfWatch(at);
  if (kind === "mail") {
    el.watchPlain.textContent =
      `It will count your unread mail ${howOftenInWords()}, while Errand is running, window or no ` +
      `window, and only while Mail is open. If there is more of it, ${then}`;
    return;
  }
  if (kind === "calendar") {
    el.watchPlain.textContent =
      `It will look at your calendars ${howOftenInWords()}, while Errand is running, window or no ` +
      `window. At least ${howLongBefore(at)} before each event, ${then}`;
    return;
  }
  const page = kind === "page";
  const looking = page ? `read ${at}` : `look at ${at}`;
  const changed = page ? "If the page has changed" : "If anything there has changed";
  el.watchPlain.textContent =
    `It will ${looking} ${howOftenInWords()}, while Errand is running, window or no window. ${changed}, ${then}`;
}

/**
 * Which kind of thing is being named, as far as saying it back goes.
 *
 * Only for the sentence. What is saved is read by the app, which refuses
 * anything it cannot read and says why.
 */
function whatKindOfWatch(at) {
  const words = at.toLowerCase().split(/\s+/).filter(Boolean);
  const named = words[0] === "my" ? words.slice(1) : words;
  if (["mail", "new mail", "email", "inbox", "inboxes"].includes(named.join(" "))) return "mail";
  if (["calendar", "calendars", "diary"].includes(named[0])) return "calendar";
  return /^https?:\/\//i.test(at) ? "page" : "path";
}

/** How long before each event a calendar watch wakes somebody, in words. */
function howLongBefore(at) {
  const said = at.match(/(\d+)\s*([mhd])\b/i);
  if (!said) return "15 minutes";
  const unit = { m: "minute", h: "hour", d: "day" }[said[2].toLowerCase()];
  return `${said[1]} ${unit}${said[1] === "1" ? "" : "s"}`;
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
  onlyThisOneOpen(el.aiming);
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
  const asked = showing;
  let now;
  try {
    now = await invoke("goal_of", { id: asked });
  } catch (why) {
    el.goalSays.textContent = String(why);
    return;
  }
  if (!leaveTheField) el.goalWhat.value = now.goal || "";
  // The task it was asked about, which is not always the one on screen by the
  // time the answer comes.
  const here = talks.get(asked);
  if (here) {
    here.aim = now.goal && !now.over ? now : null;
    if (showing === asked) drawTaskCard();
  }
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
    why: "Checked: this is where it answers. With your key in, it lists its models; kimi-k3 is the flagship, with a 1M-token window.",
  },
  {
    name: "GLM",
    url: "https://api.z.ai/api/paas/v4",
    sure: true,
    why: "Checked: this is where it answers. Not /v1, which is not served at all. With your key in, it lists its models; GLM-5.3 is the flagship.",
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
  drawEngines();
  drawLocalModel();
  drawNotifications();
  // What the switch last said is about a change made then, and the switch is
  // read again now; left up, it could say the opposite of what it shows.
  el.sshKeyAtStartSays.textContent = "";
  drawSshKey();
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
  await Promise.all([drawChosen(), drawKept(), drawAtLogin(), drawTeammateLogins(), drawReachable()]);
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

/**
 * Teammates' apps that start at every login, each with a way to stop it. Read
 * from the files the system obeys, every time, like the switch above.
 */
async function drawTeammateLogins() {
  let theirs = [];
  try {
    theirs = (await invoke("teammates_at_login")) || [];
  } catch {
    theirs = [];
  }
  el.teammateLogins.hidden = !theirs.length;
  el.teammateLoginsList.replaceChildren(
    ...theirs.map(([label, opens]) => {
      const li = document.createElement("li");
      const name = String(opens).split("/").filter(Boolean).pop() || label;
      const what = note("span", name, "name");
      what.title = opens;
      const stop = document.createElement("button");
      stop.type = "button";
      stop.textContent = "Stop starting it at login";
      stop.onclick = async () => {
        try {
          await invoke("stop_teammate_at_login", { label });
        } catch (why) {
          stop.textContent = String(why);
          return;
        }
        drawTeammateLogins();
      };
      li.append(what, stop);
      return li;
    }),
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

/* ------------------------------------------------------------ overview -- */

async function readTheSettings() {
  errandModel = (await invoke("setting", { key: "errand_model" }).catch(() => null)) || null;
  localModel = (await invoke("setting", { key: "local_model" }).catch(() => null)) || null;
}

/**
 * Where the words of the teammate on screen go: on this network, out to
 * somebody else's servers, or nowhere, for one kept local with nothing local
 * to run on. Asked of the app, which is what decides it.
 */
async function drawWordsGo() {
  const a = whose();
  if (!a) {
    el.wordsGo.hidden = true;
    return;
  }
  let where;
  try {
    where = await invoke("where_words_go", { id: a.id });
  } catch {
    el.wordsGo.hidden = true;
    return;
  }
  // Somebody went on to another teammate while this was being asked.
  if (whose() !== a || !where) return;
  const kept = a.keepLocal ? "Kept local: " : "";
  // Whose choice the model is: its own, Errand's for everybody, or the local
  // one standing in because the other would send its words away.
  const why = {
    itself: " (its own)",
    errand: "",
    local: " (the Local model, since it is kept local)",
    before: "",
  }[where.by] ?? "";
  a.runsOn = where.engine || a.on;
  if (where.refused) {
    el.wordsGo.dataset.state = "refused";
    el.wordsGo.textContent =
      "Kept local, and there is nothing local to run it on. Choose a local model for it under Who this is, or a Local model in Settings.";
    el.wordsGo.title = where.refused;
  } else if (where.stays) {
    el.wordsGo.dataset.state = "here";
    el.wordsGo.textContent = `${kept}its words stay on your network \u00b7 ${where.model}${why}`;
    el.wordsGo.title = "What it is told, and everything it reads for you, stays on this Mac and your network.";
  } else {
    el.wordsGo.dataset.state = "away";
    el.wordsGo.textContent = `Its words leave your network \u00b7 ${where.model}${why}`;
    el.wordsGo.title =
      "What it is told, and everything it reads for you, goes to this model's servers. Keep it local under Who this is.";
  }
  el.wordsGo.hidden = false;
}

/**
 * The model the teammate on screen works on: Errand's, unless it has one of
 * its own. The same answering models Settings offers, and for one kept local
 * only those served here, because the app refuses the rest for it anyway.
 */
async function drawOwnModel() {
  const a = whose();
  if (!a) return;
  const choices = await whatCouldAnswer().catch(() => []);
  // Errand's model as it is now, asked rather than remembered: it may have
  // been chosen since this window last read it.
  const errandNow = (await invoke("setting", { key: "errand_model" }).catch(() => null)) || errandModel;
  if (whose() !== a) return;
  const mine = a.ownModel;
  const errands = choices.find((c) => c.id === errandNow);
  const follow = document.createElement("option");
  follow.value = "";
  follow.textContent = errands ? `Errand's model \u00b7 ${errands.name}` : "Errand's model";
  follow.selected = !mine;
  const offered = choices
    .filter((c) => c.id === mine || answersLately(c.id))
    .filter((c) => c.id === mine || !a.keepLocal || c.here);
  const own = offered.map((c) => {
    const option = document.createElement("option");
    option.value = c.id;
    option.textContent =
      c.id === mine && answeredLast.get(c.id) === false ? `${c.name} \u00b7 not answering` : c.name;
    option.selected = c.id === mine;
    return option;
  });
  el.whoisModel.replaceChildren(follow, ...own);
  if (mine && !choices.some((c) => c.id === mine)) {
    const gone = document.createElement("option");
    gone.value = mine;
    gone.selected = true;
    gone.textContent = "Its model \u00b7 not in the list";
    el.whoisModel.append(gone);
  }
}

el.whoisModel.addEventListener("change", async () => {
  const a = whose();
  if (!a) return;
  const now = el.whoisModel.value || null;
  try {
    await invoke("own_model", { id: a.id, model: now });
    a.ownModel = now;
  } catch (why) {
    complain(String(why));
  }
  drawOwnModel();
  drawWordsGo();
  drawLimit();
});

el.whoisLocal.addEventListener("change", async () => {
  const a = whose();
  if (!a) return;
  const on = el.whoisLocal.checked;
  try {
    const letGo = await invoke("keep_local", { id: a.id, on });
    a.keepLocal = on;
    // A model of its own that would send its words away, let go of.
    if (letGo) {
      a.ownModel = null;
      tellHere(letGo);
    }
  } catch (why) {
    el.whoisLocal.checked = !on;
    complain(String(why));
  }
  drawWordsGo();
  drawOwnModel();
});

/** Whether macOS lets Errand say when an errand finishes, and the way to its switch. */
let notificationsPane = null;
/**
 * Whether macOS lets Errand notify, as last asked: at opening, and each time
 * Settings is. An old note saying they were off is history once they are on.
 */
let notifyingNow = null;
async function drawNotifications() {
  let now;
  try {
    now = await invoke("notifications");
  } catch {
    return;
  }
  if (!now) return;
  notificationsPane = now.settings;
  notifyingNow = now.state;
  const says = {
    allowed: "On. Errand says when an errand finishes or needs you, and puts a count on its icon in the Dock.",
    refused: "Off in macOS, so errands finish quietly. Open Errand's notification settings and switch on Allow notifications.",
    "not-answered": "macOS has not asked yet. Open Errand's notification settings to switch them on.",
    unknown: "macOS did not say whether they are on. Open Errand's notification settings to see.",
  };
  el.notificationsSays.textContent = says[now.state] || says.unknown;
  el.notificationsSays.dataset.state = now.state;
}

el.notificationsOpen.addEventListener("click", () => {
  if (!notificationsPane) return;
  invoke("open_settings", { pane: notificationsPane }).catch((why) => complain(String(why)));
});

/* ------------------------------------------------------------- SSH key -- */

/** Names, the way a sentence lists them. */
function inASentence(names) {
  if (names.length < 2) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/**
 * Whether teammates can use SSH, as the key agent has it now: asked of the app
 * every time Settings opens, because the agent forgets at every restart and
 * anything in a terminal can change it in between.
 */
async function drawSshKey() {
  let now;
  try {
    now = await invoke("ssh_key");
  } catch (why) {
    sayAboutTheKey(String(why), true);
    return null;
  }
  el.sshKeyAtStart.checked = !!now.at_start;
  el.sshKeyLoad.disabled = !now.agent || !now.keys.length;
  if (!now.agent) {
    sayAboutTheKey(
      "The key agent cannot be reached from Errand. Quitting Errand and opening it again usually puts that right.",
      true,
    );
  } else if (now.holds > 0) {
    sayAboutTheKey(
      `Loaded. The key agent holds ${now.holds === 1 ? "a key" : `${now.holds} keys`}, so teammates can use SSH.`,
      false,
    );
  } else if (!now.keys.length) {
    sayAboutTheKey(
      "There is no SSH key in ~/.ssh to load. If yours is kept somewhere else, name it with IdentityFile in ~/.ssh/config.",
      true,
    );
  } else {
    sayAboutTheKey(
      `Not loaded. The key agent is empty, so a teammate's SSH is refused until ${inASentence(now.keys)} ${now.keys.length === 1 ? "is" : "are"} loaded.`,
      true,
    );
  }
  return now;
}

function sayAboutTheKey(words, wrong) {
  el.sshKeySays.textContent = words;
  el.sshKeySays.dataset.wrong = String(!!wrong);
}

/**
 * Load it, from whichever button was pressed, and say what came of it there.
 * The passphrase, where there is one, is asked for by macOS in a window of its
 * own and goes to ssh-add; nothing here ever has it.
 */
async function loadTheKey(button, say) {
  const was = button.textContent;
  button.disabled = true;
  button.textContent = "Loading…";
  say("If your key has a passphrase, macOS asks for it in a window of its own.", false);
  let loaded;
  try {
    loaded = await invoke("load_ssh_key");
  } catch (why) {
    say(String(why), true);
    return null;
  } finally {
    button.disabled = false;
    button.textContent = was;
  }
  if (loaded.why_not) {
    say(loaded.why_not, true);
    return null;
  }
  say(`Loaded ${inASentence(loaded.added)}. Teammates can use SSH with it now.`, false);
  return loaded;
}

el.sshKeyLoad.addEventListener("click", () => loadTheKey(el.sshKeyLoad, sayAboutTheKey));

el.sshKeyAtStart.addEventListener("change", async () => {
  const wanted = el.sshKeyAtStart.checked;
  const say = (words, wrong) => {
    el.sshKeyAtStartSays.textContent = words;
    el.sshKeyAtStartSays.dataset.wrong = String(!!wrong);
  };
  try {
    await invoke("set_setting", { key: "ssh_keys_at_start", value: wanted ? "on" : "off" });
  } catch (why) {
    el.sshKeyAtStart.checked = !wanted;
    say(String(why), true);
    return;
  }
  if (!wanted) {
    say("Errand loads your key only when you press Load my SSH key, and the agent keeps it until the Mac restarts.", false);
    return;
  }
  say(
    "Errand loads your key into the key agent whenever it starts. A key with a passphrase is loaded from your Keychain once you have loaded it here.",
    false,
  );
  // Teammates may use it, so they can now rather than after a restart.
  const now = await drawSshKey();
  if (now && now.agent && now.holds === 0 && now.keys.length) {
    await loadTheKey(el.sshKeyLoad, sayAboutTheKey);
  }
});

/** What the note above the box says before anything is pressed. */
const KEY_NOTE_SAYS =
  "Teammates use your key through the agent and never read it, so nothing under Allowed fixes this. Load it, then ask again.";

/** When "Not now" was last pressed: a teammate trying again is not asked about twice in ten minutes. */
let keyNoteDismissed = 0;

listen("ssh_key_needed", () => {
  if (Date.now() - keyNoteDismissed < 10 * 60 * 1000) return;
  if (!el.keyNote.hidden && el.keyNoteLoad.hidden) return;
  el.keyNoteSays.textContent = KEY_NOTE_SAYS;
  el.keyNoteSays.dataset.wrong = "false";
  el.keyNoteLoad.hidden = false;
  el.keyNoteClose.textContent = "Not now";
  el.keyNote.hidden = false;
});

el.keyNoteLoad.addEventListener("click", async () => {
  const say = (words, wrong) => {
    el.keyNoteSays.textContent = words;
    el.keyNoteSays.dataset.wrong = String(!!wrong);
  };
  const loaded = await loadTheKey(el.keyNoteLoad, say);
  if (!loaded) return;
  el.keyNoteLoad.hidden = true;
  el.keyNoteClose.textContent = "Done";
  const atStart = (await invoke("setting", { key: "ssh_keys_at_start" }).catch(() => null)) === "on";
  say(
    `Loaded ${inASentence(loaded.added)}. Ask again and SSH will work.` +
      (atStart ? "" : " To have Errand load it whenever it starts, turn that on under Settings, SSH key."),
    false,
  );
});

el.keyNoteClose.addEventListener("click", () => {
  if (!el.keyNoteLoad.hidden) keyNoteDismissed = Date.now();
  el.keyNote.hidden = true;
});

/** The models teammates kept local may run on: only those served here. */
async function drawLocalModel() {
  const here = (await whatCouldAnswer()).filter((c) => c.here);
  const none = document.createElement("option");
  none.value = "";
  none.textContent = here.length ? "Not chosen" : "None on this Mac or your network yet";
  el.localModel.replaceChildren(
    none,
    ...here.map((c) => {
      const option = document.createElement("option");
      option.value = c.id;
      option.textContent = c.name;
      return option;
    }),
  );
  el.localModel.value = here.some((c) => c.id === localModel) ? localModel : "";
}

el.localModel.addEventListener("change", async () => {
  const id = el.localModel.value;
  if (!id) return;
  const choice = (await whatCouldAnswer()).find((c) => c.id === id);
  try {
    await invoke("set_setting", { key: "local_model", value: id });
    localModel = id;
    el.localModelSays.textContent = `Teammates kept local now run on ${choice?.name || "it"} whenever Errand's model would send their words elsewhere.`;
  } catch (why) {
    el.localModelSays.textContent = String(why);
    drawLocalModel();
  }
  drawWordsGo();
});

/**
 * Say a task is finished, or that it is not after all, here and in the app.
 *
 * Finished is somebody's word for it, never the app's guess: a task that was
 * answered may still be waiting for them to read it, and a routine that ran
 * is not finished at all. A teammate is never finished; its tasks are.
 */
async function markTaskFinished(t, finished) {
  const at = finished ? Date.now() : null;
  const set = (when) => {
    t.finished = when;
    const held = talks.get(t.id);
    if (held) held.finished = when;
    for (const one of tasksNow) if (one.id === t.id) one.finished = when;
    drawTalks();
    drawThreads();
    if (!el.overview.hidden) drawOverview();
  };
  const was = t.finished;
  set(at);
  try {
    await invoke("finish_task", { id: t.id, finished });
  } catch (why) {
    set(was);
    complain(String(why));
    return;
  }
  if (finished && showing === t.id) await offerToKeep(t);
  else if (!finished && showing === t.id) el.taskLearn.hidden = true;
}

/**
 * Finished: is there anything for the teammate to keep from it? How it was
 * done, as a skill it can do again by name, and something to remember. Asked
 * here, once, because this is the moment somebody knows whether it went the
 * way they wanted; nothing is kept unless they say so.
 */
async function offerToKeep(t) {
  const a = agents.get(t.agent);
  if (!a) return;
  let could = null;
  try {
    could = await invoke("could_keep", { conversation: t.id });
  } catch {
    could = null;
  }
  if (showing !== t.id) return;
  el.taskLearn.dataset.task = t.id;
  el.learnAsks.textContent = `Finished. Anything for ${a.name} to keep from it?`;
  el.learnSkill.hidden = !could;
  if (could) {
    const [, steps] = could;
    el.learnSkillName.value = t.name && t.name !== "First" && t.name !== "New task" ? t.name : "";
    el.learnSkillSays.textContent = `How it was done, ${steps} ${steps === 1 ? "step" : "steps"}, as a skill to do again by name`;
  }
  el.learnNoteText.value = "";
  el.learnSays.textContent = "";
  el.taskLearn.hidden = false;
}

el.learnSkill.addEventListener("submit", async (e) => {
  e.preventDefault();
  const id = el.taskLearn.dataset.task;
  const name = el.learnSkillName.value.trim();
  if (!id || !name) return;
  try {
    el.learnSays.textContent = await invoke("keep_as_skill", { conversation: id, name });
    el.learnSkill.hidden = true;
  } catch (why) {
    el.learnSays.textContent = String(why);
  }
});
el.learnNote.addEventListener("submit", async (e) => {
  e.preventDefault();
  const t = talks.get(el.taskLearn.dataset.task);
  const text = el.learnNoteText.value.trim();
  if (!t || !text) return;
  // About its first few words: a note needs something to be about, and the
  // person reads it back under What it remembers.
  const about = text.split(/\s+/).slice(0, 4).join(" ");
  try {
    await invoke("note_down", { agent: t.agent, about, note: text });
    el.learnNoteText.value = "";
    el.learnSays.textContent = `Remembered: ${text}`;
  } catch (why) {
    el.learnSays.textContent = String(why);
  }
});
el.learnDone.addEventListener("click", () => {
  el.taskLearn.hidden = true;
});

/** Every task, read again, and the list down the side drawn with it. */
async function readTasks() {
  try {
    tasksNow = ((await invoke("tasks")) || []).map(asTask);
  } catch {
    return;
  }
  drawThreads();
}

/** One task, as the overview holds it. */
function asTask(c) {
  return {
    id: c.id,
    agent: c.agent,
    name: c.name,
    first: c.first || "",
    // Whether anything was said in it: a teammate's first task is there from
    // the moment the teammate is, and is not a piece of work until asked.
    said: c.said !== false,
    priority: c.priority || 2,
    finished: c.finished_at || null,
    spoke: c.spoke_at || 0,
    // The task that asked for this one, when another teammate did.
    askedBy: c.asked_by || null,
  };
}

/**
 * What a task is called: its name, or what was asked in it when the name is
 * one the app made up rather than one somebody chose.
 */
function titleOf(t) {
  if (!aMadeUpName(t.name)) return t.name;
  // What was asked in it, said as a name: three tasks another teammate started
  // were all "Asked by" it, and every first task was "First". A standing job
  // set up as "Set yourself a standing job: ..." is called by what it does.
  const job = t.id ? standingNow.find((s) => s.conversation === t.id && s.what)?.what : "";
  const asked = aNameFrom(t.first, job);
  if (asked) return asked;
  // Nothing asked yet: its own name, unless that is "First", which says
  // nothing on its own. A new task called "First task" was the fifth.
  return t.name && t.name !== "First" ? t.name : "First task";
}

/**
 * Whether an agent has anything that runs by itself, or is running now, said
 * on its mark where a list of forty is skimmed: a badge in the corner, the way
 * an app says it has something. Filled in the accent while a routine or a
 * watch is live, with a play sign while the agent works, and only outlined
 * when everything it has is switched off, paused or stopped. When and what
 * are in its tooltip.
 *
 * Before this, the only sign was a grey mark the size of a letter beside the
 * name, and the difference between a teammate that would check a disk every
 * hour and one that never would again was the opacity of that mark. A routine
 * switched off by mistake went four hours without anybody seeing it.
 *
 * On the mark rather than at the end of the name, because a sidebar is narrow
 * and a pill there cut "Bitcoin Desk" down to "Bitcoin".
 */
function onItsOwn(a) {
  const theirs = standingNow.filter((s) => s.agent === a.id);
  const working = busy(a.id);
  if (!theirs.length && !working) return [];
  const live = theirs.filter((s) => !s.paused && (s.kind === "routine" ? !s.off : !s.stopped));
  const badge = document.createElement("span");
  badge.className = "on-its-own";
  let shows;
  let said;
  if (working) {
    badge.classList.add("now");
    shows = "running";
    said = "Running now";
  } else if (live.length) {
    shows = live.some((s) => s.kind === "routine") ? "repeat" : "watch";
    const next = live.filter((s) => s.due).sort((x, y) => x.due - y.due)[0];
    said = next ? `Runs on its own, next ${whenNext(next.due)}` : "Watching on its own";
  } else {
    badge.classList.add("idle");
    shows = theirs.some((s) => s.kind === "routine") ? "repeat" : "watch";
    // Why nothing will run, by the reason that covers all of it: the teammate
    // paused, then a watch stopped, then a routine switched off.
    said =
      a.paused || theirs.every((s) => s.paused)
        ? "Paused: nothing runs on its own"
        : theirs.every((s) => s.kind === "watch")
          ? "Stopped: nothing runs on its own"
          : "Switched off: nothing runs on its own";
  }
  badge.dataset.shows = shows;
  const details = repeatMarks(a).map((m) => m.title);
  badge.title = [said, ...details].join("\n");
  badge.setAttribute("role", "img");
  badge.setAttribute("aria-label", said);
  badge.innerHTML = BADGE_ICONS[shows];
  return [badge];
}

/** What the badge shows, drawn at the size of the badge. */
const BADGE_ICONS = {
  repeat:
    '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M12.6 5.4A5 5 0 0 0 3.3 6.5M3.4 10.6a5 5 0 0 0 9.3-1.1" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><path d="M12.9 2.6v3h-3M3.1 13.4v-3h3" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  watch:
    '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8s-2.4 4.5-6.5 4.5S1.5 8 1.5 8Z" fill="none" stroke="currentColor" stroke-width="1.8"/><circle cx="8" cy="8" r="2.2" fill="currentColor"/></svg>',
  running: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3.2v9.6L13 8z" fill="currentColor"/></svg>',
};

/** Read what repeats again, and redraw what shows it if any of it changed. */
async function readStanding() {
  let now;
  try {
    now = (await invokeTheApp("standing")) || [];
  } catch {
    return;
  }
  // Every minute, so only when something is different: redrawing the list
  // under somebody's pointer for nothing is how a list starts to flicker.
  if (JSON.stringify(now) === JSON.stringify(standingNow)) return;
  standingNow = now;
  overviewKnows.standing = standingNow;
  drawThreads();
  drawTaskCard();
  if (!el.overview.hidden) drawOverview();
}
// And now and then regardless, for a watch the app stopped on its own, which
// says so in its conversation rather than by any event of this kind.
setInterval(readStanding, 60_000);

/**
 * The marks for what an agent does on its own: one for routines and one for
 * watches, each saying in its tooltip when and what.
 */
function repeatMarks(a, conversation) {
  const theirs = standingNow.filter(
    (s) => s.agent === a.id && (!conversation || s.conversation === conversation),
  );
  const routines = theirs.filter((s) => s.kind === "routine");
  const watches = theirs.filter((s) => s.kind === "watch");
  const marks = [];
  if (routines.length) {
    marks.push(
      aMark(
        "repeat",
        routines.every((r) => r.off || r.paused),
        routines
          .map((r) => {
            const when = r.off
              ? "switched off"
              : r.paused
                ? "paused"
                : r.due
                  ? `next ${whenNext(r.due)}`
                  : "";
            return `Repeats ${r.at}${when ? ` (${when})` : ""}: ${r.what}`;
          })
          .join("\n"),
      ),
    );
  }
  if (watches.length) {
    marks.push(
      aMark(
        "watch",
        watches.every((w) => w.stopped || w.paused),
        watches
          .map((w) => `Watches ${w.at}${w.stopped ? " (stopped)" : ""}: ${w.what}`)
          .join("\n"),
      ),
    );
  }
  return marks;
}

/** One mark: a small picture, dimmed when nothing of it will run, and its tooltip. */
function aMark(kind, idle, says) {
  const mark = document.createElement("span");
  mark.className = `${kind}-mark`;
  if (idle) mark.classList.add("idle");
  mark.title = says;
  mark.setAttribute("role", "img");
  mark.setAttribute("aria-label", says);
  mark.innerHTML =
    kind === "repeat"
      ? '<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M12.6 5.4A5 5 0 0 0 3.3 6.5M3.4 10.6a5 5 0 0 0 9.3-1.1" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M12.9 2.6v3h-3M3.1 13.4v-3h3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>'
      : '<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8s-2.4 4.5-6.5 4.5S1.5 8 1.5 8Z" fill="none" stroke="currentColor" stroke-width="1.4"/><circle cx="8" cy="8" r="2" fill="currentColor"/></svg>';
  return mark;
}

/** What the overview last read from the app: what is running, and what repeats. */
let overviewKnows = { running: [], standing: [] };
let overviewTicking = null;

/** When the overview was last looked at, so "while you were away" means something. */
function lastLookedAt() {
  try {
    const at = Number(localStorage.getItem("errand-overview-looked"));
    return at > 0 ? at : null;
  } catch {
    return null;
  }
}

function lookedNow() {
  try {
    localStorage.setItem("errand-overview-looked", String(Date.now()));
  } catch {
    // A private window keeps no such thing, and the last day is shown instead.
  }
}

async function showOverview({ focus = true } = {}) {
  // How it was last grouped and ordered, which is somebody's habit rather
  // than anything the app needs to know.
  try {
    el.overviewGroup.value = localStorage.getItem("errand-overview-group") || "state";
    el.overviewOrder.value = localStorage.getItem("errand-overview-order") || "priority";
    el.overviewShow.value = localStorage.getItem("errand-overview-show") || "all";
  } catch {
    // The first option of each stands.
  }
  // One kept from before a choice was taken away, like grouping by subject,
  // is no choice at all: the first one stands instead.
  for (const [select, first] of [
    [el.overviewGroup, "state"],
    [el.overviewOrder, "priority"],
    [el.overviewShow, "all"],
  ]) {
    if (!select.value) select.value = first;
  }
  cameFrom();
  el.mission.hidden = false;
  el.teams.hidden = true;
  tabShown("tasks");
  el.overview.hidden = false;
  // The keys go to Mission Control, not to the conversation hidden under it.
  // Not at launch, where nothing had them and a ring round a tab is noise.
  if (focus) el.missionTasks.focus();
  awaySeen = false;
  await readTheOverview();
  drawOverview();
  await drawAway();
  if (!el.overview.hidden && !el.mission.hidden) awaySeen = true;
  // Live while it is open: a turn ending or a routine firing behind it would
  // otherwise leave it describing a state that is over.
  clearInterval(overviewTicking);
  overviewTicking = setInterval(async () => {
    if (el.overview.hidden) {
      clearInterval(overviewTicking);
      return;
    }
    await readTheOverview();
    drawOverview();
  }, 10_000);
}

/** Leave the Tasks tab: it stops reading itself again. */
function leaveTasks() {
  el.overview.hidden = true;
  clearInterval(overviewTicking);
}

/** Whether "Since you last looked" was on screen in this opening of Mission Control. */
let awaySeen = false;
/** What had the keyboard before Mission Control opened, to have it back after. */
let focusBeforeMission = null;

/** Where focus was, the first time Mission Control opens over it. */
function cameFrom() {
  if (el.mission.hidden) focusBeforeMission = document.activeElement;
}

/**
 * Close Mission Control, whichever tab was open. What it said had happened
 * since you last looked counts as seen only if it was shown, and focus goes
 * back where it was, or to the box to type in.
 */
function closeMission({ refocus = true } = {}) {
  if (el.mission.hidden) return;
  if (!el.overview.hidden) leaveTasks();
  if (awaySeen) lookedNow();
  awaySeen = false;
  el.teams.hidden = true;
  el.mission.hidden = true;
  if (!refocus) return;
  const back = focusBeforeMission;
  focusBeforeMission = null;
  if (back?.isConnected && back.offsetParent !== null && back !== document.body) back.focus();
  else el.what.focus();
}

/** Everything that left the Tasks or Teams tab for somewhere else leaves Mission Control. */
function closeOverview() {
  closeMission();
}

/** Mark which tab is shown, and remember it for the next time Mission Control opens. */
function tabShown(which) {
  el.missionTasks.setAttribute("aria-selected", String(which === "tasks"));
  el.missionTeams.setAttribute("aria-selected", String(which === "teams"));
  try {
    localStorage.setItem("errand-mission-tab", which);
  } catch {
    // Opens on Tasks next time instead.
  }
}

/** Mission Control on the tab it was last left on, Tasks the first time. */
function showMission({ focus = true } = {}) {
  let last = "tasks";
  try {
    last = localStorage.getItem("errand-mission-tab") || "tasks";
  } catch {
    // Tasks.
  }
  return last === "teams" ? showTeams({ focus }) : showOverview({ focus });
}

async function readTheOverview() {
  const [running, standing, tasks] = await Promise.all([
    invoke("whats_running").catch(() => []),
    invoke("standing").catch(() => []),
    invoke("tasks").catch(() => null),
  ]);
  overviewKnows = { running: running || [], standing: standing || [] };
  standingNow = overviewKnows.standing;
  if (Array.isArray(tasks)) tasksNow = tasks.map(asTask);
}

/**
 * What the overview's search found: which jobs, and the first line in each
 * where the words are. Nothing while nothing is being searched for.
 */
let overviewFound = null;
let overviewSearching = null;

el.overviewFind.addEventListener("input", () => {
  clearTimeout(overviewSearching);
  overviewSearching = setTimeout(searchTheOverview, 160);
});

/**
 * Search every job and everything said in it, the same way the list down the
 * side does, and what each repeats as well: a routine's instructions are part
 * of the job even before it has run and said them anywhere.
 */
async function searchTheOverview() {
  const lookingFor = el.overviewFind.value.trim();
  if (!lookingFor) {
    overviewFound = null;
    drawOverview();
    return;
  }
  const [found, where] = await Promise.all([
    invoke("matching", { lookingFor }).catch(() => []),
    invoke("hits", { lookingFor }).catch(() => []),
  ]);
  // Tasks: the ones the words are in, every task of a teammate whose name or
  // job has them, and one whose routine or watch says them.
  const hits = new Map();
  for (const hit of where || []) if (!hits.has(hit.conversation)) hits.set(hit.conversation, hit);
  const ids = new Set(hits.keys());
  const teammates = new Set((found || []).map((a) => a.id));
  const lower = lookingFor.toLowerCase();
  for (const t of tasksNow) {
    if (teammates.has(t.agent) || `${t.name} ${t.first}`.toLowerCase().includes(lower)) ids.add(t.id);
  }
  for (const s of standingNow) {
    if (`${s.at} ${s.what}`.toLowerCase().includes(lower)) ids.add(s.conversation);
  }
  for (const a of found || []) if (!agents.has(a.id)) agents.set(a.id, asAgent(a));
  overviewFound = { ids, hits };
  drawOverview();
}

/* ------------------------------------------------------------- teams -- */

/**
 * Teams: a lead and the teammates it hands work to.
 *
 * The one place a crew is put together and the one place it can be seen.
 * Before it, the only way to know who a lead would ask was to watch it ask,
 * and every teammate could ask every other, so "the crew" was a word in a
 * job description rather than anything the app knew.
 */
let teamsKnown = [];
/** What each teammate brings, by id: its skills by name, and its checklist's length. */
let bringsNow = new Map();

async function showTeams({ focus = true } = {}) {
  if (!el.overview.hidden) leaveTasks();
  cameFrom();
  el.mission.hidden = false;
  tabShown("teams");
  el.teams.hidden = false;
  if (focus) el.missionTeams.focus();
  await drawTeams();
}

/** Everything that left the Teams tab for somewhere else leaves Mission Control. */
function closeTeams() {
  closeMission();
}

/** The teammates a team can be made of: named ones, in the order of the list. */
function teammatesToChoose() {
  return [...agents.values()].filter((a) => a.name && a.name !== NOT_YET_NAMED);
}

/** One teammate as a team shows it: mark, name, role, and its job on one line. */
function personRow(a, more) {
  const row = document.createElement("div");
  row.className = "person";
  row.dataset.agent = a.id;
  const who = document.createElement("div");
  who.className = "who";
  const name = document.createElement("span");
  name.className = "name";
  name.textContent = a.name;
  if (a.title) {
    const role = document.createElement("span");
    role.className = "role";
    role.textContent = a.title;
    name.append(role);
  }
  const does = document.createElement("span");
  does.className = "does";
  does.textContent = a.about || "Has not said what it handles";
  does.title = a.about || "";
  who.append(name, does);
  // What it brings: the skills it has been taught and how it checks its work.
  const brought = bringsNow.get(a.id);
  if (brought) {
    const said = [];
    // A model of its own, which is part of what it brings to a team.
    if (brought.own) said.push(`On ${brought.own}`);
    if (brought.skills.length) said.push(`Skills: ${brought.skills.join(", ")}`);
    if (brought.checks) said.push(`checks its work against ${brought.checks} ${brought.checks === 1 ? "point" : "points"}`);
    const brings = document.createElement("span");
    brings.className = said.length ? "brings" : "brings none";
    brings.textContent = said.length
      ? said.join(" \u00b7 ").replace(/^checks/, "Checks")
      : "No skills or checklist yet";
    who.append(brings);
  }
  row.append(tile(kindFor(a), busy(a.id), a.hue), who, more || document.createElement("span"));
  return row;
}

/** A select that reads as a quiet button, with a first line that says what it does. */
function chooser(label, options, onChoose) {
  const select = document.createElement("select");
  select.setAttribute("aria-label", label);
  const first = document.createElement("option");
  first.value = "";
  first.textContent = label;
  select.append(first);
  for (const [value, text] of options) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = text;
    select.append(option);
  }
  select.addEventListener("change", async () => {
    const value = select.value;
    if (!value) return;
    select.disabled = true;
    try {
      await onChoose(value);
    } finally {
      await drawTeams();
    }
  });
  return select;
}

async function drawTeams() {
  try {
    teamsKnown = (await invoke("teams")) || [];
  } catch {
    teamsKnown = [];
  }
  try {
    const brought = (await invoke("what_they_bring")) || [];
    bringsNow = new Map(
      brought.map(([id, skills, checks, own]) => [id, { skills: skills || [], checks: checks || 0, own: own || null }]),
    );
  } catch {
    bringsNow = new Map();
  }
  const everybody = teammatesToChoose();
  const byId = new Map([...agents.values()].map((a) => [a.id, a]));
  const named = (id) => byId.get(id);
  const label = (a) => (a.title ? `${a.name} (${a.title})` : a.name);

  const cards = teamsKnown.map((team) => {
    const card = document.createElement("article");
    card.className = "team";
    card.dataset.team = team.id;

    const header = document.createElement("header");
    const name = document.createElement("input");
    name.className = "team-name";
    name.type = "text";
    name.value = team.name;
    name.setAttribute("aria-label", "What the team is called");
    // What the card's buttons say with the team's name in it, said again
    // once a new name is kept. Set once the buttons are made.
    let retitle = () => {};
    // Kept on Enter and when the box is left, once: the name is taken as
    // kept before the app answers, so leaving the box after Enter asks nothing.
    const keep = async () => {
      const now = name.value.trim();
      const was = team.name;
      if (!now || now === was) {
        name.value = was;
        return;
      }
      team.name = now;
      try {
        await invoke("rename_team", { id: team.id, name: now });
      } catch {
        team.name = was;
        name.value = was;
      }
      retitle();
    };
    name.addEventListener("blur", keep);
    name.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        keep();
        name.blur();
      }
      if (e.key === "Escape") {
        name.value = team.name;
        name.blur();
        e.stopPropagation();
      }
    });
    const count = document.createElement("span");
    count.className = "team-count";
    const size = team.members.length + (team.lead ? 1 : 0);
    count.textContent = size === 1 ? "1 teammate" : `${size} teammates`;
    // Where the team keeps what it makes: every member writes there.
    const folder = document.createElement("button");
    folder.type = "button";
    folder.className = "team-folder";
    folder.textContent = "Team folder";
    folder.addEventListener("click", async () => {
      try {
        await invoke("show_team_folder", { id: team.id });
      } catch (why) {
        folder.title = String(why);
      }
    });
    header.append(name, count, folder);

    const lead = document.createElement("div");
    lead.className = "lead";
    const leader = team.lead && named(team.lead);
    if (leader) {
      const row = personRow(leader);
      const badge = document.createElement("span");
      badge.className = "badge";
      badge.textContent = "Lead";
      row.querySelector(".name").prepend(badge);
      lead.append(row);
    } else {
      const none = document.createElement("p");
      none.className = "no-lead";
      none.textContent = "No lead yet. Choose one below: it is the teammate you give the work to.";
      lead.append(none);
    }

    const crew = document.createElement("ul");
    crew.className = "crew";
    for (const id of team.members) {
      const a = named(id);
      if (!a) continue;
      const li = document.createElement("li");
      const off = document.createElement("button");
      off.type = "button";
      off.className = "take-off";
      off.textContent = "Take off";
      off.title = `Take ${a.name} off ${team.name}`;
      off.addEventListener("click", async () => {
        off.disabled = true;
        try {
          await invoke("leave_team", { id: team.id, agent: a.id });
        } finally {
          await drawTeams();
        }
      });
      li.append(personRow(a, off));
      crew.append(li);
    }
    if (!crew.children.length) {
      const li = document.createElement("li");
      li.className = "nobody";
      li.textContent = leader
        ? `Nobody on it yet, so ${leader.name} has nobody to hand work to.`
        : "Nobody on it yet.";
      crew.append(li);
    }

    const changes = document.createElement("div");
    changes.className = "changes";
    // The team's own work: a task of the lead's, named after the team, whose
    // lead is told it is the team's and hands out the parts.
    const give = document.createElement("button");
    give.type = "button";
    give.className = "team-task";
    give.disabled = !leader;
    retitle = () => {
      give.textContent = `Give ${team.name} a task`;
      give.title = leader
        ? `A new task for ${team.name}: ${leader.name} hands each part to whoever on the team fits`
        : "Choose a lead first: it is the one the team's tasks go to";
      folder.title = `Open the folder where ${team.name} keeps its work. Everybody on the team can write in it.`;
    };
    retitle();
    // Through the box +, aimed at this team: the task is made with what it
    // is for, rather than made first and left empty if nothing was said.
    give.addEventListener("click", () => openNewTask({ aim: { kind: "team", id: team.id } }));
    changes.append(give);
    const onIt = new Set([team.lead, ...team.members]);
    const addable = everybody.filter((a) => !onIt.has(a.id)).map((a) => [a.id, label(a)]);
    if (addable.length) {
      const add = chooser("Put a teammate on it…", addable, (agent) =>
        invoke("join_team", { id: team.id, agent }),
      );
      add.className = "team-add";
      changes.append(add);
    }
    const leaders = everybody.filter((a) => a.id !== team.lead).map((a) => [a.id, label(a)]);
    if (leaders.length) {
      const choose = chooser(team.lead ? "Change the lead…" : "Choose the lead…", leaders, (agent) =>
        invoke("lead_team", { id: team.id, lead: agent }),
      );
      choose.className = "team-lead";
      changes.append(choose);
    }
    const breakUp = document.createElement("button");
    breakUp.type = "button";
    breakUp.className = "team-break";
    breakUp.textContent = "Break up";
    breakUp.title = "The teammates stay; only the team goes";
    // Asked twice, the way Delete is: the second press answers the question
    // the first one put on the button.
    breakUp.dataset.sure = "false";
    breakUp.addEventListener("click", async () => {
      if (breakUp.dataset.sure !== "true") {
        breakUp.dataset.sure = "true";
        breakUp.textContent = `Break up ${team.name}? Its teammates stay`;
        return;
      }
      try {
        await invoke("break_up_team", { id: team.id });
      } finally {
        await drawTeams();
      }
    });
    changes.append(breakUp);

    card.append(header, lead, crew, changes);
    return card;
  });

  if (!cards.length) {
    const empty = document.createElement("p");
    empty.className = "empty";
    empty.textContent =
      "No teams yet. Make one, choose its lead, and put on it the teammates the lead should hand work to.";
    cards.push(empty);
  }
  el.teamsList.replaceChildren(...cards);

  // Who is on no team: they can still ask anybody, and anybody can be put on one.
  const onATeam = new Set(teamsKnown.flatMap((t) => [t.lead, ...t.members]).filter(Boolean));
  const free = everybody.filter((a) => !onATeam.has(a.id));
  el.teamsFree.hidden = !free.length;
  el.teamsFreeList.replaceChildren(
    ...free.map((a) => {
      const li = document.createElement("li");
      li.dataset.agent = a.id;
      const name = document.createElement("span");
      name.textContent = a.name;
      li.append(tile(kindFor(a), busy(a.id), a.hue), name);
      if (a.title) {
        const role = document.createElement("span");
        role.className = "role";
        role.textContent = a.title;
        li.append(role);
      }
      return li;
    }),
  );
}

async function newTeam() {
  const n = teamsKnown.length + 1;
  try {
    const id = await invoke("make_team", { name: n === 1 ? "A team" : `Team ${n}`, lead: null });
    await drawTeams();
    const name = el.teamsList.querySelector(`.team[data-team="${id}"] .team-name`);
    name?.focus();
    name?.select();
  } catch {
    await drawTeams();
  }
}

el.teamsNew.addEventListener("click", newTeam);
// A teammate made from here is asked who it is first, as + used to ask.
el.teamsNewTeammate.addEventListener("click", () => {
  closeMission();
  start({ introduce: true });
});

el.missionOpen.addEventListener("click", () => showMission());
el.missionDone.addEventListener("click", closeMission);
el.missionTasks.addEventListener("click", () => {
  if (el.overview.hidden) showOverview();
});
el.missionTeams.addEventListener("click", () => {
  if (el.teams.hidden) showTeams();
});
document.addEventListener("keydown", (e) => {
  // Not while a team's name is being typed, where Escape puts the old one
  // back, nor while the palette or the chooser is over it: Escape closes
  // that alone.
  if (e.key !== "Escape" || el.mission.hidden || !el.newTask.hidden || !el.palette.hidden) return;
  if (e.target.closest?.(".team-name")) return;
  // A search on the Tasks tab is let go of first; the next Escape leaves.
  if (e.target === el.overviewFind && el.overviewFind.value) {
    el.overviewFind.value = "";
    el.overviewFind.dispatchEvent(new Event("input"));
    return;
  }
  closeMission();
});
for (const [value, label] of SHOWING) {
  const option = document.createElement("option");
  option.value = value;
  option.textContent = label;
  el.overviewShow.append(option);
}
for (const [box, key] of [
  [el.overviewGroup, "errand-overview-group"],
  [el.overviewOrder, "errand-overview-order"],
  [el.overviewShow, "errand-overview-show"],
]) {
  box.addEventListener("change", () => {
    try {
      localStorage.setItem(key, box.value);
    } catch {
      // Kept for this look only.
    }
    drawOverview();
  });
}

/**
 * What a task is doing, and the line that says so. Where it stands is
 * whereItStands, as for its chip and its row down the side; what the app last
 * said is running only words the line, with the step a running task is on.
 */
function whatItIsDoing(t, going = goingNow()) {
  const is = whereItStands(t.id, t.agent, going);
  const told = overviewKnows.running.find(
    (w) => w.conversation === t.id && !w.command && w.waiting === (is.state === "waiting"),
  )?.what;
  const open = theOpenQuestion(talks.get(t.id));
  const line = {
    finished: () => `Finished ${howLongAgo(talks.get(t.id)?.finished ?? t.finished)}`,
    waiting: () => told || `Needs you: ${(open?.kind === "asking" ? open.text : open?.what) || "a question"}`,
    working: () => told || "Running now",
    paused: () =>
      is.off ? `Its routine (${is.off.at}) is paused` : "Paused: nothing of it runs on its own",
    stopped: () => is.stopped.stopped || "Stopped",
    scheduled: () =>
      is.next ? `Next: ${whenNext(is.next.due)}, ${is.next.what}` : `Watching ${is.watch.at}`,
    idle: () => (t.spoke ? `Last spoke ${howLongAgo(t.spoke)}` : "Not asked anything yet"),
  }[is.state]();
  return [is.state, line];
}

/** When a routine is next due, the way somebody says it. */
function whenNext(at) {
  const when = new Date(at);
  const today = new Date();
  const time = when.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (when.toDateString() === today.toDateString()) return `today ${time}`;
  const tomorrow = new Date(today.getTime() + 86_400_000);
  if (when.toDateString() === tomorrow.toDateString()) return `tomorrow ${time}`;
  return `${when.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} ${time}`;
}

/** Every task, grouped and ordered the way the controls say. */
function drawOverview() {
  const show = el.overviewShow.value || "all";
  const going = goingNow();
  nowDrawnAt = statesAt(going);
  const everyone = tasksNow
    .filter((t) => agents.has(t.agent))
    .filter(
      (t) =>
        t.said ||
        going.some((w) => w.conversation === t.id) ||
        overviewKnows.running.some((w) => w.conversation === t.id) ||
        standingNow.some((s) => s.conversation === t.id),
    )
    .filter((t) => !overviewFound || overviewFound.ids.has(t.id))
    .filter((t) =>
      shown(
        show,
        whatItIsDoing(t, going)[0],
        standingNow.some((s) => s.conversation === t.id),
      ),
    );
  for (const t of everyone) t.who = agents.get(t.agent)?.name || "";
  const byPriority = el.overviewOrder.value === "priority";
  const doing = new Map(everyone.map((t) => [t.id, whatItIsDoing(t, going)]));

  // Each group with what it is, when that is a state: its panel says so in
  // colour as well as in words. Or each teammate's tasks together.
  let groups;
  if (el.overviewGroup.value !== "state") {
    groups = byTeammate(everyone).map(([who, list]) => [null, who, list]);
  } else {
    // Answered is three groups: the person's to check, somebody else's, and
    // the person's but quiet for over a week. The last two are folded away
    // above Finished, each with a way to mark the lot finished.
    const now = Date.now();
    const answered = (as) =>
      everyone.filter((t) => doing.get(t.id)[0] === "idle" && answeredAs(t, now) === as);
    groups = [];
    for (const [state, label] of STATES) {
      if (state === "idle") {
        groups.push([state, label, answered("yours")]);
        continue;
      }
      if (state === "finished") {
        groups.push(
          ["idle-others", "Answered for someone else", answered("others")],
          ["idle-quiet", "Answered, quiet for over a week", answered("quiet")],
        );
      }
      groups.push([state, label, everyone.filter((t) => doing.get(t.id)[0] === state)]);
    }
  }

  const drawn = groups
    .filter(([, , list]) => list.length)
    .map(([state, label, list]) => {
      const group = document.createElement("section");
      group.className = "job-group";
      if (state) group.dataset.state = state;
      const head = document.createElement("h2");
      const count = note("span", String(list.length), "count");
      count.title = `${list.length} ${list.length === 1 ? "task" : "tasks"}`;
      if (FOLDED_IN_NOW.has(state)) {
        // Folded until somebody opens it: its name and how many, and the one
        // thing anybody does with a pile like this.
        const open = nowGroupsOpen.has(state);
        group.classList.add("folds");
        if (open) group.classList.add("open");
        const fold = document.createElement("button");
        fold.type = "button";
        fold.className = "fold-group";
        fold.setAttribute("aria-expanded", String(open));
        fold.title = open ? "Fold these away" : "Show them";
        fold.innerHTML =
          '<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M6 3.5 10.5 8 6 12.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>';
        fold.append(note("span", label, "label"), count);
        fold.onclick = () => {
          if (open) nowGroupsOpen.delete(state);
          else nowGroupsOpen.add(state);
          drawOverview();
        };
        head.append(fold, finishingAll(state, list));
      } else {
        head.append(note("span", label, "label"), count);
      }
      const tiles = document.createElement("div");
      tiles.className = "jobs";
      // What is next, soonest first: it is the one order a list of what is
      // coming up can have.
      const due = (t) => whereItStands(t.id, t.agent, going).next?.due ?? Infinity;
      const ordered = state === "scheduled" ? [...list].sort((x, y) => due(x) - due(y)) : inOrder(list, byPriority);
      tiles.append(...ordered.map((t) => aTask(t, doing.get(t.id))));
      group.append(head, tiles);
      return group;
    });
  const nothing = overviewFound
    ? "Nothing matches that."
    : show !== "all"
      ? "No tasks like that just now."
      : "No tasks yet. Give a teammate something to do.";
  el.overviewTiles.replaceChildren(...(drawn.length ? drawn : [note("p", nothing, "quiet")]));
}

/** The groups in Now that start folded, and which of them somebody opened. */
const FOLDED_IN_NOW = new Set(["idle-others", "idle-quiet"]);
const nowGroupsOpen = new Set();

/** Which group's Mark all finished was pressed once, and when: it asks first. */
let sureAboutAll = null;

/**
 * Mark all finished, for a folded group: asked once on the button itself, the
 * way deleting is, because it is thirty tasks at a time. Each can be opened
 * again from Finished.
 */
function finishingAll(state, list) {
  const sure = sureAboutAll?.state === state && Date.now() - sureAboutAll.at < 8000;
  const b = document.createElement("button");
  b.type = "button";
  b.className = "finish-all";
  b.dataset.sure = String(sure);
  b.textContent = sure ? `Mark ${list.length} finished?` : "Mark all finished";
  b.title = sure
    ? "Press again to mark every one of them finished. Each can be opened again from Finished."
    : `Mark all ${list.length} finished: they move to Finished, and each can be opened again`;
  b.onclick = async () => {
    if (!sure) {
      sureAboutAll = { state, at: Date.now() };
      drawOverview();
      return;
    }
    sureAboutAll = null;
    await finishThemAll(list);
  };
  return b;
}

/** Mark several tasks finished at once, here and in the app. */
async function finishThemAll(list) {
  const at = Date.now();
  const set = (t, when) => {
    t.finished = when;
    const held = talks.get(t.id);
    if (held) held.finished = when;
    for (const one of tasksNow) if (one.id === t.id) one.finished = when;
  };
  for (const t of list) set(t, at);
  drawTalks();
  drawThreads();
  if (!el.overview.hidden) drawOverview();
  const failed = [];
  await Promise.all(
    list.map((t) => invoke("finish_task", { id: t.id, finished: true }).catch(() => failed.push(t))),
  );
  if (failed.length) {
    for (const t of failed) set(t, null);
    drawTalks();
    drawThreads();
    if (!el.overview.hidden) drawOverview();
    complain(`${failed.length} of them could not be marked finished.`);
  }
}

/** One task, as a tile, with its teammate on it. */
function aTask(t, [state, line]) {
  const a = agents.get(t.agent);
  const job = document.createElement("article");
  job.className = "job";
  job.dataset.task = t.id;
  job.dataset.agent = t.agent;
  job.dataset.priority = String(t.priority);
  if (t.finished) job.classList.add("is-finished");

  const head = document.createElement("div");
  head.className = "job-head";
  const who = document.createElement("div");
  who.className = "job-who";
  const title = note("span", titleOf(t), "job-name");
  // All of it, when the tile has room for only some.
  title.title = titleOf(t) === t.name ? t.name : (t.first || titleOf(t)).trim();
  who.append(title);
  who.append(note("span", [a?.name, a?.title].filter(Boolean).join(" \u00b7 "), "job-role"));
  const marks = a ? repeatMarks(a, t.id) : [];
  head.append(tile(a ? kindFor(a) : "default", state === "working", a?.hue), who);
  if (marks.length) {
    const side = document.createElement("span");
    side.className = "job-marks";
    side.append(...marks);
    head.append(side);
  }
  job.append(head);
  // Where the search found it, so a tile found by a word says where the word is.
  const hit = overviewFound?.hits.get(t.id);
  if (hit) job.append(note("p", hit.snippet, "job-hit"));

  const chip = note("span", line, "job-state");
  chip.dataset.state = state;
  chip.title = line;
  job.append(chip);

  const foot = document.createElement("div");
  foot.className = "job-foot";
  foot.append(note("span", t.spoke ? howLongAgo(t.spoke) : "", "when"));

  const priority = document.createElement("select");
  priority.title = "How much this task matters";
  priority.setAttribute("aria-label", `Priority of ${titleOf(t)}`);
  for (const [value, label] of [["1", "High"], ["2", "Normal"], ["3", "Low"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    priority.append(option);
  }
  priority.value = String(t.priority);
  priority.onchange = async () => {
    const was = t.priority;
    t.priority = Number(priority.value);
    drawOverview();
    drawThreads();
    try {
      await invoke("set_task_priority", { id: t.id, priority: t.priority });
    } catch (why) {
      t.priority = was;
      drawOverview();
      drawThreads();
      complain(String(why));
    }
  };

  const finish = document.createElement("button");
  finish.type = "button";
  finish.textContent = t.finished ? "Not finished" : "Finished";
  finish.title = t.finished
    ? "It is not done after all: back with the open tasks"
    : "Mark this task done: its teammate carries on, and the task moves to Finished";
  finish.onclick = () => markTaskFinished(t, !t.finished);

  const open = document.createElement("button");
  open.type = "button";
  open.className = "open";
  open.textContent = "Open";
  open.onclick = async () => {
    closeOverview();
    // At the line the search found, when it found one; otherwise at the task.
    if (hit) return goToTheLine(hit);
    await openAgent(t.agent);
    if (talks.has(t.id) && showing !== t.id) await show(t.id);
  };

  foot.append(priority, finish, open);
  job.append(foot);
  return job;
}

/**
 * What happened while somebody was away, and what is open now.
 *
 * The two questions asked first on coming back, answered before the tiles:
 * the runs that happened on their own since the overview was last looked at,
 * and whatever is waiting on them, working, or stopped.
 */
async function drawAway() {
  const since = lastLookedAt() || Date.now() - 86_400_000;
  let ran = [];
  try {
    ran = (await invoke("happened_since", { since })) || [];
  } catch {
    ran = [];
  }
  const finished = ran.filter((r) => r.outcome);
  const failed = finished.filter((r) => r.failed);

  const away = document.createElement("section");
  away.append(note("h2", `Since you last looked, ${howLongAgo(since)}`, ""));
  away.append(
    note(
      "p",
      finished.length
        ? `${finished.length} run${finished.length === 1 ? "" : "s"} on ${finished.length === 1 ? "its" : "their"} own${failed.length ? `, ${failed.length} of them failed` : ""}.`
        : "Nothing ran on its own.",
      "",
    ),
  );
  const list = document.createElement("ul");
  for (const r of finished.slice(0, 8)) {
    const li = document.createElement("li");
    const who = document.createElement("button");
    who.type = "button";
    who.textContent = r.who;
    who.onclick = async () => {
      closeOverview();
      await openAgent(r.agent);
      // At the task that ran, not whichever of its teammate's was open last.
      if (r.conversation && talks.has(r.conversation) && showing !== r.conversation) {
        await show(r.conversation);
      }
    };
    const when = new Date(r.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    li.append(who, ` at ${when}: `);
    li.append(
      r.failed
        ? note("span", `failed. ${r.outcome}`, "failed")
        : note("span", r.said || "done", "gist"),
    );
    list.append(li);
  }
  if (finished.length) away.append(list);

  const open = document.createElement("section");
  open.append(note("h2", "Open now", ""));
  // Tasks, each with its teammate: two of one teammate's tasks can be in two
  // different states, and it is the task somebody goes to look at.
  const going = goingNow();
  const byState = (state) =>
    tasksNow
      .filter((t) => agents.has(t.agent) && whatItIsDoing(t, going)[0] === state)
      .map((t) => `${titleOf(t)} (${agents.get(t.agent).name})`);
  // A command left running is not where its task stands: the turn that
  // started it is over, and the task can be asked something else. It is still
  // running, though, and only the app's list knows it.
  const commands = overviewKnows.running.filter((w) => w.command).map((w) => `${w.what} (${w.who})`);
  const said = [
    ["Needs you", byState("waiting")],
    ["Running now", byState("working")],
    ["Stopped", byState("stopped")],
    ["Commands still running", commands],
  ].filter(([, names]) => names.length);
  if (!said.length) open.append(note("p", "Nothing needs you, is running, or is stopped.", ""));
  for (const [label, names] of said) {
    open.append(note("p", `${label}: ${names.join("; ")}`, ""));
  }
  el.overviewAway.replaceChildren(away, open);
}
el.modelsDone.addEventListener("click", () => {
  el.models.hidden = true;
  // Whatever changed in there, the teammate on screen says it now.
  drawWordsGo();
  if (!el.whois.hidden) {
    drawOwnModel();
    drawLimit();
  }
});

/**
 * Teammates whose own model was just taken out of the picker: they follow
 * Errand's model again, and Settings says who.
 */
function theyLostTheirModel(names) {
  if (!names?.length) return;
  for (const one of agents.values()) {
    if (names.includes(one.name)) one.ownModel = null;
  }
  const who = names.length === 1 ? names[0] : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  el.errandModelSays.textContent = `${who} ${names.length === 1 ? "goes" : "go"} back to Errand's model, since ${names.length === 1 ? "its" : "their"} own was taken out.`;
}

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
      out.title =
        "Take it out of the picker. A teammate given it as its own model goes back to Errand's model; if it is Errand's model, choose another.";
      out.onclick = async () => {
        theyLostTheirModel(await invoke("stop_offering", { id: one.id }));
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
        drop.title =
          "Forget the address and its key, and take its models out of the picker. A teammate given one of them as its own model goes back to Errand's model.";
        drop.onclick = async () => {
          theyLostTheirModel(await invoke("forget_backend", { id: place.id }));
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
            ? `Choose which of its ${kept.models.length} model${kept.models.length === 1 ? "" : "s"} to show in the picker:`
            : "It offered no models, which is what a key with nothing enabled on it looks like.",
        ].join(" ");
    // And the models themselves, right here. Nothing else on this screen
    // offered them: they live in a card above the form, behind a button, so
    // adding a provider ended with a sentence about three models and no way
    // to reach any of them.
    showTheModels(kept);
  } catch (why) {
    el.handSays.dataset.wrong = "true";
    el.handSays.textContent = String(why);
    el.handModels.replaceChildren();
  }
  // The key is not kept in the page for a moment longer than it takes to send.
  el.handKey.value = "";
  backToAdding();
  await drawKept();
});

/**
 * The models a newly added place offered, each with a way to show it.
 *
 * The same row as the list above, deliberately: the same dot for whether it is
 * loaded, the same words, the same button doing the same thing. A second way
 * of choosing a model would be a second thing to keep working.
 */
function showTheModels(place) {
  const models = place.models || [];
  if (!models.length) {
    el.handModels.replaceChildren();
    return;
  }
  el.handModels.replaceChildren(
    ...models.map((m) => {
      const row = document.createElement("li");
      const lit = document.createElement("span");
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
        try {
          await invoke("offer_this", {
            engine: "local",
            label: `${m.model} · ${place.label}`,
            settings: JSON.stringify({
              provider: place.provider,
              base_url: place.base_url,
              model: m.model,
              wire: place.wire || "openai",
            }),
            backend: place.id,
          });
        } catch (why) {
          add.disabled = false;
          el.handSays.dataset.wrong = "true";
          el.handSays.textContent = String(why);
          return;
        }
        thePickerHasChanged();
        add.textContent = "In the picker";
        add.className = "on";
        await Promise.all([drawChosen(), drawKept()]);
        const a = whose();
        if (a) await drawEngines(a);
      };
      row.append(lit, words, add);
      return row;
    }),
  );
}


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
      "for as long as Errand is running, window or no window; the switch under " +
      "Settings brings Errand back after a restart.",
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

  // It used to say "only Claude is paid for" while twelve of thirteen agents
  // ran on models billed by the token.
  if (spent.nothing_yet) {
    el.costing.replaceChildren(
      note(
        "p",
        "Nothing has been paid for yet. Claude says what each errand cost, in dollars. " +
          "DeepSeek, Kimi and other hosted models are paid for by the token, and what each " +
          "errand used is counted here. A model on this Mac or your own network costs nothing " +
          "and never appears here.",
      ),
    );
    return;
  }

  const money = (d) => `$${d.toFixed(2)}`;
  const total = (rows) => rows.reduce((sum, one) => sum + one.dollars, 0);
  const agents = (rows) => new Set(rows.map((one) => one.agent)).size;
  const counted = (n) => (n === 1 ? "agent" : "agents");

  // Claude in dollars, because it says; hosted models in tokens, because what
  // a token costs depends on a plan this app cannot see, and a guessed price
  // beside an agent is worse than an honest count.
  const section = (title, rows, used) => {
    if (!rows.length && !used.length) return [note("p", `${title}: nothing.`, "server-what")];
    const parts = [];
    if (rows.length) {
      parts.push(
        note("p", `${title}: ${money(total(rows))} on Claude across ${agents(rows)} ${counted(agents(rows))}.`, "server-what"),
        ...paid(rows),
      );
    }
    if (used.length) {
      const all = used.reduce((sum, one) => sum + one.tokens_in + one.tokens_out, 0);
      parts.push(
        note(
          "p",
          `${title}: ${tokensSaid(all)} tokens on hosted models across ${agents(used)} ${counted(agents(used))}.`,
          "server-what",
        ),
        ...used.map((one) => {
          const row = document.createElement("div");
          row.className = "one";
          const who = document.createElement("span");
          who.className = "who";
          who.textContent = one.who;
          const much = document.createElement("span");
          much.className = "where";
          much.textContent = `${one.model} at ${one.by} · ${tokensSaid(one.tokens_in)} in, ${tokensSaid(
            one.tokens_out,
          )} out · ${one.errands} ${one.errands === 1 ? "errand" : "errands"}`;
          row.append(who, much);
          return row;
        }),
      );
    }
    return parts;
  };

  const paid = (rows) =>
    rows.map((one) => {
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

  const parts = [
    ...section("Today", spent.today, spent.used_today || []),
    ...section("This month", spent.this_month, spent.used_this_month || []),
  ];
  if ((spent.used_this_month || []).length) {
    parts.push(
      note(
        "p",
        "Hosted models are counted in tokens rather than dollars: what a token costs depends on " +
          "your plan with each provider, and Errand does not guess.",
      ),
    );
  }
  el.costing.replaceChildren(...parts);
}

/**
 * Everything that runs on its own, every agent's, in one list.
 *
 * Routines and watches were found by opening each agent in turn and looking
 * for a clock beside a conversation's name, so nobody could say what their Mac
 * would do overnight without going through all of them.
 */
async function whatRunsOnItsOwn({ again = false } = {}) {
  if (!again && !el.standing.hidden) {
    el.standing.hidden = true;
    return;
  }
  el.standing.hidden = false;
  let all;
  try {
    all = await invoke("standing");
  } catch (why) {
    el.standing.replaceChildren(note("p", String(why)));
    return;
  }
  if (!all.length) {
    el.standing.replaceChildren(
      note(
        "p",
        "Nothing runs on its own yet. Ask any agent to do something every morning, or to keep an " +
          "eye on a folder or a page, and it appears here.",
      ),
    );
    return;
  }
  const routines = all.filter((one) => one.kind === "routine").length;
  const watches = all.length - routines;
  const agentsIn = new Set(all.map((one) => one.agent)).size;
  const counted = (n, one, many) => `${n} ${n === 1 ? one : many}`;
  el.standing.replaceChildren(
    note(
      "p",
      `${counted(routines, "routine", "routines")} and ${counted(watches, "watch", "watches")}, across ${counted(
        agentsIn,
        "agent",
        "agents",
      )}.`,
      "server-what",
    ),
    ...all.map(aStandingJob),
  );
}

function aStandingJob(one) {
  const row = document.createElement("div");
  row.className = "one";
  const when = one.kind === "routine" ? one.at : `watching ${one.at}`;
  // What stops it comes first, because it is the only thing about it somebody
  // has to do something about.
  const state = one.paused
    ? "its agent is paused"
    : one.off
      ? "switched off"
      : one.stopped
        ? `stopped: ${one.stopped}`
        : one.due
          ? `next ${new Date(one.due).toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit" })}`
          : "";
  const open = document.createElement("button");
  open.type = "button";
  open.textContent = "Open";
  open.onclick = async () => {
    el.standing.hidden = true;
    if (!talks.has(one.conversation)) await openAgent(one.agent);
    if (talks.has(one.conversation)) await show(one.conversation);
  };
  row.append(
    note("span", one.who, "who"),
    note("span", `${one.name} · ${when}`, "when"),
    ...(state ? [note("span", state, one.due && !one.paused && !one.off && !one.stopped ? "when" : "state")] : []),
    open,
  );
  // A routine is switched off and on here, the same switch as under Repeat.
  if (one.kind === "routine") {
    const flip = document.createElement("button");
    flip.type = "button";
    flip.textContent = one.off ? "Start again" : "Pause";
    flip.onclick = async () => {
      try {
        await invoke("routine_off", { id: one.conversation, off: !one.off });
      } catch (why) {
        complain(String(why));
        return;
      }
      const t = talks.get(one.conversation);
      if (t) {
        t.repeats = one.off;
        drawTalks();
      }
      whatRunsOnItsOwn({ again: true });
    };
    row.append(flip);
  }
  row.append(note("span", one.what, "what"));
  return row;
}

/** A number of tokens the way somebody would say it: 950, 12.4k, 1.3M. */
function tokensSaid(n) {
  // One place after the point, and none when it would be nought.
  const onePlace = (x) => x.toFixed(1).replace(/\.0$/, "");
  if (n < 1000) return String(n);
  if (n < 10000) return `${onePlace(n / 1000)}k`;
  if (n < 1000000) return `${Math.round(n / 1000)}k`;
  return `${onePlace(n / 1000000)}M`;
}
