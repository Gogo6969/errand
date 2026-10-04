// The window, run against a stand-in for the app behind it.
//
// This exists because of a specific failure and should not be removed without
// replacing it. Three separate changes were shipped after being "verified" by
// reading the code and by measuring the stylesheet, and the third one left both
// pickers in the header empty on startup. Reading a diff cannot catch that.
// Only running the page can, because the failure is not in any one function: it
// is in what happens to the ones after it when an earlier one quietly does
// nothing.
//
// So: the real index.html, the real app.js, the real stylesheet, and a stand-in
// for `window.__TAURI__` that answers the way the app does. Every check below
// is a thing somebody looked at the window and found wrong.
//
// Run it with `ui/harness/run.sh`, which prints one line per check and exits
// non-zero if any of them failed.

/** What the app would find in a store that has been used for a while. */
export const FIXTURE = {
  agents: [
    {
      id: "agent-unnamed",
      name: "Show me the latest Bitcoin news",
      title: null,
      about: null,
      mark: null,
      hue: null,
      asks: "ask",
      pinned: false,
      hidden: false,
      cwd: "/tmp/a",
      model: null,
      started_at: 1,
      spoke_at: 2,
      engine: "claude",
      engine_settings: null,
    },
    {
      id: "agent-bitcoin",
      name: "Bitcoin Desk",
      title: "Markets",
      about: "The morning crypto briefing",
      mark: "chart",
      hue: "green",
      asks: "ask",
      pinned: false,
      hidden: false,
      cwd: "/tmp/b",
      model: null,
      started_at: 1,
      spoke_at: 3,
      engine: "claude",
      engine_settings: "opus",
    },
  ],
  conversations: {
    "agent-unnamed": [
      { id: "talk-1", agent: "agent-unnamed", name: "First", opened: true },
      // Two that exist only to be opened once each, because whether a handover
      // still has its buttons is decided while a conversation is read back and
      // a conversation is read back exactly once.
      { id: "talk-waiting", agent: "agent-unnamed", name: "Waiting on you", opened: true },
      { id: "talk-over", agent: "agent-unnamed", name: "Was waiting", opened: true },
    ],
    "agent-bitcoin": [
      { id: "talk-2", agent: "agent-bitcoin", name: "First", opened: true },
      { id: "talk-3", agent: "agent-bitcoin", name: "Asked by Day Check", opened: true },
      { id: "talk-4", agent: "agent-bitcoin", name: "Answered a few", opened: true },
      { id: "talk-overnight", agent: "agent-bitcoin", name: "Ran overnight", opened: true },
      { id: "talk-cut-off", agent: "agent-bitcoin", name: "Cut off", opened: true },
      { id: "talk-tired", agent: "agent-bitcoin", name: "Asked over and over", opened: true },
      // A room, filed under this agent because it was the first one in.
      { id: "talk-room", agent: "agent-bitcoin", name: "Bitcoin room", opened: false },
    ],
  },
  // What the overview knows of a task besides the conversation: the first
  // thing asked in it, what matters, what is finished. Anything not here is a
  // task asked something, normal, not finished, never spoken to.
  tasks: {
    "talk-2": { first: "What moved overnight in Bitcoin?", spoke_at: 50 },
    "talk-3": { spoke_at: 40 },
  },
  // Who is in each room. Anything not listed here is an ordinary conversation.
  members: {
    "talk-room": [
      { agent: "agent-bitcoin", name: "Bitcoin Desk", talk: null },
      { agent: "agent-unnamed", name: "Show me the latest Bitcoin news", talk: null },
    ],
  },
  // What is stopping errands working. Nothing by default: the check that needs
  // one sets it, because a warning standing over every other check would be a
  // window nobody is testing in its ordinary state.
  whats_wrong: null,
  // How a routine has been going: one good morning, one failed, and one that
  // never came back because the machine slept. The three states the panel has
  // to be able to tell apart.
  // Twenty-three runs of an every-five-minute routine, which is more than one
  // page of history.
  wentBy: {
    "talk-1": Array.from({ length: 23 }, (_, i) => ({
      id: 23 - i,
      at: Date.now() - i * 300000,
      why: "clock",
      outcome: i === 22 ? "done" : "the model server is not answering",
    })),
  },
  went: [
    { at: Date.now() - 3600000, why: "clock", outcome: null },
    { at: Date.now() - 90000000, why: "clock", outcome: "the model server is not answering" },
    { at: Date.now() - 176400000, why: "clock", outcome: "done" },
  ],
  lines: {
    // Somebody is being asked to do something, and the agent is still sitting
    // there: `waiting_on_you` names this one.
    "talk-waiting": [
      {
        seq: 1,
        at: 1,
        kind: "over_to_you",
        text: "Sign in to your Apple Account\nThe order will not show without it\nhttps://secure.store.apple.com/shop/order/list",
        call: "still-waiting",
        tool: null,
        outcome: null,
      },
    ],
    // The same line, from an errand that ended long ago.
    "talk-over": [
      {
        seq: 1,
        at: 1,
        kind: "over_to_you",
        text: "Sign in to your Apple Account\nhttps://secure.store.apple.com/shop/order/list",
        call: "long-gone",
        tool: null,
        outcome: null,
      },
    ],
    // A room: the person asks, and each member answers under its own name.
    "talk-room": [
      { seq: 1, at: 1, kind: "mine", text: "Where is the price, and is it news?", call: null, tool: null, outcome: null },
      { seq: 2, at: 2, kind: "said", text: "About $77,700.", call: null, tool: null, outcome: null, said_by: "agent-bitcoin" },
      { seq: 3, at: 3, kind: "said", text: "Nothing new since yesterday.", call: null, tool: null, outcome: null, said_by: "agent-unnamed" },
    ],
    "talk-1": [
      { seq: 1, at: 1, kind: "mine", text: "Show me the latest Bitcoin news", call: null, tool: null, outcome: null },
      // A message with pictures on it, and one whose file has gone. Both are
      // states somebody will see: the second is what a tidied folder looks
      // like, and an empty gap there reads as the window being broken.
      { seq: 5, at: 5, kind: "mine", text: "What is wrong with this screen?", call: null, tool: null, outcome: null,
        pictures: ["5-0.png", "gone.png"] },
      { seq: 2, at: 2, kind: "said", text: "**BTC** is around $77,700.", call: null, tool: null, outcome: null },
    ],
    // Three of the same question answered, and a fourth still open. Its own
    // conversation rather than borrowing talk-4: a check that reads an open
    // question is a check another group can answer out from under it, and one
    // did.
    "talk-tired": [
      { seq: 1, at: 1, kind: "asking", text: "Fetch the price", call: "t1", tool: "Bash", outcome: "yes" },
      { seq: 2, at: 2, kind: "asking", text: "Fetch it again", call: "t2", tool: "Bash", outcome: "yes" },
      { seq: 3, at: 3, kind: "asking", text: "And again", call: "t3", tool: "Bash", outcome: "yes" },
      { seq: 4, at: 4, kind: "asking", text: "Fetch the price once more", call: "t4", tool: "Bash", outcome: null },
    ],
    // A turn the app was closed during: a question, and an ending that says so.
    // What made this worth a fixture is that there is no answer to hang the
    // ordinary "Ask again" on -- never getting one is the whole of what
    // happened -- so the ending itself has to offer it.
    "talk-cut-off": [
      { seq: 1, at: Date.now() - 60000, kind: "mine", text: "Show me the most important news of today", call: null, tool: null, outcome: null },
      { seq: 2, at: Date.now() - 30000, kind: "ended", call: "cut-off", tool: null, outcome: null,
        text: "Errand was closed while this was running, so it stopped part way. Nothing already written down was lost." },
      // Asked since, so the newest request is not the one that was cut off.
      // Run it again sent this one.
      { seq: 3, at: Date.now() - 20000, kind: "mine", text: "Delete the drafts folder", call: null, tool: null, outcome: null },
      { seq: 4, at: Date.now() - 10000, kind: "said", text: "Done: the drafts folder is gone.", call: null, tool: null, outcome: null },
    ],
    // A conversation an agent carried on while nobody was looking: yesterday's
    // briefing and today's, one under the other. Dated rather than numbered,
    // because the whole point of the separators is the real clock.
    "talk-overnight": [
      { seq: 1, at: Date.now() - 2 * 86400000, kind: "mine", text: "Every morning, tell me what moved", call: null, tool: null, outcome: null },
      { seq: 2, at: Date.now() - 2 * 86400000 + 60000, kind: "said", text: "Set. I will look at seven.", call: null, tool: null, outcome: null },
      { seq: 3, at: Date.now() - 86400000, kind: "said", text: "Yesterday: BTC up two per cent.", call: null, tool: null, outcome: null },
      { seq: 4, at: Date.now(), kind: "said", text: "This morning: BTC flat.", call: null, tool: null, outcome: null },
    ],
    // Three answered questions and a fourth still open, which is the shape that
    // makes the way out of being asked worth offering: somebody on their fourth
    // question is clicking through them, not weighing each one.
    "talk-4": [
      { seq: 1, at: 1, kind: "asking", text: "Read the notes", call: "a1", tool: "Bash", outcome: "yes" },
      { seq: 2, at: 2, kind: "asking", text: "Check memory", call: "a2", tool: "Bash", outcome: "yes" },
      { seq: 3, at: 3, kind: "asking", text: "List processes", call: "a3", tool: "Bash", outcome: "yes" },
    ],
    // A question with nothing written against it, in a conversation whose
    // engine is gone. Nobody will ever answer this one.
    "talk-2": [
      { seq: 1, at: 1, kind: "asking", text: "Delete the old backups", call: "c1", tool: "Bash", outcome: null },
    ],
    // The same row, in a conversation that is still live. This one is being
    // waited on this second, and it is the case that made an errand started
    // from outside impossible to answer.
    "talk-3": [
      { seq: 1, at: 1, kind: "asking", text: "Fetch BTC spot price", call: "c2", tool: "Bash", outcome: null },
    ],
  },
  /** Which conversation still has an engine behind it. */
  // Which conversations still have an engine behind them. More than one,
  // because whether a question can still be answered depends on it and there
  // is more than one thing worth asking about a question: this was a single
  // value, so any second check needing a live conversation had to borrow the
  // first one's and answer its question out from under it.
  liveConversations: ["talk-3", "talk-tired"],
  engines: [
    { engine: "claude", name: "Claude · your default", settings: null },
    { engine: "claude", name: "Claude · Opus", settings: "opus" },
    { engine: "claude", name: "Claude · Sonnet", settings: "sonnet" },
    { engine: "local", name: "qwen2.5:7b-instruct · Ollama", settings: JSON.stringify({ base_url: "http://127.0.0.1:11434", model: "qwen2.5:7b-instruct" }) },
    { engine: "local", name: "gemma-4-31b-it · LM Studio on 192.168.1.92 · needs loading", settings: JSON.stringify({ base_url: "http://192.168.1.92:1234", model: "gemma-4-31b-it" }) },
  ],
  checkup: [
    { what: "Claude Code", how: "fine", said: "2.1.221", fix: "" },
    { what: "Tool server: mempalace", how: "broken", said: "starting it: No such file or directory",
      fix: "Check the command in ~/.claude.json still exists." },
    { what: "Models on the network", how: "odd", said: "everything found is bound to this machine only",
      fix: "Start Ollama with OLLAMA_HOST=0.0.0.0." },
    // The one whose fix is done in System Settings, so it carries the pane to
    // open. The refusal used to be a line on stderr, which is nowhere.
    { what: "Notifications", how: "odd",
      said: "off for Errand in macOS, so an errand that finishes while you are reading something else finishes quietly",
      fix: "Open System Settings, then Notifications, then Errand, and switch Allow notifications on.",
      settings: "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=com.errandai.errand" },
  ],
  // What ran on its own while nobody was looking: one that went well, one
  // that failed, and one still going.
  happened: [
    { agent: "agent-bitcoin", who: "Bitcoin Desk", conversation: "talk-2", at: Date.now() - 3_600_000,
      why: "clock", outcome: "done", failed: false, said: "BTC is up 2% overnight." },
    { agent: "agent-bitcoin", who: "Bitcoin Desk", conversation: "talk-3", at: Date.now() - 7_200_000,
      why: "clock", outcome: "The model server answered with an error", failed: true, said: "" },
    { agent: "agent-bitcoin", who: "Bitcoin Desk", conversation: "talk-4", at: Date.now() - 60_000,
      why: "watch", outcome: null, failed: false, said: "" },
  ],
  settings: {},
  // The key agent, as the SSH key card asks after it: empty, with one key to
  // load, and what loading it does.
  sshKey: { agent: true, holds: 0, keys: ["id_ed25519"] },
  sshLoad: { added: ["id_ed25519"], holds: 1, why_not: null },
  whats_running: [
    { conversation: "talk-3", agent: "agent-bitcoin", who: "Bitcoin Desk", talk: "Asked by Day Check",
      what: "Needs you: Running a command", waiting: true },
    { conversation: "talk-2", agent: "agent-bitcoin", who: "Bitcoin Desk", talk: "First",
      what: "Looking something up on the web", waiting: false },
    // A command left running, with what it is printing. Until this, only the
    // model could see that -- it reaches the kept output through check_command
    // and nothing else did -- which is the wrong way round for the one person
    // who can decide to stop it.
    { conversation: "talk-2", agent: "", who: "Bitcoin Desk", talk: "First",
      what: "Building the thing", waiting: false, command: "job-1",
      tail: "Compiling errand-core v0.3.0\nCompiling errand-app v0.3.0\n" },
  ],
  brought: {
    skills: ["pdf", "docx", "artifact-design"],
    helpers: ["Explore", "general-purpose"],
    plugins: ["marketing", "productivity"],
    commands: ["init", "review"],
  },
  watches: {
    watches: "~/Downloads every 10m",
    what: "Sort these and tell me the total",
    means: "This looks at ~/Downloads every 10 minutes and wakes Bitcoin Desk when what is there changes. It compares the names and sizes of the files one level down, ignoring part-downloaded ones. At most once every 15 minutes, and at most 24 times a day. It only looks while Errand is running, window or no window, so something that changes while Errand is quit is something you hear about when it is opened again.",
    looked_at: 1788000000000,
    woke_at: null,
    woke_today: 0,
    misses: 0,
    paused: null,
  },
  // The one task with a goal.
  goalIn: "talk-cut-off",
  goal_of: {
    goal: "Get the tests passing",
    means:
      "The agent works towards this on its own and says at the end of every turn whether it is done. " +
      "It gets at most 8 turns, it stops early if it says the same thing is left twice running, and it " +
      "stops if it stops reporting at all. It only runs while Errand is running, window or no window. The goal is: Get the tests passing",
    tries: 3,
    at_most: 8,
    left: "two of them still fail on a timeout",
    over: null,
  },
  offered: [
    { id: "o-default", engine: "claude", label: "Claude - your default", settings: null, backend: null, sort: 0, mark: "claude|" },
    { id: "o-opus", engine: "claude", label: "Claude - Opus", settings: "opus", backend: null, sort: 1, mark: "claude|opus" },
    { id: "o-local", engine: "local", label: "qwen2.5:7b - Ollama", backend: "b-ollama", sort: 2,
      mark: "local|http://127.0.0.1:11434|qwen2.5:7b",
      settings: '{"provider":"ollama","base_url":"http://127.0.0.1:11434","model":"qwen2.5:7b"}' },
  ],
  backends: [
    { id: "b-ollama", label: "Ollama", provider: "ollama", base_url: "http://127.0.0.1:11434",
      has_key: false, wire: "openai", found: false, models: [], trouble: null },
    { id: "b-deepseek", label: "DeepSeek", provider: "openai-compat", base_url: "https://api.deepseek.com/anthropic",
      has_key: true, wire: "anthropic", found: false, models: [], trouble: null },
  ],
  look_for_models: [
    { id: "http://127.0.0.1:11434", label: "Ollama", provider: "ollama",
      base_url: "http://127.0.0.1:11434", has_key: false, wire: "openai", found: true, trouble: null,
      models: [
        { model: "qwen2.5:7b", loaded: true },
        { model: "llama3.2:1b", loaded: false },
      ] },
  ],
  models_at: {
    id: "b-deepseek", label: "DeepSeek", provider: "openai-compat",
    base_url: "https://api.deepseek.com/anthropic", has_key: true, wire: "anthropic",
    found: false, trouble: null,
    models: [{ model: "deepseek-v4-flash", loaded: true }],
  },
  // Two granted rules that look alike and are not: one covers every use of a
  // program, the other covers one command line and nothing else.
  allowances: [
    { id: "al-1", tool: "Bash", rule: "top", covers: "any top command" },
    { id: "al-2", tool: "Bash", rule: "printf a > f; ls", covers: "only this exact command" },
  ],
  // What Claude Code allows out of its own settings, which Errand can show and
  // cannot revoke. Two files, because which file a rule is in is the part
  // somebody needs in order to go and change it.
  also_allowed: {
    allow: [
      { rule: "Bash(awk *)", whose: "~/.claude/settings.json" },
      { rule: "Bash(chmod +x:*)", whose: "~/.claude/settings.local.json" },
    ],
    deny: [{ rule: "Read(//etc/**)", whose: "~/.claude/settings.json" }],
    mode: null,
    // The sentence the app writes, because it is the only side that knows what
    // this agent's posture puts on the command line.
    mode_says: null,
  },
  // Notes for the version running, as the app compiles them in.
  what_changed: {
    version: "0.1.0",
    // As many as the real ones, because two of them never fill the panel and
    // the fault being guarded against only appears once they do.
    lines: [
      "Answers arrive as they are written, rather than after several seconds of nothing.",
      "You can talk to it with your hands somewhere else.",
      "Repeat and Watch offer the errands you already asked in that conversation.",
      "Try it now, in both, so you can see what a routine does before a morning goes past.",
      "Errand can open when you log in, so standing jobs survive a restart.",
      "What it may do without asking now includes the rules Claude Code allows on its own.",
      "A new copy of Errand says what it is, in eight lines, once.",
      "From a terminal, the answer streams as it is written and a script can ask for JSON.",
      "Fixed: the first thing said to a new agent failed with a database error.",
      "Fixed: a routine set on a new agent was accepted and quietly kept by nobody.",
    ],
  },
  // What this Mac can be let at. Off until something turns one on.
  connectors: [
    {
      id: "mail",
      name: "Mail",
      sees: "Reads your mail: who wrote, when, the subject, and the first part of the message. It never sends anything and never deletes anything.",
      on: false,
    },
    {
      id: "calendar",
      name: "Calendar",
      sees: "Reads what is in your calendars: what, when, where, and which calendar. It never adds, moves or cancels anything.",
      on: false,
    },
    {
      id: "browser",
      name: "Chrome",
      sees: "Reads a web page in your own Chrome, the way you see it: signed in, and with the page's scripts run. That means a request does leave this Mac, carrying whatever you are signed in with. It opens a tab of its own, behind the one you are on, and closes it again; it never clicks, types or fills anything in, and never touches a tab you already had open. It never asks for a file, but a page it opens is a page, and a page can start a download the same as it would if you opened it yourself. Chrome has to allow this too, in its own menu bar: View, then Developer, then \"Allow JavaScript from Apple Events\".",
      on: false,
    },
  ],
  // Everything that runs on its own: one routine due, one switched off, one
  // whose agent is paused, and a watch that stopped.
  standing: [
    { conversation: "talk-2", agent: "agent-bitcoin", who: "Bitcoin Desk", name: "First", kind: "routine", at: "daily 07:00", what: "What moved overnight", due: Date.now() + 3600000, off: false, stopped: null, paused: false },
    { conversation: "talk-3", agent: "agent-bitcoin", who: "Bitcoin Desk", name: "Asked by Day Check", kind: "routine", at: "weekly fri 15:00", what: "The weekly tally", due: null, off: true, stopped: null, paused: false },
    { conversation: "talk-1", agent: "agent-unnamed", who: "Pulse Keeper", name: "First", kind: "routine", at: "every 5m", what: "Write one small pulse file", due: null, off: false, stopped: null, paused: true },
    { conversation: "talk-4", agent: "agent-bitcoin", who: "Bitcoin Desk", name: "Answered a few", kind: "watch", at: "https://example.com every 1h", what: "Tell me what changed", due: null, off: false, stopped: "Stopped looking. It could not be reached 5 times running.", paused: false },
  ],
  // What one agent may use in a month, and what it has used.
  limits: {
    "agent-bitcoin": { tokens: null, dollars: 20, used_tokens: 0, spent_dollars: 4.7 },
    "agent-hosted": { tokens: null, dollars: null, used_tokens: 1234567, spent_dollars: 0 },
  },
  // What one agent has been taught.
  skills: {
    "agent-bitcoin": [
      {
        name: "Morning brief",
        request: "Tell me what moved overnight",
        steps: [{ tool: "run_command", input: {}, outcome: "ok" }],
        made_at: 1,
      },
    ],
  },
  // What two agents have written down.
  notes: {
    "agent-bitcoin": [
      { about: "exchange", note: "Prices from Coinbase, not Binance", told: 3, told_at: 1 },
      { about: "report_time", note: "Send the summary before 08:00", told: 1, told_at: 2 },
    ],
  },
  what_it_cost: {
    today: [{ agent: "agent-bitcoin", who: "Bitcoin Desk", dollars: 0.19, turns: 1, errands: 1 }],
    this_month: [
      { agent: "agent-bitcoin", who: "Bitcoin Desk", dollars: 4.2, turns: 30, errands: 12 },
      { agent: "agent-gone", who: "an agent that is gone", dollars: 0.5, turns: 2, errands: 2 },
    ],
    // A hosted model, counted in tokens rather than priced.
    used_today: [],
    used_this_month: [
      {
        agent: "agent-unnamed",
        who: "Trend Scout",
        model: "deepseek-v4-flash",
        by: "api.deepseek.com",
        tokens_in: 1234567,
        tokens_out: 45210,
        errands: 31,
      },
    ],
    nothing_yet: false,
  },
  outside: [
    { name: "peekaboo", from: "~/.claude.json", tools: ["see", "click", "type"], trouble: null },
    {
      name: "mempalace",
      from: "~/.claude.json",
      tools: [],
      trouble: "starting it: No such file or directory",
      fix: "The program it starts is not on this Mac any more. It is set up under \"mempalace\" in ~/.claude.json.",
    },
  ],
};

