// What the agent wrote, as something to read.
//
// Models write Markdown whether or not anybody asked them to, so a window that
// shows their text verbatim shows people `[Yahoo Finance](https://…)` and a
// wall of pipes where a table should be. This turns the subset that actually
// turns up into elements.
//
// Everything here builds DOM nodes. Nothing is ever assigned as HTML, and that
// is not a style preference: this text arrives from a model, and a model
// repeats what it read on a web page an hour ago. A renderer that accepted
// markup would be an injection hole with a nice font. The cost of the rule is
// that unsupported syntax appears as its own source, which is a fair trade:
// wrong-looking is recoverable, and a page that runs somebody else's script is
// not.
//
// The subset is what agents actually emit: headings, paragraphs, lists, links,
// inline and fenced code, tables, quotes, rules, bold and italic. Anything else
// falls through as text rather than being half-supported.

/**
 * How this module reaches the app, for the two things it cannot do itself.
 *
 * Drawing a picture an agent made means reading a file, and revealing one in
 * Finder means asking the system: both are the app's to do and neither is the
 * window's. Handed in rather than imported, so this file stays a renderer that
 * turns text into nodes and can still be tested without an app behind it.
 */
let ask = async () => {
    throw new Error("nothing is wired up to answer that yet");
};

/** Tell this module how to reach the app. Called once, by the window. */
export function reachTheAppWith(invoke) {
    ask = invoke;
}

