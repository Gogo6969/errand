// A picture of what the errand is about.
//
// A list of threads all wearing the same dot is a list you have to read. Give
// each one a mark for the kind of thing it is and the same list can be scanned:
// the envelope is the mail one, the clock is the one that runs every morning.
//
// Drawn here rather than fetched, because sixteen line drawings are smaller
// than the request that would go and get them, and an icon that arrives late is
// worse than no icon at all -- the row moves under the pointer.
//
// The kind is guessed from what was asked, in the person's own words, which is
// all there is to go on before anything has happened. A guess is fine: the
// wrong drawing beside the right words costs nothing, and the right one saves a
// read. Nothing is ever hidden behind an icon, so nothing is lost when it is
// wrong.

const stroke =
  'fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"';

/** The drawings, each 16x16 and each one path or a few. */
const DRAWN = {
  mail: `<path d="M2 4.5h12v7H2z"/><path d="m2 5 6 4 6-4"/>`,
  clock: `<circle cx="8" cy="8" r="6"/><path d="M8 4.7V8l2.4 1.6"/>`,
  chart: `<path d="M2 13h12"/><path d="m3 10 3-3 2.5 2L13 4"/><path d="M13 7V4h-3"/>`,
  search: `<circle cx="7" cy="7" r="4.2"/><path d="m10.2 10.2 3.3 3.3"/>`,
  folder: `<path d="M2 4.5h4l1.4 1.6H14v6.4H2z"/>`,
  image: `<path d="M2.5 3.5h11v9h-11z"/><circle cx="6" cy="6.6" r="1.1"/><path d="m3 11 3.2-3 2.4 2.2L11 8l2.5 2.4"/>`,
  code: `<path d="m5.5 5-3 3 3 3"/><path d="m10.5 5 3 3-3 3"/>`,
  globe: `<circle cx="8" cy="8" r="6"/><path d="M2 8h12"/><path d="M8 2c1.7 1.8 2.6 3.8 2.6 6S9.7 12.2 8 14C6.3 12.2 5.4 10.2 5.4 8S6.3 3.8 8 2Z"/>`,
  bag: `<path d="M3 5.5h10l-.8 8H3.8z"/><path d="M6 5.5V4a2 2 0 0 1 4 0v1.5"/>`,
  pen: `<path d="M11.2 2.6 13.4 4.8 5.6 12.6l-3 .8.8-3z"/>`,
  chat: `<path d="M2.5 4h11v7h-6l-3.5 2.5V11h-1.5z"/>`,
  person: `<circle cx="8" cy="5.6" r="2.6"/><path d="M3 13.4c0-2.4 2.2-3.8 5-3.8s5 1.4 5 3.8"/>`,
  list: `<path d="M6 4.5h8"/><path d="M6 8h8"/><path d="M6 11.5h8"/><path d="M2.6 4.5h.01"/><path d="M2.6 8h.01"/><path d="M2.6 11.5h.01"/>`,
  terminal: `<path d="M2.5 3.5h11v9h-11z"/><path d="m5 7 1.8 1.6L5 10.2"/><path d="M8.8 10.4H11"/>`,
  helper: `<circle cx="5.6" cy="6" r="2.2"/><circle cx="10.8" cy="6.8" r="1.7"/><path d="M1.8 13c0-2 1.7-3.2 3.8-3.2S9.4 11 9.4 13"/><path d="M10.6 10c1.9 0 3.6.9 3.6 3"/>`,
  spark: `<path d="M8 2.2 9.5 6.5 13.8 8 9.5 9.5 8 13.8 6.5 9.5 2.2 8 6.5 6.5z"/>`,
  bell: `<path d="M4.4 11.2V7.6a3.6 3.6 0 0 1 7.2 0v3.6l1.1 1.4H3.3z"/><path d="M6.6 12.6a1.5 1.5 0 0 0 2.8 0"/>`,
};

/**
 * What kind of errand this is, from what was asked.
 *
 * First match wins, so the order is the order of specificity: "mail me the
 * chart every morning" is a routine, and the clock says more about it than the
 * envelope does. Two words that are the same word in different jobs -- "book" a
 * table, "book" to read -- are left to the more common one rather than guessed.
 */
