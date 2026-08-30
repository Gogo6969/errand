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
    ],
    "agent-bitcoin": [
      { id: "talk-2", agent: "agent-bitcoin", name: "First", opened: true },
      { id: "talk-3", agent: "agent-bitcoin", name: "Asked by Day Check", opened: true },
      { id: "talk-4", agent: "agent-bitcoin", name: "Answered a few", opened: true },
    ],
  },
  lines: {
    "talk-1": [
      { seq: 1, at: 1, kind: "mine", text: "Show me the latest Bitcoin news", call: null, tool: null, outcome: null },
      { seq: 2, at: 2, kind: "said", text: "**BTC** is around $77,700.", call: null, tool: null, outcome: null },
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
  liveConversation: "talk-3",
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
  ],
  whats_running: [
    { conversation: "talk-3", agent: "agent-bitcoin", who: "Bitcoin Desk", talk: "Asked by Day Check",
      what: "Waiting on you: Running a command", waiting: true },
    { conversation: "talk-2", agent: "agent-bitcoin", who: "Bitcoin Desk", talk: "First",
      what: "Looking something up on the web", waiting: false },
    { conversation: "talk-2", agent: "", who: "Bitcoin Desk", talk: "First",
      what: "Building the thing", waiting: false, command: "job-1" },
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
    means: "This looks at ~/Downloads every 10 minutes and wakes Bitcoin Desk when what is there changes. It compares the names and sizes of the files one level down, ignoring part-downloaded ones. At most once every 15 minutes, and at most 24 times a day. It only looks while Errand is open, so something that changes overnight is something you hear about in the morning.",
    looked_at: 1788000000000,
    woke_at: null,
    woke_today: 0,
    misses: 0,
    paused: null,
  },
  goal_of: {
    goal: "Get the tests passing",
    means:
      "The agent works towards this on its own and says at the end of every turn whether it is done. " +
      "It gets at most 8 turns, it stops early if it says the same thing is left twice running, and it " +
      "stops if it stops reporting at all. It only runs while Errand is open. The goal is: Get the tests passing",
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
  },
  what_it_cost: {
    today: [{ agent: "agent-bitcoin", who: "Bitcoin Desk", dollars: 0.19, turns: 1, errands: 1 }],
    this_month: [
      { agent: "agent-bitcoin", who: "Bitcoin Desk", dollars: 4.2, turns: 30, errands: 12 },
      { agent: "agent-gone", who: "an agent that is gone", dollars: 0.5, turns: 2, errands: 2 },
    ],
    nothing_yet: false,
  },
  outside: [
    { name: "peekaboo", from: "~/.claude.json", tools: ["see", "click", "type"], trouble: null },
    { name: "mempalace", from: "~/.claude.json", tools: [], trouble: "starting it: No such file or directory" },
  ],
};

/** Everything the window asked for, so a check can say what was never called. */
export const asked = [];

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
  return {
    core: {
      invoke(name, args) {
        asked.push({ name, args });
        if (breaking[name]) return Promise.reject(breaking[name]);
        if (slowly[name]) {
          return new Promise((go) => setTimeout(() => go(null), slowly[name]));
        }
        switch (name) {
          case "agents":
          case "matching":
            return Promise.resolve(fixture.agents);
          case "conversations":
            return Promise.resolve(fixture.conversations[args.agent] || []);
          case "lines":
            return Promise.resolve(fixture.lines[args.id] || []);
          case "engines":
            return Promise.resolve(
              fixture.offered.map((o) => ({ engine: o.engine, name: o.label, settings: o.settings })),
            );
          case "outside":
            return Promise.resolve(fixture.outside);
          case "checkup":
            return Promise.resolve(fixture.checkup);
          case "whats_running":
            return Promise.resolve(fixture.whats_running);
          case "brought":
            return Promise.resolve(fixture.brought);
          case "watches":
            return Promise.resolve(fixture.watches);
          // The conversation with the live question in it is live; the rest
          // are history. Which is the whole distinction being tested.
          case "still_going":
            return Promise.resolve(args.id === fixture.liveConversation);
          case "whats_offered":
            return Promise.resolve(fixture.offered);
          // Whether the app starts itself at login. Kept here rather than in
          // the fixture because the switch changes it: what it says has to be
          // what was last set, or the check cannot tell a switch that works
          // from one that only looks like it does.
          case "opens_at_login":
            return Promise.resolve(atLogin);
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
          case "goal_of":
            return Promise.resolve(fixture.goal_of);
          case "allowances":
            return Promise.resolve(fixture.allowances);
          // What the engine allows out of its own settings files, which this
          // app can show and cannot take back.
          case "also_allowed":
            return Promise.resolve(fixture.also_allowed);
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
          case "routines":
          case "runs":
            return Promise.resolve([]);
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