/** Everything in `text`, as nodes ready to be put on the page. */
export function render(text) {
  const out = document.createDocumentFragment();
  const lines = String(text ?? "").replace(/\r\n?/g, "\n").split("\n");

  let at = 0;
  while (at < lines.length) {
    const line = lines[at];

    if (!line.trim()) {
      at++;
      continue;
    }

    // Fenced code. Taken whole and never looked inside, because the point of a
    // fence is that what is in it is not Markdown.
    const fence = line.match(/^\s*(`{3,}|~{3,})\s*([\w+-]*)\s*$/);
    if (fence) {
      const [, marks, language] = fence;
      const body = [];
      at++;
      while (at < lines.length && !lines[at].match(new RegExp(`^\\s*${marks[0]}{3,}\\s*$`))) {
        body.push(lines[at]);
        at++;
      }
      at++; // the closing fence, or the end of the text
      out.append(fenced(body.join("\n"), language));
      continue;
    }

    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      const node = document.createElement(`h${heading[1].length}`);
      node.append(inline(heading[2]));
      out.append(node);
      at++;
      continue;
    }

    if (/^\s*([-*_])\s*\1\s*\1[\s\1]*$/.test(line)) {
      out.append(document.createElement("hr"));
      at++;
      continue;
    }

    // A table is only a table if the second line is the separator. Without
    // that check, any paragraph containing a pipe becomes one.
    if (line.includes("|") && at + 1 < lines.length && isSeparator(lines[at + 1])) {
      const rows = [];
      while (at < lines.length && lines[at].includes("|")) {
        rows.push(lines[at]);
        at++;
      }
      out.append(tabulate(rows));
      continue;
    }

    if (/^\s*>/.test(line)) {
      const said = [];
      while (at < lines.length && /^\s*>/.test(lines[at])) {
        said.push(lines[at].replace(/^\s*>\s?/, ""));
        at++;
      }
      const quote = document.createElement("blockquote");
      quote.append(render(said.join("\n")));
      out.append(quote);
      continue;
    }

    if (bulletOf(line) !== null) {
      const ordered = /^\s*\d+[.)]\s/.test(line);
      const list = document.createElement(ordered ? "ol" : "ul");
      while (at < lines.length && bulletOf(lines[at]) !== null) {
        const item = document.createElement("li");
        const first = bulletOf(lines[at]);
        at++;
        // Anything indented under a bullet belongs to it, including the
        // wrapped remainder of a long line.
        const more = [];
        while (at < lines.length && /^\s{2,}\S/.test(lines[at]) && bulletOf(lines[at]) === null) {
          more.push(lines[at].trim());
          at++;
        }
        item.append(inline([first, ...more].join(" ")));
        list.append(item);
      }
      out.append(list);
      continue;
    }

    // Anything else is a paragraph, running until a blank line or something
    // that is plainly the start of a block.
    const said = [];
    while (at < lines.length && lines[at].trim() && !startsSomething(lines[at])) {
      said.push(lines[at].trim());
      at++;
    }
    const p = document.createElement("p");
    p.append(inline(said.join(" ")));
    out.append(p);
  }

  return out;
}

/** The text of a list item, or null when this line is not one. */
function bulletOf(line) {
  const hit = line.match(/^\s*(?:[-*+]|\d+[.)])\s+(.*)$/);
  return hit ? hit[1] : null;
}

/** Would this line begin a block of its own? */
function startsSomething(line) {
  return (
    /^\s*(`{3,}|~{3,})/.test(line) ||
    /^#{1,6}\s/.test(line) ||
    /^\s*>/.test(line) ||
    bulletOf(line) !== null
  );
}

function isSeparator(line) {
  return /^\s*\|?[\s:|-]*-[\s:|-]*\|?\s*$/.test(line) && line.includes("-");
}

/** One fenced block, with its language kept for the label. */
function fenced(code, language) {
  const pre = document.createElement("pre");
  pre.className = "code";
  if (language) pre.dataset.language = language;
  const inner = document.createElement("code");
  inner.textContent = code;
  pre.append(inner);
  return pre;
}

/** A table, from its rows. */
function tabulate(rows) {
  const cellsOf = (line) =>
    line
      .trim()
      .replace(/^\|/, "")
      .replace(/\|$/, "")
      .split("|")
      .map((c) => c.trim());

  const wrap = document.createElement("div");
  wrap.className = "table-wrap";
  const table = document.createElement("table");

  const head = document.createElement("thead");
  const headRow = document.createElement("tr");
  for (const cell of cellsOf(rows[0])) {
    const th = document.createElement("th");
    th.append(inline(cell));
    headRow.append(th);
  }
  head.append(headRow);
  table.append(head);

  const body = document.createElement("tbody");
  // Row 1 is the separator, which is punctuation rather than content.
  for (const line of rows.slice(2)) {
    const tr = document.createElement("tr");
    for (const cell of cellsOf(line)) {
      const td = document.createElement("td");
      td.append(inline(cell));
      tr.append(td);
    }
    body.append(tr);
  }
  table.append(body);
  wrap.append(table);
  return wrap;
}

/**
 * The marks that happen inside a line.
 *
 * Code first and unconditionally, because whatever is inside backticks is not
 * Markdown and must not be read as any. After that, links before emphasis, so
 * that a URL with an underscore in it stays a URL.
 */
function inline(text) {
  const out = document.createDocumentFragment();
  //
  // The back-references are numbered for their place in the WHOLE pattern, not
  // in the line they are written on, and getting that wrong is silent: `\1` in
  // the bold rule pointed at the backtick group, which never participates when
  // bold matches, so it matched the empty string and `**Price:**` rendered as a
  // bold "P" followed by a stray asterisk. Counting is the price of one regex
  // instead of five passes, and the tests beside this file are what make it
  // safe to change.
  const pattern = new RegExp(
    [
      "(`+)([\\s\\S]+?)\\1", // 1,2   `code`
      "\\[([^\\]]*)\\]\\(([^)\\s]+)[^)]*\\)", // 3,4   [text](url)
      "(\\*\\*|__)(?=\\S)([\\s\\S]*?\\S)\\5", // 5,6   **bold**
      "(\\*|_)(?=\\S)([\\s\\S]*?\\S)\\7", // 7,8   *italic*
      "(https?://[^\\s<>\\])]+)", // 9     a bare address
      // 10,11  ![alt](src). Last on purpose: adding it earlier would renumber
      // every back-reference above, which the note above says is silent when it
      // goes wrong. At the position of the `!` no other alternative can match,
      // so being last costs it nothing.
      "!\\[([^\\]]*)\\]\\(([^)\\s]+)[^)]*\\)",
    ].join("|"),
    "g",
  );

  let from = 0;
  for (const hit of text.matchAll(pattern)) {
    if (hit.index > from) out.append(text.slice(from, hit.index));
    const [whole, , code, label, href, , strong, , stress, bare, alt, src] = hit;

    if (src !== undefined) {
      out.append(picture(src, alt));
    } else if (code !== undefined) {
      const node = document.createElement("code");
      node.textContent = code;
      out.append(node);
    } else if (href !== undefined) {
      out.append(link(href, label || href, whole));
    } else if (strong !== undefined) {
      const node = document.createElement("strong");
      node.append(inline(strong));
      out.append(node);
    } else if (stress !== undefined) {
      const node = document.createElement("em");
      node.append(inline(stress));
      out.append(node);
    } else if (bare !== undefined) {
      out.append(link(bare, bare, whole));
    }
    from = hit.index + whole.length;
  }
  if (from < text.length) out.append(text.slice(from));
  return out;
}

/**
 * A link, if it is one of the kinds worth following.
 *
 * Only the three schemes a person would recognise. Anything else -- and
 * `javascript:` is the one that matters -- is shown as the text it is, so a
 * model that read a poisoned page cannot put a trap in a thread.
 */
/**
 * A picture an answer points at.
 *
 * An agent that fetches or draws something writes `![it](/some/path)`, and
 * until now that fell through to the link rule, which takes http and https and
 * hands everything else back as its own text. So an answer said "here is the
 * picture" and showed a path, and the agent learnt to apologise for it in
 * prose: "both are downloaded locally if the images don't render for you".
 *
 * A path is read through the app rather than put in `src` directly. The window
 * cannot open a file for itself, and the command that does is bounded to the
 * places agents actually work.
 */
function picture(src, alt) {
  const img = document.createElement("img");
  img.className = "drawn";
  img.alt = alt || "";
  img.loading = "lazy";
  if (/^https?:/i.test(src)) {
    img.src = src;
  } else {
    ask("a_local_picture", { path: src })
      .then((url) => {
        img.src = url;
      })
      .catch((why) => {
        // Said, and the path kept. A picture that will not load must not take
        // the only way of getting at the file down with it.
        const gone = document.createElement("span");
        gone.className = "picture-gone";
        gone.textContent = String(why);
        const where = document.createElement("code");
        where.textContent = src;
        img.replaceWith(gone, where);
      });
  }
  img.onclick = () => {
    if (img.src) window.dispatchEvent(new CustomEvent("look-closer", { detail: { url: img.src, name: alt } }));
  };
  return img;
}

function link(href, label, original) {
  const safe = /^(https?:|mailto:)/i.test(href.trim());
  // A path is not a scheme and is not a trap: revealing one in Finder cannot
  // run anything. "I saved the report to ~/Desktop/report.pdf" is the end of
  // most errands, and it was a dead end.
  if (!safe && /^([~/]|file:\/\/)/.test(href.trim())) {
    const a = document.createElement("a");
    a.href = "#";
    a.textContent = label;
    a.className = "on-disk";
    a.title = "Show it in Finder";
    a.onclick = (e) => {
      e.preventDefault();
      ask("show_in_finder", { path: href.replace(/^file:\/\//, "") }).catch(() => {});
    };
    return a;
  }
  if (!safe) return document.createTextNode(original);
  const a = document.createElement("a");
  a.href = href;
  a.textContent = label;
  a.rel = "noreferrer noopener";
  // Handled by the page, not the webview: following it here would replace the
  // app with a web page and there would be no way back to the thread.
  a.dataset.away = "true";
  return a;
}