const SOUNDS_LIKE = [
  ["clock", /\b(every|each|daily|hourly|weekly|morning|evening|schedule|routine|remind|recurring|at \d|cron)\b/],
  ["mail", /\b(mail|email|e-mail|inbox|newsletter|unread|reply|cc|imap)\b/],
  ["chat", /\b(messages?|imessage|text|sms|slack|whatsapp|dm|telegram)\b/],
  ["chart", /\b(price|stock|market|bitcoin|crypto|ticker|revenue|metric|chart|graph|report|sales|news)\b/],
  ["bag", /\b(buy|order|purchase|shop|cart|reserve|tickets?|subscribe|invoice|bill|flights?|hotels?|book a|restaurant)\b/],
  ["code", /\b(code|repo|git|build|compile|deploy|bug|test|refactor|api|script|function)\b/],
  ["image", /\b(images?|photos?|pictures?|screenshots?|png|jpe?g|icons?|logos?|thumbnails?|videos?)\b/],
  ["folder", /\b(files?|folders?|rename|organi[sz]e|tidy|backup|downloads?|move|sort|pdf|csv)\b/],
  ["globe", /\b(website|site|web|browse|url|http|page|scrape|domain|online)\b/],
  ["person", /\b(contacts?|people|friends?|colleagues?|who is|who's|profile|followers?|account)\b/],
  ["pen", /\b(write|draft|note|summari[sz]e|rewrite|blog|post|essay|letter|translate)\b/],
  ["search", /\b(find|search|look up|research|compare|check|investigate|latest|what is)\b/],
];

/** The kind of errand some words describe. */
export function kindOf(words) {
  const said = (words || "").toLowerCase();
  for (const [kind, sounds] of SOUNDS_LIKE) if (sounds.test(said)) return kind;
  return "spark";
}

/** One drawing, as markup, for a kind or for anything that has a kind. */
export function icon(kind) {
  return `<svg viewBox="0 0 16 16" width="16" height="16" ${stroke} aria-hidden="true">${
    DRAWN[kind] || DRAWN.spark
  }</svg>`;
}

/**
 * The mark for a thread, in a tile that can show it is working.
 *
 * The working state lives on the tile rather than beside the row because a
 * spinner in the margin is one more thing on the screen, and a ring around the
 * mark that is already there is none. It is the same ring for every kind, so
 * "this one is busy" reads at a glance across the whole list.
 */
export function tile(kind, working, hue) {
  const box = document.createElement("span");
  box.className = `tile ${working ? "busy" : ""}`;
  box.dataset.kind = kind;
  // The colour an agent chose for itself, where it chose one. Set inline
  // because it beats the mark's own default, which is the whole point: two
  // agents doing mail should be able to look different.
  if (hue && HUES[hue]) box.style.setProperty("--hue", HUES[hue]);
  box.innerHTML = icon(kind);
  return box;
}

/// The colours an agent may pick from, by the names it is offered.
const HUES = {
  amber: "#e8a850",
  blue: "#6fa8f5",
  green: "#7fd07a",
  purple: "#c4a2ef",
  teal: "#63c7a6",
  rose: "#e089b4",
  gold: "#e8c250",
};

/**
 * The mark for one step, from the tool doing it.
 *
 * Named tools first, then anything an MCP server brought along, which is where
 * mail, messages and notes come from on this machine and is why they are worth
 * recognising by name rather than all becoming the same cog.
 */
export function forTool(tool) {
  const name = tool || "";
  const known = {
    Bash: "terminal",
    Read: "folder",
    Glob: "folder",
    Grep: "search",
    Write: "pen",
    Edit: "pen",
    NotebookEdit: "pen",
    WebSearch: "search",
    WebFetch: "globe",
    Task: "helper",
    Skill: "spark",
    TodoWrite: "list",
    ToolSearch: "search",
  };
  if (known[name]) return known[name];
  if (/cron|schedul/i.test(name)) return "clock";
  if (/mail/i.test(name)) return "mail";
  if (/message|imessage|slack/i.test(name)) return "chat";
  if (/note/i.test(name)) return "pen";
  if (/browser|chrome|navigate|playwright/i.test(name)) return "globe";
  if (/notif/i.test(name)) return "bell";
  return "spark";
}