/** Everything the window asked for, so a check can say what was never called. */
export const asked = [];

/** How much each teammate asks, as the window last set it. */
const postureNow = {};

/** Whether a line of the picker is served on this Mac or this network. */
function servedHere(o) {
  return o.engine === "local" && /\/\/(127\.|192\.168\.|10\.|localhost)/.test(o.settings || "");
}

/** Everything the page is listening for, by name. */
const listeners = {};

/** Deliver an event to the page, the way the app would. */
export function tell(name, payload) {
  for (const fn of listeners[name] || []) fn({ payload });
  return (listeners[name] || []).length;
}

/**
 * Stand in for the app, answering the way it does.
 *
 * `slowly` holds commands that take a while, which is not a detail: opening a
 * conversation starts an engine and binds a socket, and the window has to be
 * readable before that finishes rather than after.
 */
export function standIn(fixture = FIXTURE, breaking = {}, slowly = {}) {
  // What the file in LaunchAgents would say. The real one is read from disk
  // every time the screen opens; this is the same thing without a disk.
  let atLogin = "no";
  /** What is being watched, once anything has set or stopped it. */
  let watchedNow = null;
  /** Which connectors are switched on, once anything has switched one. */
  const connected = new Set();
  /**
   * What has not been read, task by task, which a check clears by opening the
   * task it is in. Mutable, because the behaviour under test is that it goes
   * away: a fixture answering the same thing twice cannot tell a mark that
   * clears from one that was never drawn. Counted by teammate the way the app
   * counts it, from the same lines, so the two cannot disagree.
   */
  const unread = new Map([["talk-2", { lines: 2, at: Date.now() - 3600000 }]]);
  const ownerOf = (conversation) =>
    Object.entries((fixture || FIXTURE).conversations).find(([, talks]) =>
      talks.some((t) => t.id === conversation),
    )?.[0];
  /** Whether the routine under test has been switched off. */
  let routineOff = false;
  return {
    /**
     * Put an agent back to having something nobody has read.
     *
     * A check cannot rely on the starting state here, because reading a
     * conversation is what every other group does on its way past, and the
     * whole behaviour under test is that reading clears this.
     */
    nowUnread(agent, lines = 2, at = Date.now() - 3600000) {
      const first = (fixture || FIXTURE).conversations[agent]?.[0]?.id;
      if (first) unread.set(first, { lines, at });
    },
    /** The same for one task, which is what the dot on a task is about. */
    nowUnreadIn(conversation, lines = 1, at = Date.now()) {
      unread.set(conversation, { lines, at });
    },
    core: {
      invoke(name, args) {
        asked.push({ name, args });
        if (breaking[name]) return Promise.reject(breaking[name]);
        if (slowly[name]) {
          return new Promise((go) => setTimeout(() => go(null), slowly[name]));
        }
        switch (name) {
          case "agents":
            return Promise.resolve(fixture.agents);
          // Every agent, unless a check says which ones its words name.
          case "matching":
            return Promise.resolve(fixture.matching ?? fixture.agents);
          // Every conversation, as a task.
          case "tasks":
            return Promise.resolve(
              Object.values(fixture.conversations)
                .flat()
                .map((c) => ({
                  priority: 2,
                  finished_at: null,
                  spoke_at: 0,
                  first: null,
                  said: true,
                  ...c,
                  ...(fixture.tasks?.[c.id] || {}),
                })),
            );
          case "conversations":
            return Promise.resolve(fixture.conversations[args.agent] || []);
          case "lines":
            return Promise.resolve(fixture.lines[args.id] || []);
          case "members":
            return Promise.resolve(fixture.members?.[args.id] || []);
          case "make_room":
            return Promise.resolve({
              name: args.name || `A room of ${args.agents.length}`,
              members: args.agents.map((id) => ({
                agent: id,
                name: fixture.agents.find((a) => a.id === id)?.name || id,
                talk: null,
              })),
            });
          // Whatever the check set, and nothing when it set nothing: an answer
          // of null is the app's own "was not asked".
          case "answering":
            return Promise.resolve(
              fixture.answering ? args.addresses.map((at) => fixture.answering[at] ?? null) : null,
            );
          case "engines":
            return Promise.resolve(
              fixture.offered.map((o) => ({ id: o.id, engine: o.engine, name: o.label, settings: o.settings, here: servedHere(o) })),
            );
          // Decided the way the app decides it: Errand's model, or what the
          // teammate was put on; and for one kept local, a model served here
          // or nothing at all.
          case "where_words_go": {
            const a = fixture.agents.find((x) => x.id === args.id);
            const chosen = fixture.offered.find((o) => o.id === fixture.settings.errand_model);
            if (!a?.keep_local || !chosen || servedHere(chosen)) {
              return Promise.resolve({
                stays: chosen ? servedHere(chosen) : false,
                model: chosen ? chosen.label : "Claude",
                refused: null,
              });
            }
            const local = fixture.offered.find((o) => o.id === fixture.settings.local_model && servedHere(o));
            return Promise.resolve(
              local
                ? { stays: true, model: local.label, refused: null }
                : { stays: true, model: "", refused: "This teammate keeps its words on your network, and Errand's model sends them elsewhere." },
            );
          }
          case "notifications":
            return Promise.resolve({
              state: fixture.notifying || "refused",
              settings: "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=com.errandai.errand",
            });
          case "keep_local": {
            const a = fixture.agents.find((x) => x.id === args.id);
            if (a) a.keep_local = args.on;
            return Promise.resolve(null);
          }
          case "outside":
            return Promise.resolve(fixture.outside);
          case "checkup":
            return Promise.resolve(fixture.checkup);
          case "whats_running":
            return Promise.resolve(fixture.whats_running);
          case "brought":
            return Promise.resolve(fixture.brought);
          // Whatever was last set, rather than the fixture every time. Saving
          // and stopping a watch are the two things this panel does, and a
          // stand-in that answers the same thing before and after cannot tell
          // a panel that works from one that does nothing at all.
          case "watches":
            return Promise.resolve(watchedNow ?? fixture.watches);
          case "watch_it":
            watchedNow = args.watches
              ? { ...fixture.watches, watches: args.watches, what: args.what }
              : { ...fixture.watches, watches: null, what: null, means: null, paused: null };
            return Promise.resolve(null);
          // The conversation with the live question in it is live; the rest
          // are history. Which is the whole distinction being tested.
          case "still_going":
            return Promise.resolve((fixture.liveConversations || []).includes(args.id));
          case "whats_offered":
            return Promise.resolve(fixture.offered);
          // Whether the app starts itself at login. Kept here rather than in
          // the fixture because the switch changes it: what it says has to be
          // what was last set, or the check cannot tell a switch that works
          // from one that only looks like it does.
          // What changed in this one, and whether anybody has been told yet.
          case "what_changed":
            return Promise.resolve({
              first_time: window.__TOLD__ !== true,
              notes: fixture.what_changed,
            });
          case "seen_what_changed":
            window.__TOLD__ = true;
            return Promise.resolve(null);
          // Deleting an agent. Answers plainly rather than doing anything to
          // the fixture: what the check is watching is that the window asks,
          // and what it does with its own list afterwards.
          case "forget":
            return Promise.resolve(null);
          // Somebody saying they have done the thing they were asked to do,
          // or that they are not going to.
          case "handed_back":
          case "show_in_browser":
            return Promise.resolve(null);
          case "teammates_at_login":
            return Promise.resolve(fixture.teammate_logins || []);
          case "stop_teammate_at_login":
            fixture.teammate_logins = (fixture.teammate_logins || []).filter(([label]) => label !== args.label);
            return Promise.resolve(null);
          // Which handovers are still being waited on. A line on disk cannot
          // say, so the window asks.
          case "waiting_on_you":
            return Promise.resolve(fixture.waiting_on_you || ["still-waiting"]);
          // What agents can be let at, and whether they are. Kept here rather
          // than in the fixture because the switch changes it: a stand-in that
          // answers the same thing before and after cannot tell a switch that
          // works from one that only looks like it does.
          // What each agent has said that nobody has read. Mutable, because
          // the whole behaviour under test is that opening a conversation
          // clears it: a fixture that answers the same thing twice cannot tell
          // a mark that goes away from one that was never drawn.
          case "what_is_new": {
            const byTeammate = {};
            for (const [conversation, { lines, at }] of unread) {
              const owner = ownerOf(conversation);
              if (!owner) continue;
              const was = byTeammate[owner];
              byTeammate[owner] = { lines: (was?.lines || 0) + lines, at: Math.max(was?.at || 0, at) };
            }
            return Promise.resolve(byTeammate);
          }
          case "what_is_new_in_tasks":
            return Promise.resolve(Object.fromEntries(unread));
          case "seen":
            unread.delete(args.conversation);
            return Promise.resolve(null);
          case "forget_conversation":
          case "looking_at":
            return Promise.resolve(null);
          case "whats_wrong":
            return Promise.resolve(fixture.whats_wrong || null);
          case "a_picture_to_send":
            // A real one-pixel PNG, so a check that looks for a drawn picture
            // fails on a broken image rather than passing on a stub string.
            return Promise.resolve(
              "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
            );
          // Narrowed by the app, exactly as pressing Always narrows it, and
          // said back in those words. `git status` becomes any git command,
          // and somebody has to be told that rather than find out.
          case "allow_in_advance": {
            const first = String(args.rule || "").trim().split(/\s+/)[0];
            if (!first) return Promise.reject("there is nothing to remember in that");
            if (args.tool === "folder") return Promise.resolve(`writing anywhere inside ${args.rule}`);
            return Promise.resolve(
              args.tool === "Bash" || args.tool === "commands"
                ? `any ${first} command`
                : `anything starting ${args.rule}`,
            );
          }
          // A real picture, small enough to sit in a fixture: one grey pixel.
          // A stub string would draw a broken image and the check would pass
          // on markup that shows nothing.
          case "a_picture":
            return args.name === "gone.png"
              ? Promise.reject("that picture is not here any more")
              : Promise.resolve(
                  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
                );
          // A routine switched off rather than thrown away, and what it did.
          // Switched on the routine and in the list of what repeats, as the
          // app keeps the one switch both read.
          case "routine_off":
            routineOff = !!args.off;
            for (const one of fixture.standing || []) {
              if (one.conversation === args.id && one.kind === "routine") one.off = !!args.off;
            }
            return Promise.resolve(null);
          case "export_conversation":
            return Promise.resolve("/Users/you/Desktop/First.md");
          // A page at a time, as the app answers: twenty, and the twenty
          // before a run's id when asked for older ones.
          case "how_it_has_been_going": {
            const all = fixture.wentBy?.[args.id] || fixture.went || [];
            const from = args.olderThan == null ? 0 : all.findIndex((run) => run.id === args.olderThan) + 1;
            return Promise.resolve(all.slice(from, from + 20));
          }
          // Where the words actually are, rather than only which agent has
          // them. Matched against the fixture's own lines so the answer and
          // what the page can show cannot drift apart.
          case "hits": {
            const needle = String(args.lookingFor || "").toLowerCase();
            const out = [];
            for (const [conversation, lines] of Object.entries(fixture.lines)) {
              const owner = Object.entries(fixture.conversations).find(([, talks]) =>
                talks.some((t) => t.id === conversation),
              )?.[0];
              const hit = [...lines].reverse().find((l) => l.text.toLowerCase().includes(needle));
              if (owner && hit) {
                out.push({
                  agent: owner,
                  conversation,
                  seq: hit.seq,
                  kind: hit.kind,
                  snippet: hit.text.slice(0, 120),
                });
              }
            }
            return Promise.resolve(out);
          }
          case "pause": {
            const a = fixture.agents.find((x) => x.id === args.id);
            if (!a) return Promise.reject("there is no such agent");
            a.paused_at = args.paused ? 1 : null;
            return Promise.resolve(null);
          }
          case "conversation_agent":
            return Promise.resolve(
              Object.entries(fixture.conversations).find(([, talks]) =>
                talks.some((t) => t.id === args.id),
              )?.[0] || null,
            );
          case "connectors":
            return Promise.resolve(
              (fixture.connectors || []).map((one) => ({ ...one, on: connected.has(one.id) })),
            );
          case "connect":
            if (args.on) connected.add(args.id);
            else connected.delete(args.id);
            return Promise.resolve(null);
          case "opens_at_login":
            // A check can put the third answer here, which is the one the app
            // cannot produce by pressing anything: something starts at login
            // and it is not this copy.
            return Promise.resolve(window.__AT_LOGIN__ ?? atLogin);
          case "open_at_login":
            atLogin = args.yes ? "yes" : "no";
            return Promise.resolve(atLogin);
          case "backends":
            return Promise.resolve(fixture.backends);
          case "look_for_models":
            return Promise.resolve(fixture.look_for_models);
          case "models_at":
            return Promise.resolve(fixture.models_at);
          case "remember_backend":
            return Promise.resolve(fixture.models_at);
          // A goal is one task's, and most have none: the fixture's is aimed
          // at in one task, and every other answers with nothing to aim at.
          case "goal_of":
            return Promise.resolve(
              args?.id === fixture.goalIn
                ? fixture.goal_of
                : { goal: null, means: "", tries: 0, at_most: 8, left: null, over: null },
            );
          case "allowances":
            return Promise.resolve(fixture.allowances);
          // How much a teammate asks, kept so what is worth allowing can
          // follow it the way the app's does.
          case "asks":
            postureNow[args.id] = args.how;
            return Promise.resolve(null);
          // What can be allowed, filled in the way the app fills it: only what
          // this teammate would ever ask about, places whole, programs by name,
          // and the whole of a kind last.
          case "allowing_choices": {
            const agent = (fixture.agents || []).find((x) => x.id === args.agent) || {};
            const asks = postureNow[args.agent] || agent.asks || "ask";
            const local = agent.engine === "local";
            const kinds =
              asks === "auto"
                ? ["folder"]
                : local
                  ? ["commands", "writing", "changing", "folder"]
                  : ["commands", "writing", "changing", "reading", "fetching"];
            const places = [
              { rule: "/Users/me/Downloads", said: "Downloads (/Users/me/Downloads)" },
              { rule: "/Volumes/Archive", said: "the disk Archive (/Volumes/Archive)" },
            ];
            const using = {
              commands: "running commands",
              writing: "writing files",
              changing: "changing files",
              reading: "reading files",
              fetching: "fetching web pages",
              folder: "a folder it may write in",
            };
            const whole = {
              commands: "running any command at all",
              writing: "writing any file",
              changing: "changing any file",
              reading: "reading any file",
              fetching: "fetching any web page",
            };
            return Promise.resolve({
              kinds: kinds.map((kind) => ({
                kind,
                using: using[kind],
                a_place: ["folder", "writing", "changing", "reading"].includes(kind),
                to_type:
                  kind === "commands"
                    ? "a program, like curl, or a whole command"
                    : kind === "fetching"
                      ? "the start of a web address, like https://github.com"
                      : "a folder's whole path, starting with /",
                choices: [
                  ...(kind === "commands"
                    ? ["curl", "git"].map((p) => ({ rule: p, said: `any ${p} command` }))
                    : kind === "fetching"
                      ? []
                      : places),
                  ...(kind === "folder" ? [] : [{ rule: "", said: whole[kind] }]),
                ],
              })),
              fewer:
                asks === "auto"
                  ? "It never asks before doing anything, so the one thing to allow is somewhere else to write."
                  : null,
            });
          }
          // The next thing to say, as the teammate's model would suggest it.
          case "suggest_next":
            return Promise.resolve(
              fixture.suggestion === undefined ? "Yes, go ahead with the bigger disk." : fixture.suggestion,
            );
          // The Mac's own folder chooser, answered as if somebody chose a
          // folder, or as if they cancelled when a check says so.
          case "choose_a_folder":
            return Promise.resolve(
              fixture.folderChosen === undefined ? "/Users/me/Projects/Clips" : fixture.folderChosen,
            );
          // Said before anything is kept, in the words the list will use.
          case "what_allowing_means": {
            const rule = String(args.rule || "").trim();
            if (["folder", "writing", "changing", "reading"].includes(args.tool) && rule && !rule.startsWith("/")) {
              return Promise.reject("give the folder's whole path, starting with /");
            }
            if (args.tool === "folder") return Promise.resolve(`writing anywhere inside ${rule}`);
            if (args.tool === "commands") {
              return Promise.resolve(rule ? `any ${rule.split(/\s+/)[0]} command` : "running any command at all");
            }
            return Promise.resolve(rule ? `anything starting ${rule}` : `${args.tool} of anything`);
          }
          // What an agent remembers, and the same refusal the app gives a note
          // that holds a key.
          case "notes":
            return Promise.resolve(fixture.notes?.[args.agent] || []);
          // A fresh copy each time, as the app's answer is: the window keeps
          // the last one to tell whether anything changed, and handed the same
          // object it would compare it with itself.
          case "standing":
            return Promise.resolve(JSON.parse(JSON.stringify(fixture.standing || [])));
          // Finishing a task switches off what it runs on its own, and
          // reopening it switches back on what finishing switched off, as the
          // app does.
          case "finish_task":
            for (const s of fixture.standing || []) {
              if (s.conversation !== args.id) continue;
              const on = s.kind === "routine" ? !s.off : !s.stopped;
              if (args.finished && on) {
                s.offWhenFinished = true;
                if (s.kind === "routine") s.off = true;
                else s.stopped = "Stopped because its task was marked finished.";
              } else if (!args.finished && s.offWhenFinished) {
                delete s.offWhenFinished;
                if (s.kind === "routine") s.off = false;
                else s.stopped = null;
              }
            }
            return Promise.resolve(null);
          case "limits":
            return Promise.resolve(
              fixture.limits?.[args.agent] || { tokens: null, dollars: null, used_tokens: 0, spent_dollars: 0 },
            );
          // A copy, and one loaded from a file, made the way the app makes
          // them: a new agent with a first conversation of its own.
          case "duplicate":
          case "load_agent": {
            const from = fixture.agents.find((a) => a.id === args.id) || fixture.agents[0];
            const id = `${name === "duplicate" ? "agent-copy" : "agent-loaded"}-${fixture.agents.length}`;
            fixture.agents.push({
              ...from,
              id,
              name: name === "duplicate" ? `${from.name} copy` : "Loaded Scout",
              pinned: false,
              hidden: false,
              paused_at: null,
            });
            fixture.conversations[id] = [{ id, agent: id, name: "First", opened: false }];
            return Promise.resolve(id);
          }
          // Who is in a room, changed the way the app changes it.
          case "set_members": {
            fixture.members[args.room] = args.agents.map((id) => ({
              agent: id,
              name: fixture.agents.find((a) => a.id === id)?.name || id,
              talk: null,
            }));
            return Promise.resolve({ name: "Bitcoin room", members: fixture.members[args.room] });
          }
          case "save_agent":
            return Promise.resolve("/Users/you/Desktop/Bitcoin Desk.errand.json");
          case "skills_of":
            return Promise.resolve(fixture.skills?.[args.agent] || []);
          // A run is a conversation of its own, made here the way the app
          // makes one, so the window can go to it.
          case "run_a_skill": {
            const id = `talk-skill-${(fixture.conversations[args.agent] || []).length}`;
            (fixture.conversations[args.agent] ||= []).push({
              id,
              agent: args.agent,
              name: `Skill: ${args.name}`,
              opened: true,
            });
            return Promise.resolve(id);
          }
          case "note_down":
            return /sk-[A-Za-z0-9]{20,}/.test(args.note || "")
              ? Promise.reject("that looks like a password or a key, and notes are read into every conversation. Leave it out.")
              : Promise.resolve(null);
          // What the engine allows out of its own settings files, which this
          // app can show and cannot take back.
          case "also_allowed":
            // A check can put a different arrangement here, since the ones
            // worth checking are the ones no amount of clicking can produce.
            return Promise.resolve(window.__ALSO_MODE__ ?? fixture.also_allowed);
          case "what_it_cost":
            return Promise.resolve(fixture.what_it_cost);
          case "already_runs":
            // The stand-in for the app's own comparison: same time, and the
            // words mostly the same.
            return Promise.resolve(
              args.at === "daily 07:00" && /bitcoin|brief/i.test(args.what || "")
                ? "Bitcoin Desk already does almost exactly this at daily 07:00."
                : null,
            );
          case "__never":
          // One routine, on the conversation the checks open, so that pausing
          // has something to pause. Answered live rather than from a constant,
          // because the behaviour under test is that the switch sticks.
          case "routines":
            return Promise.resolve([
              {
                conversation: "talk-2",
                agent: "agent-bitcoin",
                name: "First",
                at: "daily 07:00",
                what: "What moved overnight",
                // A check can say when the app now puts the next run, as
                // it does once a paused routine is started again.
                due: fixture.routineDue ?? Date.now() + 3600000,
                ran: Date.now() - 82800000,
                off: routineOff,
              },
            ]);
          case "runs":
            return Promise.resolve([]);
          case "happened_since":
            return Promise.resolve(fixture.happened);
          case "setting":
            return Promise.resolve(fixture.settings[args?.key] ?? null);
          // What the key agent holds, and loading a key into it, which the
          // app does with ssh-add and the passphrase window macOS puts up.
          case "ssh_key":
            return Promise.resolve({ ...fixture.sshKey, at_start: fixture.settings.ssh_keys_at_start === "on" });
          case "load_ssh_key": {
            const loaded = fixture.sshLoad;
            if (!loaded.why_not) fixture.sshKey = { ...fixture.sshKey, holds: loaded.holds };
            return Promise.resolve(loaded);
          }
          // Refused the way the app refuses it, so the window's handling of a
          // refusal is what gets checked.
          case "set_setting":
            if (args?.key === "ssh_keys_at_start") {
              if (!["on", "off"].includes(args.value)) return Promise.reject("That is on or off.");
              fixture.settings[args.key] = args.value;
              return Promise.resolve(null);
            }
            if (args?.key === "local_model") {
              if (!fixture.offered.some((o) => o.id === args.value && servedHere(o))) {
                return Promise.reject("That model is not served on this Mac or your network, so it cannot be the local one.");
              }
              fixture.settings[args.key] = args.value;
              return Promise.resolve(null);
            }
            if (args?.key === "errand_model") {
              if (!fixture.offered.some((o) => o.id === args.value)) {
                return Promise.reject("That model is not in the list to choose from.");
              }
              fixture.settings[args.key] = args.value;
              return Promise.resolve(null);
            }
            if (!(Number(args?.value) >= 1 && Number(args?.value) <= 365)) {
              return Promise.reject("somewhere between 1 and 365 days");
            }
            fixture.settings[args.key] = args.value;
            return Promise.resolve(null);
          // Everything else is a thing done rather than asked, and the window
          // only cares that it did not fail.
          default:
            return Promise.resolve(null);
        }
      },
    },
    event: {
      // Nothing arrives on its own in the harness, but what would arrive can
      // be delivered on purpose. Kept rather than discarded so a check can
      // send the page an event and watch what it does with it: several things
      // the window only ever learns about this way had no test at all while
      // this returned a shrug.
      listen: (name, fn) => {
        (listeners[name] ||= []).push(fn);
        return Promise.resolve(() => {
          listeners[name] = (listeners[name] || []).filter((f) => f !== fn);
        });
      },
    },
  };
}
