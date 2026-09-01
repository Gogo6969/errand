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
    ],
  },
  // How a routine has been going: one good morning, one failed, and one that
  // never came back because the machine slept. The three states the panel has
  // to be able to tell apart.
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
    "talk-1": [
      { seq: 1, at: 1, kind: "mine", text: "Show me the latest Bitcoin news", call: null, tool: null, outcome: null },
      { seq: 2, at: 2, kind: "said", text: "**BTC** is around $77,700.", call: null, tool: null, outcome: null },
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
  ],
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
  /** What is being watched, once anything has set or stopped it. */
  let watchedNow = null;
  /** Which connectors are switched on, once anything has switched one. */
  const connected = new Set();
  /**
   * What each agent has said that nobody has read, which a check clears by
   * opening the conversation it is in. Mutable, because the behaviour under
   * test is that it goes away: a fixture answering the same thing twice cannot
   * tell a mark that clears from one that was never drawn.
   */
  const unread = new Map([["agent-bitcoin", { lines: 2, at: Date.now() - 3600000 }]]);
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
      unread.set(agent, { lines, at });
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
            return Promise.resolve(args.id === fixture.liveConversation);
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
          case "what_is_new":
            return Promise.resolve(Object.fromEntries(unread));
          case "seen": {
            const owner = Object.entries(fixture.conversations).find(([, talks]) =>
              talks.some((t) => t.id === args.conversation),
            )?.[0];
            if (owner) unread.delete(owner);
            return Promise.resolve(null);
          }
          case "forget_conversation":
          case "looking_at":
            return Promise.resolve(null);
          // A routine switched off rather than thrown away, and what it did.
          case "routine_off":
            routineOff = !!args.off;
            return Promise.resolve(null);
          case "how_it_has_been_going":
            return Promise.resolve(fixture.went || []);
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
          case "goal_of":
            return Promise.resolve(fixture.goal_of);
          case "allowances":
            return Promise.resolve(fixture.allowances);
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
                due: Date.now() + 3600000,
                ran: Date.now() - 82800000,
                off: routineOff,
              },
            ]);
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
