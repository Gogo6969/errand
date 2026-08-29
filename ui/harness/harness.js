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
    ],
  },
  lines: {
    "talk-1": [
      { seq: 1, at: 1, kind: "mine", text: "Show me the latest Bitcoin news", call: null, tool: null, outcome: null },
      { seq: 2, at: 2, kind: "said", text: "**BTC** is around $77,700.", call: null, tool: null, outcome: null },
    ],
    "talk-2": [],
    "talk-3": [],
  },
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
  outside: [
    { name: "peekaboo", from: "~/.claude.json", tools: ["see", "click", "type"], trouble: null },
    { name: "mempalace", from: "~/.claude.json", tools: [], trouble: "starting it: No such file or directory" },
  ],
};

/** Everything the window asked for, so a check can say what was never called. */
export const asked = [];

/**
 * Stand in for the app, answering the way it does.
 *
 * `slowly` holds commands that take a while, which is not a detail: opening a
 * conversation starts an engine and binds a socket, and the window has to be
 * readable before that finishes rather than after.
 */
export function standIn(fixture = FIXTURE, breaking = {}, slowly = {}) {
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
            return Promise.resolve(fixture.engines);
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
          case "allowances":
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
      // Nothing arrives on its own in the harness. Returning the unlisten
      // function the real one returns, because the page keeps it.
      listen: () => Promise.resolve(() => {}),
    },
  };
}
