//! Where threads live when the window is shut.
//!
//! A conversation that vanishes when the app closes is a conversation nobody
//! trusts with anything that matters, and this one is meant to be trusted with
//! errands that run at seven in the morning. So the thread and everything said
//! in it are written down as they happen.
//!
//! SQLite, through rusqlite with the bundled library, for reasons that were
//! measured rather than preferred: it is eight crates where sqlx is
//! seventy-four, it needs no tooling the compiler is not already using, and the
//! app still builds with cargo alone and nothing else -- which is a property
//! worth keeping and easy to lose.
//!
//! One connection behind one lock. An append is microseconds, this is a desktop
//! app with one person in it, and a lock that is never contended is simpler than
//! a writer thread and gives the same thing that actually matters: the sequence
//! number is handed out inside the same lock that does the insert, so two
//! writers cannot produce the same position. That ordering is the whole point.
//! Events arrive on a background thread while the person's own messages
//! originate in the window, and if those two can interleave badly the stored
//! conversation is not the one anybody had.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::engine::Event;

/// What an agent is called before it has worked out what it is for.
///
/// One string in one place, because it is checked as well as written: the
/// naming only replaces a name nobody has chosen, and comparing against a
/// second copy of this spelled slightly differently is how that quietly stops
/// working.
pub const NOT_YET_NAMED: &str = "New errand";

/// What an agent decided it was, once it knew.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settled {
    pub name: String,
    pub title: String,
    pub about: String,
    /// One of the marks the window knows how to draw.
    pub mark: String,
    pub hue: String,
}

/// Something an agent may do without asking again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allowance {
    pub id: String,
    pub tool: String,
    /// The beginning of what it may do, or empty for any use of the tool.
    pub rule: String,
    pub said_at: i64,
}

/// One conversation with an agent.
///
/// The unit an engine session belongs to. Its id *is* the session id: Claude
/// Code is started with it and resumed with it, and its transcript is filed
/// under it. So a conversation id is never reused and never regenerated once
/// anything has been said, or the conversation becomes unreachable while its
/// transcript sits on disk under a name nothing will ask for again.
/// Where some words were found, and in which conversation.
#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub agent: String,
    pub conversation: String,
    /// Which line, so the window can scroll to it and mark it.
    pub seq: i64,
    pub kind: String,
    /// Enough of the line to recognise it by, around the match.
    pub snippet: String,
}

/// One time a routine ran, and what came of it.
#[derive(Debug, Clone, Serialize)]
pub struct Run {
    pub at: i64,
    /// What started it: the clock, a watch, or somebody pressing Try it now.
    pub why: String,
    /// How it ended. Nothing means it never came back, which is its own
    /// outcome: the app was quit, or the machine went to sleep.
    pub outcome: Option<String>,
}

/// What one agent has said that has not been read.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Fresh {
    /// How many lines, so a row can say "3 new" rather than only that there is
    /// something.
    pub lines: i64,
    /// When the newest of them arrived, so a row can say when rather than only
    /// that it happened.
    pub at: i64,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub agent: String,
    /// What this one is about, so several with the same agent can be told
    /// apart.
    pub name: String,
    /// Whether the engine has ever run in this conversation, which decides how
    /// the next process is started. A new session is `--session-id`; every
    /// reopen after that is `--resume`, and the wrong one is a hard error with
    /// nothing on stdout to explain it.
    pub opened: bool,
    pub started_at: i64,
    pub spoke_at: i64,
    /// The schedule, as somebody would write it: `daily 07:00`. Nothing here
    /// means this is an ordinary conversation that never runs on its own.
    pub runs_at: Option<String>,
    /// What it says to itself when the time comes.
    pub runs_what: Option<String>,
    /// When it last ran, which is what the next run is counted from.
    pub ran_at: Option<i64>,
    /// Switched off without being thrown away. The schedule and what it says
    /// are still here, and the clock walks past it.
    pub routine_off: bool,
    /// The conversation that asked for this one, if it was delegated.
    pub asked_by: Option<String>,
    /// The conversation this one carries on from, if it does. Kept for ever,
    /// so there is always a way back to where the work came from.
    pub came_from: Option<String>,
    /// Where the first launch should pick that conversation up. Cleared the
    /// moment anything runs here.
    pub carries_on: bool,
    /// The engine's own name for the point to carry on from. Nothing means
    /// the end of it.
    pub carries_on_at: Option<String>,
    /// What this conversation watches, as somebody wrote it. Nothing means it
    /// watches nothing.
    pub watches: Option<String>,
    /// What it says to itself when what it watches changes.
    pub watches_what: Option<String>,
    /// The mark of what was there when somebody was last woken.
    pub saw: Option<String>,
    /// Enough of what was there to say later what changed.
    pub saw_note: Option<String>,
    /// A mark seen since, not yet seen twice.
    pub seeing: Option<String>,
    pub looked_at: Option<i64>,
    pub woke_at: Option<i64>,
    /// How many times it has woken somebody today, and which day that is.
    pub woke_today: i64,
    pub woke_on: Option<i64>,
    /// Looks that found something different and never the same thing twice.
    pub unsettled: i64,
    /// Looks that failed outright.
    pub misses: i64,
    /// Why it stopped, if it has. Nothing means it is still looking.
    pub paused: Option<String>,
    /// What this conversation is trying to get to. Nothing means it is doing
    /// what it is asked and no more.
    pub goal: Option<String>,
    pub goal_at: Option<i64>,
    /// Turns spent on it, against the ceiling that stops it being a bill.
    pub goal_tries: i64,
    /// What the agent last said was still to do. Compared against what it says
    /// next time, which is the only way to see a goal going round in circles.
    pub goal_left: Option<String>,
    /// Why it ended, if it has: finished, ran out of turns, went round, or
    /// stopped saying where it was. Four different things to do about it.
    pub goal_over: Option<String>,
}

/// Somewhere models are served from.
///
/// One row covers a machine on the network running Ollama and a company on the
/// other side of the world reached with a key, because from here they are the
/// same thing: an address that lists models and answers questions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    pub id: String,
    /// What somebody calls it. Theirs to choose, because "the Mac Studio" and
    /// "work DeepSeek" are the names that mean anything.
    pub label: String,
    pub provider: String,
    pub base_url: String,
    /// Whether a key is kept for it. Never the key.
    pub has_key: bool,
    /// Which protocol it speaks: `openai` or `anthropic`.
    pub wire: String,
    pub added_at: i64,
}

/// One line in the picker.
///
/// The picker is exactly this table, in this order. Nothing is discovered while
/// it is open and nothing appears that somebody did not put there.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offered {
    pub id: String,
    /// `claude` or `local`.
    pub engine: String,
    pub label: String,
    /// For Claude, the model alias. For a local one, the settings as JSON.
    pub settings: Option<String>,
    /// Where it came from, so it can be looked at again. Nothing for Claude.
    pub backend: Option<String>,
    pub sort: i64,
    /// What makes this the same line as another: the alias for Claude, the
    /// address and model name for anything else. Never how it is configured.
    pub mark: String,
}

/// What one agent has cost over some stretch of time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spending {
    pub agent: String,
    /// What it is called, or something honest if it has been forgotten. The
    /// spending outlives the agent, because it happened.
    pub who: String,
    pub dollars: f64,
    /// How many times a model went round, across all of it.
    pub turns: i64,
    /// How many turns were paid for.
    pub errands: i64,
}

/// One standing job, and whoever is doing it.
///
/// Named after what it is for rather than after the first thing it was asked,
/// because it is the same one you come back to. Its identity is its own: it
/// settles on a name, a role and a mark once it knows what the job is, and all
/// of that is nullable because none of it is a form somebody has to fill in
/// before they can ask for anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    /// The role, in a word or two, shown beside the name so a list of them can
    /// be read at a glance rather than deciphered.
    pub title: Option<String>,
    /// What it handles, in a sentence, in its own words.
    pub about: Option<String>,
    /// The mark it chose for itself. Nothing here means nobody has asked yet,
    /// and the window falls back to guessing from the words.
    pub mark: Option<String>,
    pub hue: Option<String>,
    /// How much it asks before acting: `ask`, `edits` or `auto`.
    pub asks: String,
    /// Kept at the top of the list, by somebody who uses it constantly.
    pub pinned: bool,
    /// Out of the list but still alive, still running whatever it runs.
    pub hidden: bool,
    /// Where the agent works, resolved. Kept because Claude Code scopes a
    /// session to the directory it was started in: reopening from anywhere else
    /// finds no conversation, and reopening the wrong way silently starts an
    /// empty one under the same name. That is a lie a person would not catch.
    pub cwd: String,
    pub model: Option<String>,
    pub started_at: i64,
    pub spoke_at: i64,
    /// `claude`, or `local`.
    pub engine: String,
    /// What the chosen engine needs telling. For a local model that is where
    /// it lives and which one, as JSON; for Claude it is one word, the alias of
    /// the model to run as. Nothing means the engine's own default, which for
    /// Claude is whatever that person's CLI is set to.
    pub engine_settings: Option<String>,
}

/// One thing an agent has written down about how its own job is done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    /// A short handle, which is also what a correction replaces and what
    /// forgetting names. Free text alone has no key, so nothing can ever be
    /// corrected and both the old answer and the new one sit there for ever.
    pub about: String,
    pub note: String,
    /// How often this has come up. The only importance signal here, and one is
    /// enough: a thing an agent has been told three times outranks one it
    /// wrote down once.
    pub told: i64,
    pub told_at: i64,
}

/// One line of a conversation, as it will be shown again tomorrow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub seq: i64,
    pub at: i64,
    /// `mine`, `said`, `doing`, `asking` or `ended`. A string rather than an
    /// enum because
    /// this is what the window switches on, and one vocabulary shared between
    /// the store, the wire and the page is one thing to keep right.
    pub kind: String,
    pub text: String,
    /// The tool call this line belongs to, where it is one. This is what joins
    /// a step to the outcome that arrives later: both sides carry the same
    /// `toolu_...` id, and without it a reloaded thread cannot put an outcome
    /// back on the step it belongs to.
    pub call: Option<String>,
    pub tool: Option<String>,
    pub outcome: Option<String>,
    /// The engine's own name for this point in the conversation, where it has
    /// one. Claude Code names every message it writes and will carry a
    /// conversation on from any of them; a local model names nothing.
    pub anchor: Option<String>,
    /// Pictures attached to this line, by file name.
    ///
    /// The bytes are beside the store rather than in it. A picture somebody
    /// sent used to reach the engine and be thrown away, so the thread said
    /// "(with a picture)" and a conversation that had been about a picture
    /// read afterwards as a conversation about nothing.
    #[serde(default)]
    pub pictures: Vec<String>,
}

pub struct Store {
    conn: Mutex<Connection>,
}

/// Every change to the schema, in order, applied by `PRAGMA user_version`.
///
/// A list rather than a migration ledger with checksums. The previous
/// generation of this program used one and was bitten by it: a daemon built
/// from a slightly older tree refused to open a database it understood
/// perfectly well, reporting "migration 8 was previously applied but is missing
/// in the resolved migrations", which reads like corruption and is nothing of
/// the kind. A version number cannot say that.
/// Every change ever made to the shape of this, in the order they were made.
///
/// Nothing already in this list may be edited, ever. These are not a
/// description of the schema, they are what has already happened on somebody's
/// machine, and a store that has applied change 1 will never apply it again --
/// so an "improvement" to it changes what a fresh install gets and nothing
/// else, and the two silently diverge. This was learnt the ordinary way: a
/// rename of `thread` to `agent` was applied across the file, change 1 included,
/// and every fresh store then failed to build a table the migration below was
/// about to rename anyway.
///
/// Change 1 therefore still says `thread`, and change 3 renames it. That reads
/// oddly and is correct.
const CHANGES: &[&str] = &[
    // 1
    "CREATE TABLE threads (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL,
        cwd         TEXT NOT NULL,
        model       TEXT,
        opened      INTEGER NOT NULL DEFAULT 0,
        started_at  INTEGER NOT NULL,
        spoke_at    INTEGER NOT NULL
     );
     CREATE TABLE lines (
        thread   TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
        seq      INTEGER NOT NULL,
        at       INTEGER NOT NULL,
        kind     TEXT NOT NULL,
        text     TEXT NOT NULL,
        call     TEXT,
        tool     TEXT,
        outcome  TEXT,
        PRIMARY KEY (thread, seq)
     );
     CREATE INDEX lines_by_call ON lines(thread, call);",
    // 2. Which engine this thread belongs to.
    //
    // A thread's memory lives inside its engine -- Claude Code keeps a session
    // transcript of its own, and a local model's history is a list we hold --
    // so this is not a preference, it is part of what the thread is. Two
    // columns rather than one because "which kind" and "which model, where"
    // answer different questions, and a model id on its own does not say which
    // machine it is on.
    "ALTER TABLE threads ADD COLUMN engine TEXT NOT NULL DEFAULT 'claude';
     ALTER TABLE threads ADD COLUMN engine_settings TEXT;",
    // 3. A thread was one errand. An agent is a standing job.
    //
    // The difference is not a rename. A thread was named after the request that
    // started it and had nothing to do afterwards; an agent has a role it keeps,
    // and the second thing you want from it goes to the same one rather than to
    // a stranger who has never met you. Everything later leans on that: the
    // routine belongs to an agent, the connectors are held by one, the
    // delegation is between two.
    //
    // The identity is its own to fill in, which is why every one of these is
    // nullable. Nothing here is a setting somebody has to open a panel to
    // provide.
    "ALTER TABLE threads RENAME TO agents;
     ALTER TABLE lines RENAME COLUMN thread TO agent;
     ALTER TABLE agents ADD COLUMN title TEXT;
     ALTER TABLE agents ADD COLUMN about TEXT;
     ALTER TABLE agents ADD COLUMN mark TEXT;
     ALTER TABLE agents ADD COLUMN hue TEXT;
     ALTER TABLE agents ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE agents ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;",
    // 4. An agent has many conversations.
    //
    // One long conversation is fine until you want to ask the same agent about
    // something else, and then it is not: the whole context of this morning's
    // briefing sits in front of an unrelated question. So the agent keeps its
    // identity, its directory and its engine, and the conversation is what gets
    // an engine session.
    //
    // The first conversation of every existing agent KEEPS THE AGENT'S ID, and
    // that is not tidiness. A conversation id is the engine's session id:
    // Claude Code is resumed with it, and its transcript is filed under it.
    // Give these fresh ids and every conversation anybody has had becomes
    // unresumable, silently, with the transcripts still on disk under names
    // nothing will ever ask for again.
    //
    // `lines` is rebuilt rather than renamed because its foreign key has to
    // point somewhere else, and SQLite cannot alter one in place. Foreign keys
    // are off around every change and checked afterwards, which is what makes
    // dropping a referenced table safe to do here.
    "CREATE TABLE conversations (
        id         TEXT PRIMARY KEY,
        agent      TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
        name       TEXT NOT NULL,
        opened     INTEGER NOT NULL DEFAULT 0,
        started_at INTEGER NOT NULL,
        spoke_at   INTEGER NOT NULL
     );
     INSERT INTO conversations (id, agent, name, opened, started_at, spoke_at)
          SELECT id, id, name, opened, started_at, spoke_at FROM agents;
     CREATE TABLE lines_next (
        conversation TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        seq          INTEGER NOT NULL,
        at           INTEGER NOT NULL,
        kind         TEXT NOT NULL,
        text         TEXT NOT NULL,
        call         TEXT,
        tool         TEXT,
        outcome      TEXT,
        PRIMARY KEY (conversation, seq)
     );
     INSERT INTO lines_next
          SELECT agent, seq, at, kind, text, call, tool, outcome FROM lines;
     DROP TABLE lines;
     ALTER TABLE lines_next RENAME TO lines;
     CREATE INDEX lines_by_call ON lines(conversation, call);
     CREATE INDEX conversations_by_agent ON conversations(agent, spoke_at DESC);",
    // 5. A conversation that runs itself.
    //
    // Not a routines table, because a routine is not a separate thing: it is a
    // conversation with a schedule and something to say. Keeping it here means
    // yesterday's briefing sits directly above today's in the same place, and
    // "what did it say last Tuesday" is scrolling rather than archaeology.
    //
    // `ran_at` is when it last actually ran, and is what the next run is
    // counted from. Nothing means it has never run.
    "ALTER TABLE conversations ADD COLUMN runs_at TEXT;
     ALTER TABLE conversations ADD COLUMN runs_what TEXT;
     ALTER TABLE conversations ADD COLUMN ran_at INTEGER;",
    // 6. What an agent is allowed to do without being asked again.
    //
    // Ours rather than the engine's. Claude Code will happily remember an
    // "always" in its own settings, and then the list of what this app may do
    // to your machine lives somewhere the app cannot show you and cannot take
    // anything off. An allowlist you cannot read is not a boundary, it is a
    // rumour.
    //
    // A rule is a tool and optionally the beginning of the thing it does, so
    // "curl -s https://example.com" can be allowed without allowing every
    // command. An empty rule means any use of that tool.
    "CREATE TABLE allowed (
        id      TEXT PRIMARY KEY,
        agent   TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
        tool    TEXT NOT NULL,
        rule    TEXT NOT NULL DEFAULT '',
        said_at INTEGER NOT NULL
     );
     CREATE INDEX allowed_by_agent ON allowed(agent);
     ALTER TABLE agents ADD COLUMN asks TEXT NOT NULL DEFAULT 'ask';",
    // 7. Who asked for this conversation, when somebody did.
    //
    // Kept so a chain of hand-offs can be walked back. Two agents that each
    // think the other should handle a job will hand it to each other for ever,
    // and every round of that costs a conversation, a process and ten minutes
    // of somebody's money. Handling delegations one at a time used to hide it,
    // because the second hand-off simply waited for the first; running them at
    // once is what turns it into a real loop.
    //
    // Not a column on the asking side and not a field passed along with the
    // request, because when the delegate delegates the app is told only which
    // conversation is asking. The chain has to be recoverable from what
    // outlives the call, which is the store.
    //
    // No foreign key: the conversation that asked may be forgotten while this
    // one is kept, and losing the record of who asked is not a reason to lose
    // the conversation.
    "ALTER TABLE conversations ADD COLUMN asked_by TEXT;",
    // 8. What an agent has learnt about how its own job is done.
    //
    // The agent and not the conversation, and that is the whole design in one
    // foreign key. A conversation already remembers itself: Claude Code holds a
    // transcript and the local engine holds a history. What neither holds is
    // the thing worth keeping across all of them, which is how this particular
    // job is done here -- where the briefing goes, which template the invoices
    // use, the flag that export needs. That belongs to the standing job, so it
    // belongs to the agent, and cascading from `agents` means forgetting one
    // takes its notes with it rather than leaving a second thing to remember.
    //
    // Not global, for the same reason conversations exist at all: one agent's
    // notes in front of an unrelated agent is the same failure as this
    // morning's briefing sitting in front of an unrelated question.
    //
    // An ordinary table with a full-text index over it, rather than the index
    // as the table. A virtual table has no foreign keys, so notes kept only in
    // one would survive `forget()` entirely: the agent gone and its notes still
    // searchable, and `points_at_nothing` unable to see it because there is no
    // reference left to check. All three triggers go in now rather than only
    // the insert one, because an index that is correct only while nothing is
    // ever deleted is an index that is wrong the first time anybody uses this.
    "CREATE TABLE memories (
        id       TEXT PRIMARY KEY,
        agent    TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
        about    TEXT NOT NULL,
        note     TEXT NOT NULL,
        told     INTEGER NOT NULL DEFAULT 1,
        noted_at INTEGER NOT NULL,
        told_at  INTEGER NOT NULL,
        UNIQUE(agent, about)
     );
     CREATE INDEX memories_by_agent ON memories(agent, told DESC, told_at DESC);
     CREATE VIRTUAL TABLE memories_fts USING fts5(
        about, note, content='memories', content_rowid='rowid',
        tokenize='porter unicode61'
     );
     CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
        INSERT INTO memories_fts(rowid, about, note)
        VALUES (new.rowid, new.about, new.note);
     END;
     CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, about, note)
        VALUES ('delete', old.rowid, old.about, old.note);
     END;
     CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, about, note)
        VALUES ('delete', old.rowid, old.about, old.note);
        INSERT INTO memories_fts(rowid, about, note)
        VALUES (new.rowid, new.about, new.note);
     END;",
    // 9. A conversation that carries on from another one.
    //
    // Three facts, and they are not the same fact, which is why they are three
    // columns. `came_from` is kept for ever and is only ever read to say where
    // this came from and to offer the way back. The other two are an
    // instruction for the first launch and are cleared the moment anything
    // runs here, because a conversation that has already spoken has a history
    // of its own, and carrying it on from its parent again would throw that
    // history away.
    //
    // The instruction is a column rather than an argument to the command that
    // makes the fork, because somebody can carry a conversation on and quit
    // before saying anything in it. Then nothing has run, there is no
    // transcript, and the next launch has to be told again where to start.
    //
    // No foreign key on `came_from`, for the same reason `asked_by` has none:
    // the conversation it came from may be forgotten while this one is kept,
    // and losing the way back is not a reason to lose the conversation.
    //
    // `anchor` on a line is the engine's own name for that point, handed back
    // when a conversation is carried on from there. Claude Code names every
    // message it writes; a local model names nothing and puts nothing here.
    "ALTER TABLE conversations ADD COLUMN came_from TEXT;
     ALTER TABLE conversations ADD COLUMN carries_on INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE conversations ADD COLUMN carries_on_at TEXT;
     ALTER TABLE lines ADD COLUMN anchor TEXT;",
    // 10. A conversation woken by something changing.
    //
    // Beside the schedule rather than instead of it. They are different
    // questions and one conversation may want both: a briefing every morning
    // that also wakes when the folder it reports on gets a new file. Putting
    // the interval in `runs_at` would mean every reader of that column had to
    // check another one to know what the string in it meant, and a column with
    // two meanings is the shape of mistake this file already carries two scars
    // from.
    //
    // Not a watches table, for change 5's reason unchanged: a watch is not a
    // separate thing. It is a conversation with something to look at and
    // something to say.
    //
    // `saw` is the mark of what was there when somebody was last woken, and
    // `seeing` is a mark seen since that has not yet repeated. Both are needed
    // because a difference is not a change until it has been seen twice the
    // same way: a page with a fresh token in every response differs on every
    // look and must never wake anybody, and without `seeing` there is nothing
    // to tell that from a page that really moved.
    //
    // `unsettled` and `misses` count the two ways a watch can be useless: one
    // that can never settle, and one that cannot be reached. Both pause it,
    // because a watch failing quietly forever is worse than one that stops and
    // says why.
    "ALTER TABLE conversations ADD COLUMN watches TEXT;
     ALTER TABLE conversations ADD COLUMN watches_what TEXT;
     ALTER TABLE conversations ADD COLUMN saw TEXT;
     ALTER TABLE conversations ADD COLUMN saw_note TEXT;
     ALTER TABLE conversations ADD COLUMN seeing TEXT;
     ALTER TABLE conversations ADD COLUMN looked_at INTEGER;
     ALTER TABLE conversations ADD COLUMN woke_at INTEGER;
     ALTER TABLE conversations ADD COLUMN woke_today INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE conversations ADD COLUMN woke_on INTEGER;
     ALTER TABLE conversations ADD COLUMN unsettled INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE conversations ADD COLUMN misses INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE conversations ADD COLUMN paused TEXT;",
    // 11: something to get to, rather than something to do.
    //
    // `goal_tries` is the ceiling that stops a goal turning into a bill, and
    // `goal_left` is what the agent last said was still to do. That second one
    // is not a nicety: comparing it against what the agent says this time is
    // the only way to notice a goal going round in circles, which is the way
    // this fails in practice, because every individual turn looks like work.
    //
    // `goal_over` is why it ended rather than whether, since "it finished" and
    // "it ran out of turns" and "it stopped saying where it was" want three
    // different things done about them.
    "ALTER TABLE conversations ADD COLUMN goal TEXT;
     ALTER TABLE conversations ADD COLUMN goal_at INTEGER;
     ALTER TABLE conversations ADD COLUMN goal_tries INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE conversations ADD COLUMN goal_left TEXT;
     ALTER TABLE conversations ADD COLUMN goal_over TEXT;",
    // 12: the picker stops being a search and becomes a list somebody keeps.
    //
    // What was there before was a question asked every time the dropdown
    // opened: probe this machine, probe the network if asked, and show whatever
    // answered. That is the wrong shape for the thing it is. It is slow every
    // time, it is different every time, most of what it finds is not loaded and
    // could not answer without a wait, and nothing anybody chose is remembered.
    //
    // `backends` is a place that serves models, which now includes hosted ones
    // reached with a key. `offered` is the picker: exactly what is in it is
    // exactly what the dropdown shows, in the order it shows them.
    //
    // Seeded from what is already true, because a change that empties somebody's
    // picker is a change that breaks their app. Both halves of that: the Claude
    // entries that were compiled in, and whatever any agent is actually set to
    // right now, which is the one thing that must not vanish.
    "CREATE TABLE IF NOT EXISTS backends (
         id       TEXT PRIMARY KEY,
         label    TEXT NOT NULL,
         provider TEXT NOT NULL,
         base_url TEXT NOT NULL,
         has_key  INTEGER NOT NULL DEFAULT 0,
         added_at INTEGER NOT NULL
     );
     CREATE TABLE IF NOT EXISTS offered (
         id       TEXT PRIMARY KEY,
         engine   TEXT NOT NULL,
         label    TEXT NOT NULL,
         settings TEXT,
         backend  TEXT REFERENCES backends(id) ON DELETE CASCADE,
         sort     INTEGER NOT NULL
     );
     CREATE UNIQUE INDEX IF NOT EXISTS offered_once
         ON offered(engine, coalesce(settings, ''));

     INSERT OR IGNORE INTO offered (id, engine, label, settings, backend, sort)
     VALUES
       ('seed-claude-default', 'claude', 'Claude - your default', NULL,   NULL, 0),
       ('seed-claude-opus',    'claude', 'Claude - Opus',         'opus', NULL, 1),
       ('seed-claude-sonnet',  'claude', 'Claude - Sonnet',       'sonnet', NULL, 2),
       ('seed-claude-haiku',   'claude', 'Claude - Haiku',        'haiku', NULL, 3);

     INSERT OR IGNORE INTO offered (id, engine, label, settings, backend, sort)
     SELECT 'seed-' || a.id, 'local', 'In use by ' || a.name, a.engine_settings, NULL, 10
       FROM agents a
      WHERE a.engine = 'local'
        AND a.engine_settings IS NOT NULL
        AND trim(a.engine_settings) <> '';",
    // 13: which protocol a backend speaks.
    //
    // Its own change rather than a column added to the one above, and the
    // reason is written at the top of this list: a change that has already run
    // on somebody's machine is history, not a description. Editing 12 gave
    // fresh installs a column that every existing store lacked, and the screen
    // that lists backends answered "no such column: wire" -- which is exactly
    // the failure the note above describes, from the person who had just read
    // it.
    "ALTER TABLE backends ADD COLUMN wire TEXT NOT NULL DEFAULT 'openai';",
    // 14: what makes two lines in the picker the same line.
    //
    // It was the settings blob, compared as text, which is not what identifies
    // a model: the same model reached the same way is one line whether or not
    // the JSON around it also carries a context window and a temperature. It
    // does not, when one came from an agent that was already using it and the
    // other from somebody ticking it in the list, so the picker showed
    // qwen2.5:7b-instruct twice on the very first use of the screen built to
    // stop exactly that.
    //
    // What identifies it: for Claude the alias, and for anything else the
    // address and the model name. Nothing about how it is configured.
    //
    // The seeded labels are made readable on the way past. "In use by Run the
    // shell command: echo the-picker-wo…" is what an agent was called, not what
    // a model is called, and it is the first thing anybody would want to
    // change.
    "ALTER TABLE offered ADD COLUMN mark TEXT;

     UPDATE offered
        SET label = coalesce(json_extract(settings, '$.model'), 'a model') || ' · ' ||
                    replace(replace(coalesce(json_extract(settings, '$.base_url'), ''),
                            'https://', ''), 'http://', '')
      WHERE label LIKE 'In use by %'
        AND json_extract(settings, '$.model') IS NOT NULL;

     UPDATE offered
        SET mark = CASE engine
                     WHEN 'claude' THEN 'claude|' || coalesce(settings, '')
                     ELSE 'local|' || coalesce(json_extract(settings, '$.base_url'), '')
                             || '|' || coalesce(json_extract(settings, '$.model'), '')
                   END;

     DELETE FROM offered
      WHERE rowid NOT IN (SELECT min(rowid) FROM offered GROUP BY mark);

     DROP INDEX IF EXISTS offered_once;
     CREATE UNIQUE INDEX offered_once ON offered(mark);",
    // 15: what it cost.
    //
    // The engine says on every turn and this app threw it away, so an errand
    // that ran every morning for a month had no answer at all to the one
    // question anybody asks about running errands.
    //
    // Kept per turn rather than as a running total on the conversation, because
    // "today" and "this month" are both questions and a total answers neither.
    //
    // No foreign key to the conversation, on purpose. Money spent is a fact
    // whether or not the thread it was spent on is still kept, and deleting the
    // record along with the thread would quietly understate what was spent.
    "CREATE TABLE IF NOT EXISTS spending (
         id           TEXT PRIMARY KEY,
         agent        TEXT NOT NULL,
         conversation TEXT NOT NULL,
         at           INTEGER NOT NULL,
         dollars      REAL NOT NULL,
         turns        INTEGER NOT NULL DEFAULT 1
     );
     CREATE INDEX IF NOT EXISTS spending_when ON spending(at);",
    // 16
    //
    // What an agent may reach outside its own folder. Nothing is in here until
    // somebody puts it there, one at a time, which is what protects a person's
    // mail: these are run by the app rather than by the walled engine, so the
    // wall is not what decides.
    "CREATE TABLE IF NOT EXISTS connected (
         id TEXT PRIMARY KEY,
         at INTEGER NOT NULL
     );",
    // 17
    //
    // How far down this conversation somebody has read, as a line number rather
    // than a time. Two lines can share a millisecond, and they do: an agent
    // answering and the app writing a note about it land on the same one often
    // enough that a clock here loses the second of them for good.
    //
    // Nought rather than the end, deliberately. A store upgrading has
    // conversations full of lines somebody has already read, and nought marks
    // all of them new at once -- which is right, because the window clears it
    // the moment anything is opened, and the alternative is an app that has
    // just learnt to say "something happened" and never does.
    "ALTER TABLE conversations ADD COLUMN seen INTEGER NOT NULL DEFAULT 0;",
    // 18
    //
    // A routine switched off rather than thrown away. The only stop there was
    // cleared the schedule, what it says and when it last ran, in one
    // statement, so going away for a week and coming back meant setting the
    // whole thing up again from memory. The pattern is one struct over: a
    // watch has had `paused` since it was written.
    "ALTER TABLE conversations ADD COLUMN routine_off INTEGER NOT NULL DEFAULT 0;",
    // 19
    //
    // A row for every time a routine ran, rather than one column holding only
    // the last one. "When is it next" is not the question people ask about a
    // standing job; "has it been working" is, and a single `ran_at` cannot
    // answer it. Three failed mornings leave a conversation looking merely
    // quiet.
    // Keyed by the row rather than by the clock. Two runs can share a
    // millisecond -- a routine and somebody pressing Try it now, or two
    // triggers landing together -- and a key of (conversation, at) quietly
    // makes those one run, which is the kind of loss a history exists to stop.
    "CREATE TABLE IF NOT EXISTS runs (
         id           INTEGER PRIMARY KEY,
         conversation TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
         at           INTEGER NOT NULL,
         why          TEXT NOT NULL,
         outcome      TEXT
     );",
    "CREATE INDEX IF NOT EXISTS runs_by_conversation ON runs(conversation, at DESC);",
    // 21
    //
    // The pictures attached to a line, by file name, as a JSON array. Names
    // rather than bytes: a base64 image in a transcript line is read back into
    // the window on every reopen, and a store that holds a conversation should
    // not also be an album. The files sit beside the store, and this says which
    // of them belong to which thing somebody said.
    "ALTER TABLE lines ADD COLUMN pictures TEXT;",
    // 22
    //
    // Whether a turn was in flight. Set when one starts and cleared when it
    // ends, so that a turn cut off by the app closing can be told, at the next
    // start, from one that finished.
    //
    // Nothing anywhere knew this. Quitting Errand mid-turn killed the engine
    // and left the transcript holding a question with no answer and nothing
    // saying why -- which reads exactly like an app still thinking about it,
    // for ever.
    "ALTER TABLE conversations ADD COLUMN in_flight INTEGER NOT NULL DEFAULT 0;",
];

/// What makes two lines in the picker the same line.
///
/// The alias for Claude, and the address with the model name for anything else.
/// Never how it is configured: the same model reached the same way is one line
/// whether or not the settings around it also carry a context window.
pub fn what_makes_it_the_same(engine: &str, settings: Option<&str>) -> String {
    if engine != "local" {
        return format!("claude|{}", settings.unwrap_or_default());
    }
    let named: serde_json::Value =
        serde_json::from_str(settings.unwrap_or("{}")).unwrap_or(serde_json::Value::Null);
    format!(
        "local|{}|{}",
        named["base_url"].as_str().unwrap_or_default(),
        named["model"].as_str().unwrap_or_default()
    )
}

/// Make a store and everything beside it readable by its owner and nobody else.
fn nobody_elses_business(at: &Path) {
    use std::os::unix::fs::PermissionsExt;
    for what in [at.to_path_buf(), wal(at), shm(at)] {
        if what.exists() {
            let _ = std::fs::set_permissions(&what, std::fs::Permissions::from_mode(0o600));
        }
    }
}

/// The write-ahead log beside a store, which holds the most recent of
/// everything and is created by SQLite rather than by us.
fn wal(at: &Path) -> std::path::PathBuf {
    beside_it(at, "-wal")
}

/// The shared-memory index beside a store.
fn shm(at: &Path) -> std::path::PathBuf {
    beside_it(at, "-shm")
}

fn beside_it(at: &Path, suffix: &str) -> std::path::PathBuf {
    let mut named = at.as_os_str().to_os_string();
    named.push(suffix);
    std::path::PathBuf::from(named)
}

/// What a new agent does about permission, before anybody changes it.
///
/// "Never", which is not what it sounds like. Asking and a wall are the two
/// mechanisms there are, and turning one off is exactly when the other has to
/// be on: an agent that never asks is confined to its own folder and the usual
/// temporary places, and can touch nothing else on the machine.
///
/// It was "ask", and the cost of that was watching somebody give the same
/// errand to this and to something else. Half of these errands run at seven in
/// the morning with nobody at the window, where a card is not a question but a
/// refusal; and even at the keyboard, an errand that stops four times before it
/// reaches the thing it was asked to do is one nobody finishes. Every question
/// it does not ask is still visible afterwards: what it did is in the
/// conversation, line by line.
pub const HOW_A_NEW_AGENT_ASKS: &str = "auto";

impl Store {
    /// Open the store, making it if it is not there yet.
    pub fn open(at: &Path) -> Result<Self> {
        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("making {}", parent.display()))?;
        }
        let conn = Connection::open(at).with_context(|| format!("opening {}", at.display()))?;
        // Write-ahead logging so a read while something is being written does
        // not block, and NORMAL because the cost of the last few milliseconds
        // of a conversation after a power cut is not worth an fsync per line.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.bring_up_to_date()?;
        // Nobody else's business, and it was not: every conversation anybody
        // has ever had with this app is in here, and the file was made
        // readable by anything running on the machine.
        //
        // After the pragmas and the migration rather than before, because
        // switching on write-ahead logging is what creates the two files
        // beside it, and those hold the most recent of everything. Every time
        // it is opened rather than only when it is made, because a store made
        // by an older version is the one with the most in it.
        nobody_elses_business(at);
        Ok(store)
    }

    /// A store in memory, for tests and for nothing else.
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.bring_up_to_date()?;
        Ok(store)
    }

    /// Apply whatever has not been applied, one change at a time, all or
    /// nothing.
    ///
    /// Each change is wrapped in a transaction together with the version bump,
    /// which they were not before. Without that a change that fails halfway
    /// leaves the schema half-altered and the version unmoved, so the next
    /// start applies it again from the top and fails somewhere different: a
    /// store that gets worse every time it is opened.
    ///
    /// Foreign keys go off for the duration, and outside the transaction
    /// because SQLite ignores the pragma inside one. That is not laziness
    /// about correctness, it is what the documented procedure for rebuilding a
    /// referenced table requires -- dropping one with references pointing at it
    /// is exactly the operation -- and `foreign_key_check` afterwards is the
    /// part that keeps it honest. A change that leaves a dangling reference is
    /// rolled back rather than kept.
    fn bring_up_to_date(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let at: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;

        // Measured once, before anything is applied, so each change is judged
        // on what it did rather than on what it found.
        let mut inherited = points_at_nothing(&conn)?;

        for (i, change) in CHANGES.iter().enumerate().skip(at as usize) {
            let version = i + 1;
            conn.pragma_update(None, "foreign_keys", false)?;
            let applied = conn
                .execute_batch(&format!(
                    "BEGIN;\n{change}\nPRAGMA user_version = {version};\nCOMMIT;"
                ))
                .with_context(|| format!("applying change {version}"));
            if applied.is_err() {
                let _ = conn.execute_batch("ROLLBACK;");
            }
            conn.pragma_update(None, "foreign_keys", true)?;
            applied?;

            // What this change broke, not what it inherited. Counting only the
            // rows dangling afterwards makes every future change answer for
            // damage done long before it, and the way that shows up is the
            // worst way anything can: the app will not start, and the message
            // names the one change that is innocent. Which is exactly what
            // happened -- rows orphaned months earlier by a delete made with
            // foreign keys off stopped a change that only added a column.
            let dangling = points_at_nothing(&conn)?;
            anyhow::ensure!(
                dangling <= inherited,
                "change {version} left {} more rows pointing at nothing",
                dangling - inherited
            );
            // Whatever was already wrong stays wrong and stays visible. It is
            // not this change's to repair, and quietly deleting somebody's rows
            // to get past a check is worse than the check.
            inherited = dangling;
        }
        Ok(())
    }

    /// A change that matched no rows changed nothing, whatever it reported.
    ///
    /// SQLite answers "0 rows" and calls it success, so every setting written
    /// against an agent that did not exist yet was accepted by the app, agreed
    /// to on screen, and kept by nobody. A routine set that way left the panel
    /// saying "this runs only when you ask it to", and the only way to find out
    /// was the morning it did not happen.
    ///
    /// Seven of those were found in one afternoon. This is what stops the
    /// eighth being quiet: an update that matches nothing is a failure, and it
    /// says which thing was not there.
    fn only_if_it_is_there(changed: usize, what: &str) -> Result<()> {
        match changed {
            0 => Err(anyhow::anyhow!("there is no {what} here to change")),
            _ => Ok(()),
        }
    }

    /// Which connectors are switched on.
    pub fn connected(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut ask = conn.prepare("SELECT id FROM connected ORDER BY at")?;
        let found = ask
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(found)
    }

    /// Let agents reach one, or stop letting them.
    ///
    /// Never partly: a connector is on or it is not, and being on is a thing
    /// somebody did rather than a thing that accumulated.
    pub fn connect(&self, id: &str, on: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        match on {
            true => conn.execute(
                "INSERT OR REPLACE INTO connected (id, at) VALUES (?, ?)",
                params![id, now()],
            )?,
            false => conn.execute("DELETE FROM connected WHERE id = ?", [id])?,
        };
        Ok(())
    }

    /// What an agent has said that nobody has read yet.
    ///
    /// The whole point of this app is errands that run while nobody is looking,
    /// and until this the window had no way to say that one had. An agent that
    /// produced a briefing at seven this morning looked exactly like one that
    /// had not run in a month: the row said what the agent is for, which is the
    /// right line to have there and not an answer to "did anything happen".
    ///
    /// Lines somebody typed themselves are never new. They read them as they
    /// wrote them, and counting them would mean a conversation somebody just
    /// finished having is unread the moment they close it.
    pub fn what_is_new(&self) -> Result<HashMap<String, Fresh>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT c.agent, count(*), max(l.at)
               FROM lines l JOIN conversations c ON c.id = l.conversation
              WHERE l.seq > c.seen AND l.kind <> 'mine'
              GROUP BY c.agent",
        )?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Fresh {
                    lines: r.get(1)?,
                    at: r.get(2)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
    }

    /// Say that everything in this conversation has now been seen.
    ///
    /// Marked from the last line there is rather than from the clock, and never
    /// backwards. Between reading a conversation and writing this down an agent
    /// can say something else, and a time here would mark that line read
    /// without anybody having laid eyes on it.
    pub fn seen(&self, conversation: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE conversations
                SET seen = max(seen, coalesce(
                      (SELECT max(seq) FROM lines WHERE conversation = ?1), 0))
              WHERE id = ?1",
            [conversation],
        )?;
        Self::only_if_it_is_there(changed, "conversation")
    }

    /// Everything a new agent needs to exist, before anything points at it.
    ///
    /// An agent made in the window is not written down until there is something
    /// to write: somebody who makes one and then thinks better of it should not
    /// leave a row behind. The cost of that is that the first thing ever said to
    /// a new agent is said to something that does not exist, and the line
    /// written for it fails on the foreign key -- which is what somebody saw
    /// instead of an answer, on the first thing they ever typed into this app.
    ///
    /// One call rather than two, so there is one moment an agent starts existing
    /// and it cannot be half done. Saying it twice is not a second agent.
    pub fn make_sure_it_exists(&self, id: &str, name: &str, cwd: &Path) -> Result<()> {
        self.begin(id, name, cwd)?;
        // Its first conversation shares the agent's id, which is what the
        // migration did for every agent that existed before conversations did.
        self.begin_conversation(id, id, "First")
    }

    /// Start a thread, or say nothing if it is already there.
    ///
    /// Nothing here uses this directly except `make_sure_it_exists`.
    pub fn begin(&self, id: &str, name: &str, cwd: &Path) -> Result<()> {
        let now = now();
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO agents (id, name, cwd, opened, started_at, spoke_at, asks)
             VALUES (?, ?, ?, 0, ?, ?, ?)",
            params![
                id,
                name,
                cwd.to_string_lossy(),
                now,
                now,
                HOW_A_NEW_AGENT_ASKS
            ],
        )?;
        Ok(())
    }

    /// Start a conversation with an agent.
    ///
    /// The id is the caller's to choose and becomes the engine's session id, so
    /// it must be a fresh one every time. Reusing one would resume a
    /// conversation somebody meant to leave behind.
    pub fn begin_conversation(&self, id: &str, agent: &str, name: &str) -> Result<()> {
        self.begin_conversation_for(id, agent, name, None)
    }

    /// The same, for a conversation one agent opened by asking another.
    ///
    /// `asked_by` is the conversation that asked, not the agent, because that
    /// is what can be followed back another step. A chain of hand-offs is a
    /// chain of conversations.
    pub fn begin_conversation_for(
        &self,
        id: &str,
        agent: &str,
        name: &str,
        asked_by: Option<&str>,
    ) -> Result<()> {
        let now = now();
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO conversations
                 (id, agent, name, opened, started_at, spoke_at, asked_by)
             VALUES (?, ?, ?, 0, ?, ?, ?)",
            params![id, agent, name, now, now, asked_by],
        )?;
        Ok(())
    }

    /// Every agent already on the chain of hand-offs that led here.
    ///
    /// Nearest first, starting with the agent of the conversation given. Used
    /// to refuse a job being handed back to somebody who is already waiting on
    /// it, which without this is two processes waiting ten minutes for each
    /// other and, once several hand-offs can run at once, a chain that grows a
    /// conversation and a process at every step.
    ///
    /// Bounded rather than trusted. Following a chain by reading rows is
    /// exactly the shape of thing that loops for ever if a row is ever wrong,
    /// and a delegation depth in double figures is already a runaway.
    /// How many rows refer to something that is not there.
    ///
    /// Asked by the doctor rather than acted on. Rows can be orphaned by a
    /// delete made anywhere with foreign keys off, and they are invisible to
    /// the app because everything is looked up through the conversation it
    /// belongs to. Worth reporting, and not worth deleting behind somebody's
    /// back to make a check pass.
    /// Write something down, or correct what was written before.
    ///
    /// One row per handle per agent, so saying the same thing twice is a
    /// confirmation and saying something different under the same handle is a
    /// correction. Without that, "the briefing goes to email" and "the briefing
    /// goes to Telegram" both sit there and nothing can settle which is true.
    pub fn remember(&self, agent: &str, about: &str, note: &str) -> Result<()> {
        let now = now();
        self.conn.lock().unwrap().execute(
            "INSERT INTO memories (id, agent, about, note, told, noted_at, told_at)
                  VALUES (?, ?, ?, ?, 1, ?, ?)
             ON CONFLICT(agent, about) DO UPDATE SET
                  note = excluded.note,
                  told = told + 1,
                  told_at = excluded.told_at",
            params![
                uuid_like(&format!("{agent}{about}")),
                agent,
                about,
                note,
                now,
                now
            ],
        )?;
        Ok(())
    }

    /// What this agent knows, most-confirmed first.
    pub fn remembers(&self, agent: &str, most: usize) -> Result<Vec<Memory>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT about, note, told, told_at FROM memories
              WHERE agent = ? ORDER BY told DESC, told_at DESC LIMIT ?",
        )?;
        let rows = q.query_map(params![agent, most as i64], read_memory)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// What this agent knows about something in particular.
    ///
    /// Ranked, which is the whole reason this is a full-text index rather than
    /// a LIKE: the one right note has to come above four that share a word, and
    /// LIKE cannot order at all. The handle is weighted four times the note,
    /// because it is the thing that most identifies what a note is about and
    /// would otherwise be drowned by the note's own prose.
    pub fn recall(&self, agent: &str, looking_for: &str, most: usize) -> Result<Vec<Memory>> {
        let asking = as_a_query(looking_for);
        if asking.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT m.about, m.note, m.told, m.told_at
               FROM memories_fts
               JOIN memories m ON m.rowid = memories_fts.rowid
              WHERE memories_fts MATCH ?1 AND m.agent = ?2
              ORDER BY bm25(memories_fts, 4.0, 1.0) ASC
              LIMIT ?3",
        )?;
        let rows = q.query_map(params![asking, agent, most as i64], read_memory)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Take a note back. True when there was one to take back.
    pub fn forget_note(&self, agent: &str, about: &str) -> Result<bool> {
        let gone = self.conn.lock().unwrap().execute(
            "DELETE FROM memories WHERE agent = ? AND about = ?",
            params![agent, about],
        )?;
        Ok(gone > 0)
    }

    /// Start a conversation that carries on from another one.
    ///
    /// Everything up to and including `up_to` is copied, and nothing at all is
    /// removed from where it came from. That is the whole safety of the
    /// feature: going back to an earlier point makes a second conversation
    /// rather than shortening the first, so being wrong about where to go back
    /// to costs nothing but a conversation nobody uses.
    ///
    /// `outcome` is copied along with the step it belongs to. Without it every
    /// carried-over step reads as a step that hung, which is why this is the
    /// second writer of `lines` rather than a loop over `append`.
    ///
    /// `seq` and `at` are kept rather than re-derived: the copied lines
    /// happened when they happened, and a prefix of a dense sequence is still
    /// dense, so the next line appended here still lands in the right place.
    ///
    /// A schedule and a delegation chain are deliberately not copied. A
    /// carried-on routine would be a second thing firing at seven every
    /// morning that nobody set up, and an inherited chain would make this
    /// conversation refuse hand-offs it has nothing to do with.
    pub fn carry_on(
        &self,
        new_id: &str,
        from: &str,
        up_to: i64,
        name: &str,
        at_anchor: Option<&str>,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let now = now();
        let doing = conn.transaction()?;
        doing.execute(
            "INSERT INTO conversations
                  (id, agent, name, opened, started_at, spoke_at,
                   came_from, carries_on, carries_on_at)
             SELECT ?1, agent, ?2, 0, ?3, ?3, ?4, 1, ?5
               FROM conversations WHERE id = ?4",
            params![new_id, name, now, from, at_anchor],
        )?;
        doing.execute(
            "INSERT INTO lines
                  (conversation, seq, at, kind, text, call, tool, outcome, anchor)
             SELECT ?1, seq, at, kind, text, call, tool, outcome, anchor
               FROM lines WHERE conversation = ?2 AND seq <= ?3",
            params![new_id, from, up_to],
        )?;
        doing.commit()?;
        Ok(())
    }

    /// Say something into a conversation that nobody has to answer.
    ///
    /// For the kind of news that costs no engine turn to deliver: a watch that
    /// has stopped, and why. It belongs in the conversation because that is
    /// where this program says things, and putting it anywhere else means
    /// somebody has to go and look for it.
    pub fn noted(&self, conversation: &str, said: &str) -> Result<()> {
        self.append(conversation, "ended", said, None, None)?;
        Ok(())
    }

    /// Set or clear what a conversation watches.
    ///
    /// Everything it had seen is forgotten along with it, for the same reason
    /// changing a schedule clears when it last ran: what a watch saw is only
    /// meaningful against the thing it was watching, and keeping it would mean
    /// comparing a folder's mark against a page's.
    pub fn watch(
        &self,
        conversation: &str,
        watches: Option<&str>,
        what: Option<&str>,
    ) -> Result<()> {
        let changed = self.conn.lock().unwrap().execute(
            "UPDATE conversations
                SET watches = ?, watches_what = ?,
                    saw = NULL, saw_note = NULL, seeing = NULL, looked_at = NULL,
                    woke_at = NULL, woke_today = 0, woke_on = NULL,
                    unsettled = 0, misses = 0, paused = NULL
              WHERE id = ?",
            params![watches, what, conversation],
        )?;
        Self::only_if_it_is_there(changed, "conversation")
    }

    /// Set, change or clear what this conversation is trying to get to.
    ///
    /// Changing a goal starts it over rather than carrying the count on. That is
    /// the point of being able to change it: somebody who has watched an agent
    /// struggle and has narrowed the goal is starting a different attempt, and
    /// giving the new one the old one's spent turns would end it before it
    /// began.
    pub fn aim_at(&self, conversation: &str, goal: Option<&str>, now: i64) -> Result<()> {
        let changed = self.conn.lock().unwrap().execute(
            "UPDATE conversations
                SET goal = ?, goal_at = ?, goal_tries = 0,
                    goal_left = NULL, goal_over = NULL
              WHERE id = ?",
            params![goal, goal.map(|_| now), conversation],
        )?;
        Self::only_if_it_is_there(changed, "conversation")
    }

    /// Write down where a goal has got to after a turn.
    pub fn got_to(&self, conversation: &str, left: Option<&str>, over: Option<&str>) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations
                SET goal_tries = goal_tries + 1, goal_left = ?, goal_over = ?
              WHERE id = ?",
            params![left, over, conversation],
        )?;
        Ok(())
    }

    /// Everything the picker shows, in the order it shows it.
    pub fn offered(&self) -> Result<Vec<Offered>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, engine, label, settings, backend, sort, mark
               FROM offered ORDER BY sort, label",
        )?;
        let rows = q.query_map([], |r| {
            Ok(Offered {
                id: r.get(0)?,
                engine: r.get(1)?,
                label: r.get(2)?,
                settings: r.get(3)?,
                backend: r.get(4)?,
                sort: r.get(5)?,
                mark: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Put something in the picker, or leave it there if it already is.
    ///
    /// The same model chosen twice is one line, not two, which the index
    /// enforces rather than the caller remembering.
    pub fn offer(&self, one: &Offered) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO offered (id, engine, label, settings, backend, sort, mark)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(mark) DO UPDATE
                SET label = excluded.label,
                    backend = excluded.backend,
                    settings = excluded.settings",
            params![
                one.id,
                one.engine,
                one.label,
                one.settings,
                one.backend,
                one.sort,
                what_makes_it_the_same(&one.engine, one.settings.as_deref()),
            ],
        )?;
        Ok(())
    }

    /// Write down that a model turns out to hold a different amount.
    ///
    /// Everywhere at once, and that is the whole point of it being here rather
    /// than at the two call sites. An agent's settings are a copy taken when
    /// the model was chosen, and nothing has ever gone back to correct one: so
    /// changing the line in the picker leaves every agent already on that model
    /// still sending the old number, which is the same silence this is meant to
    /// end, moved one table over.
    ///
    /// Returns how many rows it changed, so the caller can say nothing at all
    /// when nothing needed saying.
    pub fn it_holds(&self, mark: &str, holds: usize) -> Result<usize> {
        let wants = crate::local::room_for_an_answer(holds);
        let corrected = |settings: Option<&str>| -> Option<String> {
            let mut kept: serde_json::Value = serde_json::from_str(settings?).ok()?;
            // Already right is not a change. Saying so would put a line in
            // front of somebody every time an app starts.
            if kept.get("context_window").and_then(|v| v.as_u64()) == Some(holds as u64) {
                return None;
            }
            kept["context_window"] = serde_json::json!(holds);
            kept["max_tokens"] = serde_json::json!(wants);
            Some(kept.to_string())
        };

        let mut changed = 0;
        for one in self.offered()? {
            if what_makes_it_the_same(&one.engine, one.settings.as_deref()) != mark {
                continue;
            }
            let Some(now) = corrected(one.settings.as_deref()) else {
                continue;
            };
            self.conn.lock().unwrap().execute(
                "UPDATE offered SET settings = ? WHERE id = ?",
                params![now, one.id],
            )?;
            changed += 1;
        }
        for who in self.agents()? {
            if what_makes_it_the_same(&who.engine, who.engine_settings.as_deref()) != mark {
                continue;
            }
            let Some(now) = corrected(who.engine_settings.as_deref()) else {
                continue;
            };
            self.conn.lock().unwrap().execute(
                "UPDATE agents SET engine_settings = ? WHERE id = ?",
                params![now, who.id],
            )?;
            changed += 1;
        }
        Ok(changed)
    }

    /// Give a line in the picker a name somebody chose.
    pub fn call_it_something(&self, id: &str, label: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE offered SET label = ? WHERE id = ?",
            params![label, id],
        )?;
        Ok(())
    }

    /// Move a line up or down the picker.
    ///
    /// Swapped with its neighbour rather than renumbered, so moving one line
    /// does not rewrite the position of every other.
    pub fn move_it(&self, id: &str, up: bool) -> Result<()> {
        let all = self.offered()?;
        let Some(at) = all.iter().position(|o| o.id == id) else {
            return Ok(());
        };
        let swap_with = match up {
            true if at > 0 => at - 1,
            false if at + 1 < all.len() => at + 1,
            // Already at the end it was going towards. Not a failure: somebody
            // pressing up on the top line meant nothing by it.
            _ => return Ok(()),
        };
        // By position in the list rather than by the numbers already stored,
        // which may be equal, sparse, or both after a seed.
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE offered SET sort = ? WHERE id = ?",
            params![swap_with as i64, all[at].id],
        )?;
        conn.execute(
            "UPDATE offered SET sort = ? WHERE id = ?",
            params![at as i64, all[swap_with].id],
        )?;
        Ok(())
    }

    /// Take something out of the picker.
    ///
    /// Only out of the picker. An agent already set to it goes on using it,
    /// because taking away what something is running on is not what "do not
    /// show me this any more" means.
    pub fn stop_offering(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM offered WHERE id = ?", params![id])?;
        Ok(())
    }

    /// Every place models are served from that somebody has told this about.
    pub fn backends(&self) -> Result<Vec<Backend>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, label, provider, base_url, has_key, wire, added_at
               FROM backends ORDER BY added_at",
        )?;
        let rows = q.query_map([], |r| {
            Ok(Backend {
                id: r.get(0)?,
                label: r.get(1)?,
                provider: r.get(2)?,
                base_url: r.get(3)?,
                has_key: r.get::<_, i64>(4)? != 0,
                wire: r.get(5)?,
                added_at: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Remember somewhere models are served from.
    pub fn add_backend(&self, one: &Backend) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO backends (id, label, provider, base_url, has_key, wire, added_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE
                SET label = excluded.label,
                    provider = excluded.provider,
                    base_url = excluded.base_url,
                    has_key = excluded.has_key,
                    wire = excluded.wire",
            params![
                one.id,
                one.label,
                one.provider,
                one.base_url,
                i64::from(one.has_key),
                one.wire,
                one.added_at
            ],
        )?;
        Ok(())
    }

    /// Forget one, and everything it was offering.
    pub fn forget_backend(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM backends WHERE id = ?", params![id])?;
        Ok(())
    }

    /// Write down what a turn cost.
    ///
    /// Only where there was one. A model on this machine costs no dollars, and
    /// a row of zeroes would make every total a lie by omission of what it is
    /// a total of.
    pub fn spent(
        &self,
        agent: &str,
        conversation: &str,
        dollars: f64,
        turns: i64,
        at: i64,
    ) -> Result<()> {
        if dollars <= 0.0 {
            return Ok(());
        }
        self.conn.lock().unwrap().execute(
            "INSERT INTO spending (id, agent, conversation, at, dollars, turns)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![uuid(), agent, conversation, at, dollars, turns],
        )?;
        Ok(())
    }

    /// What has been spent since a moment, by agent, biggest first.
    pub fn spending_since(&self, at: i64) -> Result<Vec<Spending>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT s.agent,
                    coalesce(a.name, 'an agent that is gone'),
                    sum(s.dollars),
                    sum(s.turns),
                    count(*)
               FROM spending s
               LEFT JOIN agents a ON a.id = s.agent
              WHERE s.at >= ?
              GROUP BY s.agent
              ORDER BY sum(s.dollars) DESC",
        )?;
        let rows = q.query_map([at], |r| {
            Ok(Spending {
                agent: r.get(0)?,
                who: r.get(1)?,
                dollars: r.get(2)?,
                turns: r.get(3)?,
                errands: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every conversation that is watching something and has not stopped.
    pub fn watching(&self) -> Result<Vec<Conversation>> {
        Ok(self
            .every_conversation()?
            .into_iter()
            .filter(|c| c.watches.is_some() && c.paused.is_none())
            .collect())
    }

    /// Every conversation that watches something, stopped or not.
    pub fn watchers(&self) -> Result<Vec<Conversation>> {
        Ok(self
            .every_conversation()?
            .into_iter()
            .filter(|c| c.watches.is_some())
            .collect())
    }

    /// Write down what a look found, without waking anybody.
    pub fn looked(
        &self,
        conversation: &str,
        saw: Option<&str>,
        saw_note: Option<&str>,
        seeing: Option<&str>,
        unsettled: i64,
        misses: i64,
    ) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations
                SET looked_at = ?, saw = COALESCE(?, saw), saw_note = COALESCE(?, saw_note),
                    seeing = ?, unsettled = ?, misses = ?
              WHERE id = ?",
            params![
                now(),
                saw,
                saw_note,
                seeing,
                unsettled,
                misses,
                conversation
            ],
        )?;
        Ok(())
    }

    /// Write down that somebody was woken, before they are.
    ///
    /// Before, for the same reason a routine's last run is written before it
    /// starts: if starting fails it has still had its turn, and a watch that
    /// retried every thirty seconds because starting kept failing would be the
    /// worst thing in this file.
    pub fn woke(&self, conversation: &str, saw: &str, saw_note: &str, today: i64) -> Result<()> {
        let now = now();
        self.conn.lock().unwrap().execute(
            "UPDATE conversations
                SET saw = ?, saw_note = ?, seeing = NULL, looked_at = ?,
                    woke_at = ?, unsettled = 0, misses = 0,
                    woke_today = CASE WHEN woke_on = ? THEN woke_today + 1 ELSE 1 END,
                    woke_on = ?
              WHERE id = ?",
            params![saw, saw_note, now, now, today, today, conversation],
        )?;
        Ok(())
    }

    /// Stop a watch, with the reason somebody can act on.
    pub fn pause_watch(&self, conversation: &str, why: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations SET paused = ? WHERE id = ?",
            params![why, conversation],
        )?;
        Ok(())
    }

    /// Start a stopped watch looking again, forgiving whatever stopped it.
    pub fn look_again(&self, conversation: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations
                SET paused = NULL, unsettled = 0, misses = 0, seeing = NULL
              WHERE id = ?",
            params![conversation],
        )?;
        Ok(())
    }

    /// Every conversation there is, whoever it belongs to.
    fn every_conversation(&self) -> Result<Vec<Conversation>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, agent, name, opened, started_at, spoke_at,
                    runs_at, runs_what, ran_at, asked_by,
                    came_from, carries_on, carries_on_at,
                    watches, watches_what, saw, saw_note, seeing, looked_at,
                    woke_at, woke_today, woke_on, unsettled, misses, paused,
                    goal, goal_at, goal_tries, goal_left, goal_over, routine_off
               FROM conversations",
        )?;
        let rows = q.query_map([], read_conversation)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// A name for a conversation carried on from this one, that is not taken.
    pub fn a_name_like(&self, agent: &str, wanted: &str) -> Result<String> {
        let taken: Vec<String> = self
            .conversations(agent)?
            .into_iter()
            .map(|c| c.name)
            .collect();
        if !taken.iter().any(|n| n == wanted) {
            return Ok(wanted.to_string());
        }
        // Counted rather than stamped with a time. "First, again 2" is a thing
        // somebody can say out loud, and a timestamp is not.
        for n in 2..100 {
            let tried = format!("{wanted} {n}");
            if !taken.contains(&tried) {
                return Ok(tried);
            }
        }
        Ok(wanted.to_string())
    }

    pub fn points_at_nothing(&self) -> Result<i64> {
        points_at_nothing(&self.conn.lock().unwrap())
    }

    pub fn who_is_waiting(&self, conversation: &str) -> Result<Vec<String>> {
        const DEEP_ENOUGH: usize = 12;
        let mut chain = Vec::new();
        let mut at = Some(conversation.to_string());

        while let Some(id) = at.take() {
            if chain.len() >= DEEP_ENOUGH {
                break;
            }
            let Some(talk) = self.conversation(&id)? else {
                break;
            };
            chain.push(talk.agent);
            at = talk.asked_by;
        }
        Ok(chain)
    }

    /// One agent's conversations, most recently spoken to first.
    pub fn conversations(&self, agent: &str) -> Result<Vec<Conversation>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, agent, name, opened, started_at, spoke_at,
                    runs_at, runs_what, ran_at, asked_by,
                    came_from, carries_on, carries_on_at,
                    watches, watches_what, saw, saw_note, seeing, looked_at,
                    woke_at, woke_today, woke_on, unsettled, misses, paused,
                    goal, goal_at, goal_tries, goal_left, goal_over, routine_off
               FROM conversations WHERE agent = ? ORDER BY spoke_at DESC",
        )?;
        let rows = q.query_map([agent], read_conversation)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// One conversation, wherever it belongs.
    pub fn conversation(&self, id: &str) -> Result<Option<Conversation>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, agent, name, opened, started_at, spoke_at,
                    runs_at, runs_what, ran_at, asked_by,
                    came_from, carries_on, carries_on_at,
                    watches, watches_what, saw, saw_note, seeing, looked_at,
                    woke_at, woke_today, woke_on, unsettled, misses, paused,
                    goal, goal_at, goal_tries, goal_left, goal_over, routine_off
               FROM conversations WHERE id = ?",
        )?;
        let mut rows = q.query_map([id], read_conversation)?;
        rows.next().transpose().map_err(Into::into)
    }

    /// How much this agent asks before acting.
    pub fn asks(&self, agent: &str, how: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE agents SET asks = ? WHERE id = ?",
            params![how, agent],
        )?;
        Ok(())
    }

    /// Remember that somebody said yes to this, for good.
    pub fn allow(&self, agent: &str, tool: &str, rule: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO allowed (id, agent, tool, rule, said_at) VALUES (?, ?, ?, ?, ?)",
            params![uuid(), agent, tool, rule, now()],
        )?;
        Ok(())
    }

    /// Everything this agent may do without being asked.
    pub fn allowances(&self, agent: &str) -> Result<Vec<Allowance>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, tool, rule, said_at FROM allowed WHERE agent = ? ORDER BY said_at DESC",
        )?;
        let rows = q.query_map([agent], |r| {
            Ok(Allowance {
                id: r.get(0)?,
                tool: r.get(1)?,
                rule: r.get(2)?,
                said_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Take one back.
    pub fn revoke(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM allowed WHERE id = ?", [id])?;
        Ok(())
    }

    /// Has this exact thing already been allowed?
    ///
    /// A rule matches when the tool is the same and the thing being done starts
    /// with the rule. Prefix rather than equality, because the rule an engine
    /// suggests is the shape of the command and not the command: allowing
    /// `curl -s https://example.com` should cover fetching a second page of it
    /// and must not cover `curl` on its own.
    pub fn already_allowed(&self, agent: &str, tool: &str, doing: &str) -> Result<bool> {
        Ok(self
            .allowances(agent)?
            .into_iter()
            .any(|a| a.tool == tool && crate::allowing::covers(&a.rule, doing)))
    }

    /// Give a conversation a schedule, or take one away.
    ///
    /// `ran_at` is cleared with it. A schedule that has just been set has never
    /// run, whatever the conversation did before, and counting the first run
    /// from an old timestamp would either fire it at once or hold it back by
    /// however long it happened to be since.
    pub fn runs(&self, conversation: &str, at: Option<&str>, what: Option<&str>) -> Result<()> {
        let changed = self.conn.lock().unwrap().execute(
            "UPDATE conversations SET runs_at = ?, runs_what = ?, ran_at = NULL WHERE id = ?",
            params![at, what, conversation],
        )?;
        Self::only_if_it_is_there(changed, "conversation")
    }

    /// Say that a turn has started in this conversation.
    pub fn a_turn_began(&self, conversation: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations SET in_flight = 1 WHERE id = ?",
            [conversation],
        )?;
        Ok(())
    }

    /// Say that it has ended, however it ended.
    pub fn a_turn_ended(&self, conversation: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations SET in_flight = 0 WHERE id = ?",
            [conversation],
        )?;
        Ok(())
    }

    /// Every conversation that was mid-turn when this last stopped.
    ///
    /// Read once at startup. A turn cannot survive the process that was running
    /// it, so anything still marked when the app opens was cut off rather than
    /// finished, and the person is owed a sentence saying so.
    pub fn turns_that_were_cut_off(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare("SELECT id FROM conversations WHERE in_flight = 1")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Say that a routine has just run.
    pub fn ran(&self, conversation: &str, at: i64) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations SET ran_at = ? WHERE id = ?",
            params![at, conversation],
        )?;
        Ok(())
    }

    /// Switch a routine off, or back on, without throwing it away.
    ///
    /// The only stop there was cleared the schedule, what it says and when it
    /// last ran in one statement, so going away for a week and coming back
    /// meant setting the whole thing up again from memory. Nothing about a
    /// routine is destroyed here: the clock simply walks past it.
    pub fn routine_off(&self, conversation: &str, off: bool) -> Result<()> {
        let changed = self.conn.lock().unwrap().execute(
            "UPDATE conversations SET routine_off = ? WHERE id = ?",
            params![i64::from(off), conversation],
        )?;
        Self::only_if_it_is_there(changed, "conversation")
    }

    /// Write down that a run has started, and what started it.
    ///
    /// Returns which run it is, so the caller can say how that one ended. Not
    /// the time: two runs can share a millisecond, and a history that turns
    /// those into one run is losing exactly what it exists to keep.
    pub fn a_run_began(&self, conversation: &str, why: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO runs (conversation, at, why, outcome) VALUES (?, ?, ?, NULL)",
            params![conversation, now(), why],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Write down how a run ended.
    ///
    /// A row left with nothing against it is a run that never came back, which
    /// is a real outcome and a different one from failing: the app was quit, or
    /// the machine slept. Saying nothing about it is more honest than inventing
    /// a reason at the next launch.
    pub fn a_run_ended(&self, run: i64, outcome: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE runs SET outcome = ? WHERE id = ?",
            params![outcome, run],
        )?;
        Ok(())
    }

    /// How a routine has actually been going, newest first.
    ///
    /// The question people ask about a standing job is not when it is next but
    /// whether it has been working, and one `ran_at` column cannot answer it.
    /// Three failed mornings leave a conversation looking merely quiet.
    pub fn how_it_has_been_going(&self, conversation: &str, at_most: i64) -> Result<Vec<Run>> {
        let conn = self.conn.lock().unwrap();
        // By id within the same millisecond, so two runs that landed together
        // still come back in the order they happened.
        let mut q = conn.prepare(
            "SELECT at, why, outcome FROM runs
              WHERE conversation = ? ORDER BY at DESC, id DESC LIMIT ?",
        )?;
        let rows = q.query_map(params![conversation, at_most], |r| {
            Ok(Run {
                at: r.get(0)?,
                why: r.get(1)?,
                outcome: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every conversation with a schedule on it, whichever agent it belongs to.
    pub fn routines(&self) -> Result<Vec<Conversation>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, agent, name, opened, started_at, spoke_at,
                    runs_at, runs_what, ran_at, asked_by,
                    came_from, carries_on, carries_on_at,
                    watches, watches_what, saw, saw_note, seeing, looked_at,
                    woke_at, woke_today, woke_on, unsettled, misses, paused,
                    goal, goal_at, goal_tries, goal_left, goal_over, routine_off
               FROM conversations
              WHERE runs_at IS NOT NULL AND runs_what IS NOT NULL AND routine_off = 0
              ORDER BY spoke_at DESC",
        )?;
        let rows = q.query_map([], read_conversation)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every conversation with a schedule on it, switched off ones included.
    ///
    /// Separate from `routines` on purpose. The clock must not see a routine
    /// that is switched off, and the panel must, or there is nowhere to switch
    /// it back on from: it would read as having no routine at all, which is
    /// exactly the state pausing exists to avoid.
    pub fn every_routine(&self) -> Result<Vec<Conversation>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, agent, name, opened, started_at, spoke_at,
                    runs_at, runs_what, ran_at, asked_by,
                    came_from, carries_on, carries_on_at,
                    watches, watches_what, saw, saw_note, seeing, looked_at,
                    woke_at, woke_today, woke_on, unsettled, misses, paused,
                    goal, goal_at, goal_tries, goal_left, goal_over, routine_off
               FROM conversations
              WHERE runs_at IS NOT NULL AND runs_what IS NOT NULL
              ORDER BY spoke_at DESC",
        )?;
        let rows = q.query_map([], read_conversation)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Give a conversation the name it will be picked out by.
    pub fn call_it(&self, conversation: &str, name: &str) -> Result<()> {
        let changed = self.conn.lock().unwrap().execute(
            "UPDATE conversations SET name = ? WHERE id = ?",
            params![name, conversation],
        )?;
        Self::only_if_it_is_there(changed, "conversation")
    }

    /// Every thread, the one spoken to most recently first.
    pub fn agents(&self) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, name, title, about, mark, hue, asks, pinned, hidden,
                    cwd, model, started_at, spoke_at, engine, engine_settings
               FROM agents ORDER BY pinned DESC, spoke_at DESC",
        )?;
        let rows = q.query_map([], |r| {
            Ok(Agent {
                id: r.get(0)?,
                name: r.get(1)?,
                title: r.get(2)?,
                about: r.get(3)?,
                mark: r.get(4)?,
                hue: r.get(5)?,
                asks: r.get(6)?,
                pinned: r.get::<_, i64>(7)? != 0,
                hidden: r.get::<_, i64>(8)? != 0,
                cwd: r.get(9)?,
                model: r.get(10)?,
                started_at: r.get(11)?,
                spoke_at: r.get(12)?,
                engine: r.get(13)?,
                engine_settings: r.get(14)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn agent(&self, id: &str) -> Result<Option<Agent>> {
        Ok(self.agents()?.into_iter().find(|t| t.id == id))
    }

    /// Everything said in a thread, in the order it was said.
    pub fn lines(&self, conversation: &str) -> Result<Vec<Line>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT seq, at, kind, text, call, tool, outcome, anchor, pictures
               FROM lines WHERE conversation = ? ORDER BY seq",
        )?;
        let rows = q.query_map([conversation], |r| {
            Ok(Line {
                seq: r.get(0)?,
                at: r.get(1)?,
                kind: r.get(2)?,
                text: r.get(3)?,
                call: r.get(4)?,
                tool: r.get(5)?,
                outcome: r.get(6)?,
                anchor: r.get(7)?,
                pictures: named(r.get::<_, Option<String>>(8)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Write down what the person said.
    ///
    /// Their half of the conversation does not come through the engine, so if
    /// it were not written here it would exist only on the screen -- which is
    /// exactly where it was before this file existed.
    pub fn asked(&self, conversation: &str, text: &str) -> Result<Line> {
        self.append(conversation, "mine", text, None, None)
    }

    /// Say that a line somebody wrote has pictures with it.
    ///
    /// Written after the line rather than with it, because the file names carry
    /// the line's own number: a picture belongs to a particular thing somebody
    /// said, and the number is not known until the line exists.
    pub fn pictures_with(&self, conversation: &str, seq: i64, named: &[String]) -> Result<()> {
        let changed = self.conn.lock().unwrap().execute(
            "UPDATE lines SET pictures = ? WHERE conversation = ? AND seq = ?",
            params![serde_json::to_string(named)?, conversation, seq],
        )?;
        Self::only_if_it_is_there(changed, "line")
    }

    /// Write down what happened, if it is the sort of thing worth keeping.
    ///
    /// Returns the line where one was written. Not everything is: a sentence
    /// still being typed is shown and not kept, or the history would hold every
    /// prefix of every sentence; the end of a turn carries the same words as the
    /// last thing said and would be kept twice; and the start of a thread is
    /// something about the thread rather than something said in it.
    pub fn happened(&self, conversation: &str, event: &Event) -> Result<Option<Line>> {
        match event {
            // `opened` is the conversation's, not the agent's, and that is the
            // one flag in here that must be right: it decides whether the next
            // process is started with `--session-id` or `--resume`, and the
            // wrong one fails with an exit code and nothing on stdout at all.
            // The model is the agent's, because it is a property of what is
            // answering rather than of what is being talked about.
            Event::Started { model, .. } => {
                let conn = self.conn.lock().unwrap();
                // `carries_on` is spent the moment something runs here. It is
                // an instruction for the first launch and nothing else, and a
                // conversation that has spoken has a history of its own: told
                // to carry on from its parent a second time it would throw
                // that history away and start again from somebody else's
                // words. Cleared here rather than where the fork is made,
                // because a fork can be made and the app quit before anything
                // runs, and then the next launch still has to be told.
                conn.execute(
                    "UPDATE conversations
                        SET opened = 1, spoke_at = ?, carries_on = 0, carries_on_at = NULL
                      WHERE id = ?",
                    params![now(), conversation],
                )?;
                conn.execute(
                    "UPDATE agents SET model = ?, spoke_at = ?
                      WHERE id = (SELECT agent FROM conversations WHERE id = ?)",
                    params![model, now(), conversation],
                )?;
                Ok(None)
            }
            Event::Said { text, settled } if *settled => self
                .append(conversation, "said", text, None, None)
                .map(Some),
            Event::Said { .. } => Ok(None),
            Event::Doing(step) => self
                .append(
                    conversation,
                    "doing",
                    &step.what,
                    Some(&step.call),
                    Some(&step.tool),
                )
                .map(Some),
            // The outcome goes onto the step it belongs to rather than onto a
            // line of its own, which is how it is shown and how it should be
            // remembered. Joined by the call id both sides carry.
            Event::Did { call, outcome } => {
                let conn = self.conn.lock().unwrap();
                conn.execute(
                    "UPDATE lines SET outcome = ? WHERE conversation = ? AND call = ?
                       AND kind IN ('doing', 'asking')",
                    params![outcome, conversation, call],
                )?;
                Ok(None)
            }
            // A question is not a new thing that happened. It is the step
            // that was already announced, stopping. So it turns that line into
            // a question rather than adding one underneath it, or the thread
            // reads as the same sentence written twice -- once as something
            // being done and once as something being asked about.
            //
            // Joined by the step's own id, which the question carries for
            // exactly this. If there is no step to find, the question stands
            // on its own rather than being lost.
            Event::NeedsYou(ask) => {
                let conn = self.conn.lock().unwrap();
                let turned = conn.execute(
                    "UPDATE lines SET kind = 'asking' WHERE conversation = ? AND call = ? AND kind = 'doing'",
                    params![conversation, &ask.step],
                )?;
                drop(conn);
                match turned {
                    0 => self
                        .append(
                            conversation,
                            "asking",
                            &ask.asking,
                            Some(&ask.step),
                            Some(&ask.tool),
                        )
                        .map(Some),
                    _ => Ok(None),
                }
            }
            Event::Done { .. } => Ok(None),
            Event::Failed { why } => self
                .append(conversation, "ended", why, None, None)
                .map(Some),
        }
    }

    /// Give a thread the name it will be remembered by.
    /// Give an conversation the identity it settled on.
    ///
    /// All of it at once, because it is one decision made in one moment: an
    /// conversation that has worked out what it is for knows its name, its role and
    /// what it looks like at the same time, and writing them separately would
    /// invite a half-named one.
    ///
    /// Only fills in what is still empty, so this can be called after every
    /// first errand without overwriting a name somebody has since changed by
    /// hand. The exception is the name itself when it is still the placeholder.
    pub fn settled_on(&self, conversation: &str, on: &Settled) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE agents
                SET name  = CASE WHEN name = ?2 OR name = '' THEN ?3 ELSE name END,
                    title = COALESCE(title, ?4),
                    about = COALESCE(about, ?5),
                    mark  = COALESCE(mark,  ?6),
                    hue   = COALESCE(hue,   ?7)
              WHERE id = ?1",
            params![
                conversation,
                NOT_YET_NAMED,
                on.name,
                on.title,
                on.about,
                on.mark,
                on.hue
            ],
        )?;
        Ok(())
    }

    /// Change something about an conversation by hand, whatever it settled on.
    pub fn rename(&self, conversation: &str, name: &str, title: &str, about: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE agents SET name = ?2, title = ?3, about = ?4 WHERE id = ?1",
            params![conversation, name, title, about],
        )?;
        Self::only_if_it_is_there(changed, "agent")
    }

    /// Keep it at the top of the list, or stop.
    pub fn pin(&self, conversation: &str, pinned: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE agents SET pinned = ? WHERE id = ?",
            params![pinned as i64, conversation],
        )?;
        Self::only_if_it_is_there(changed, "agent")
    }

    /// Take it out of the list. It keeps working; it is only out of the way.
    pub fn hide(&self, conversation: &str, hidden: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE agents SET hidden = ? WHERE id = ?",
            params![hidden as i64, conversation],
        )?;
        Self::only_if_it_is_there(changed, "agent")
    }

    /// Forget a thread and everything in it.
    pub fn forget(&self, conversation: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM agents WHERE id = ?", [conversation])?;
        Ok(())
    }

    /// Forget one conversation, and everything said in it.
    ///
    /// Never the last one an agent has. A conversation is where an engine
    /// session lives and where the picker points, and an agent with none is a
    /// row the window cannot open: it would have to invent one to show, which
    /// is the app quietly replacing something somebody just deleted.
    ///
    /// The transcript on disk is left alone, as with an agent. It is filed
    /// under the session id and it is somebody's work, not the app's to throw
    /// away on the strength of a menu click.
    pub fn forget_conversation(&self, conversation: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let agent: String = conn
            .query_row(
                "SELECT agent FROM conversations WHERE id = ?",
                [conversation],
                |r| r.get(0),
            )
            .map_err(|_| anyhow::anyhow!("there is no conversation here to delete"))?;
        let left: i64 = conn.query_row(
            "SELECT count(*) FROM conversations WHERE agent = ?",
            [&agent],
            |r| r.get(0),
        )?;
        if left <= 1 {
            anyhow::bail!(
                "this is the only conversation this agent has, and an agent with none is one \
                 the window cannot open. Delete the agent instead, or start another conversation \
                 first."
            );
        }
        let changed = conn.execute("DELETE FROM conversations WHERE id = ?", [conversation])?;
        Self::only_if_it_is_there(changed, "conversation")
    }

    /// The one place a line is written, so the one place a position is decided.
    /// A line the app itself puts into a conversation.
    ///
    /// Not everything in a transcript was said by an agent or typed by
    /// somebody. A goal ending is neither, and dressing it as one or the other
    /// would be a small lie in the one place a person goes to find out what
    /// actually happened.
    pub fn the_app_says(&self, conversation: &str, kind: &str, text: &str) -> Result<Line> {
        self.append(conversation, kind, text, None, None)
    }

    /// The same, for a line an answer will arrive against later.
    ///
    /// `about` is what the answer will name. A line asking somebody to go and
    /// do something has to be recognisable when the conversation is read back,
    /// or reopening it turns a question still being waited on into a note about
    /// something that used to be waited on -- with the agent still sitting
    /// there, and nothing on screen to answer it with.
    pub fn the_app_says_about(
        &self,
        conversation: &str,
        kind: &str,
        text: &str,
        about: &str,
    ) -> Result<Line> {
        self.append(conversation, kind, text, Some(about), None)
    }

    fn append(
        &self,
        conversation: &str,
        kind: &str,
        text: &str,
        call: Option<&str>,
        tool: Option<&str>,
    ) -> Result<Line> {
        let at = now();
        let conn = self.conn.lock().unwrap();
        // Handed out under the same lock as the insert. Two writers -- the
        // window and the engine's own thread -- must never be given the same
        // position, or the conversation that is read back is not the one that
        // happened.
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM lines WHERE conversation = ?",
            [conversation],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO lines (conversation, seq, at, kind, text, call, tool)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![conversation, seq, at, kind, text, call, tool],
        )?;
        // Both: the list of agents is ordered by when the agent last spoke,
        // and an agent does not speak -- its conversations do.
        conn.execute(
            "UPDATE conversations SET spoke_at = ? WHERE id = ?",
            params![at, conversation],
        )?;
        conn.execute(
            "UPDATE agents SET spoke_at = ?
              WHERE id = (SELECT agent FROM conversations WHERE id = ?)",
            params![at, conversation],
        )?;
        Ok(Line {
            seq,
            at,
            kind: kind.to_string(),
            text: text.to_string(),
            call: call.map(str::to_string),
            tool: tool.map(str::to_string),
            outcome: None,
            anchor: None,
            pictures: Vec::new(),
        })
    }
}

/// Where the store lives for a real installation.
impl Store {
    /// Threads with something in them matching `looking_for`.
    ///
    /// Across everything rather than within the open thread, because the
    /// question a person actually has is "which errand was that" and they do
    /// not remember which one it was -- that is the whole reason they are
    /// looking.
    ///
    /// A plain LIKE rather than a full-text index. At a few thousand lines it
    /// is instant and it is one fewer thing that can be out of step with the
    /// table it describes; the day a thread has a novel in it, FTS5 is the
    /// upgrade and this is the thing to replace.
    /// Where in a conversation the words were actually found.
    ///
    /// The expensive half of a search was already being done and then thrown
    /// away: the query below finds the exact line, inside a subquery, and
    /// selects agent columns only. So somebody searching for a phrase they
    /// remember was dropped into whichever of that agent's conversations spoke
    /// most recently, with no highlight, and scrolled for it by hand. It gets
    /// worse the more the app is used as intended.
    ///
    /// One hit per conversation, the most recent. A conversation that says a
    /// word forty times is one place to go and look, not forty.
    pub fn hits(&self, looking_for: &str) -> Result<Vec<Hit>> {
        let looking_for = looking_for.trim();
        if looking_for.is_empty() {
            return Ok(Vec::new());
        }
        let like = like_for(looking_for);
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT c.agent, l.conversation, l.seq, l.kind, l.text
               FROM lines l JOIN conversations c ON c.id = l.conversation
              WHERE l.text LIKE ?1 ESCAPE '\\'
                AND l.seq = (SELECT max(seq) FROM lines
                              WHERE conversation = l.conversation
                                AND text LIKE ?1 ESCAPE '\\')
              ORDER BY l.at DESC
              LIMIT 200",
        )?;
        let rows = q.query_map([&like], |r| {
            Ok(Hit {
                agent: r.get(0)?,
                conversation: r.get(1)?,
                seq: r.get(2)?,
                kind: r.get(3)?,
                snippet: around(&r.get::<_, String>(4)?, looking_for),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn matching(&self, looking_for: &str) -> Result<Vec<Agent>> {
        let looking_for = looking_for.trim();
        if looking_for.is_empty() {
            return self.agents();
        }
        let like = like_for(looking_for);
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, name, title, about, mark, hue, asks, pinned, hidden,
                    cwd, model, started_at, spoke_at, engine, engine_settings
               FROM agents
              WHERE name LIKE ?1 ESCAPE '\\'
                 OR COALESCE(about, '') LIKE ?1 ESCAPE '\\'
                 -- An agent is found by what was said in any of its
                 -- conversations. `agent`, not `conversation`: this asked
                 -- `conversations` for a column it has never had, so every
                 -- non-empty search errored, and a search that errors looks
                 -- from the window exactly like a search that found nothing.
                 OR id IN (SELECT agent FROM conversations
                            WHERE id IN (SELECT conversation FROM lines
                                          WHERE text LIKE ?1 ESCAPE '\\'))
              ORDER BY pinned DESC, spoke_at DESC",
        )?;
        let rows = q.query_map([&like], |r| {
            Ok(Agent {
                id: r.get(0)?,
                name: r.get(1)?,
                title: r.get(2)?,
                about: r.get(3)?,
                mark: r.get(4)?,
                hue: r.get(5)?,
                asks: r.get(6)?,
                pinned: r.get::<_, i64>(7)? != 0,
                hidden: r.get::<_, i64>(8)? != 0,
                cwd: r.get(9)?,
                model: r.get(10)?,
                started_at: r.get(11)?,
                spoke_at: r.get(12)?,
                engine: r.get(13)?,
                engine_settings: r.get(14)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Put this thread on a different engine.
    ///
    /// Every conversation it has goes back to un-opened with it. That flag
    /// records whether *this* engine has run there before, and the answer for
    /// one that has never run is no -- leaving it true would have Claude Code
    /// resume a session it never started, which fails with nothing on stdout to
    /// say why.
    pub fn use_engine(&self, id: &str, engine: &str, settings: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE agents SET engine = ?, engine_settings = ?, model = NULL WHERE id = ?",
            params![engine, settings, id],
        )?;
        Self::only_if_it_is_there(changed, "agent")?;
        // Every one of its conversations, not the agent: `opened` moved to the
        // conversation when conversations arrived and this update did not
        // follow it, which is the quiet kind of wrong. A conversation that has
        // only ever run on a local model still said it had been opened, so the
        // next start under Claude Code asked it to resume a session that never
        // existed and the person was told their history was gone.
        conn.execute(
            "UPDATE conversations SET opened = 0 WHERE agent = ?",
            params![id],
        )?;
        Ok(())
    }

    /// Forget that this conversation was ever opened.
    ///
    /// Only for a session that really is gone. The flag says "resume me", and
    /// when there is nothing to resume, resuming is all it will ever try: the
    /// conversation fails the same way every time with no way back. Clearing
    /// it costs the engine's own memory of the thread and keeps the
    /// conversation, its lines and its history, which is the better half.
    pub fn start_it_again(&self, id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations SET opened = 0 WHERE id = ?",
            params![id],
        )?;
        Ok(())
    }

    /// Write down what somebody said to a question.
    ///
    /// Onto the question rather than under it, the same way an outcome goes
    /// onto its step: a question and its answer are one thing that happened,
    /// and splitting them across two lines makes a reopened thread read as
    /// though it were asked twice.
    pub fn answered(&self, conversation: &str, step: &str, said: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE lines SET outcome = ?
              WHERE conversation = ? AND call = ? AND kind = 'asking'",
            params![said, conversation, step],
        )?;
        Ok(())
    }
}

/// How many rows refer to something that is not there.
fn points_at_nothing(conn: &Connection) -> Result<i64> {
    Ok(
        conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })?,
    )
}

pub fn beside(data_dir: &Path) -> PathBuf {
    data_dir.join("errand.db")
}

/// One note, from a row that selected its columns in order.
fn read_memory(r: &rusqlite::Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        about: r.get(0)?,
        note: r.get(1)?,
        told: r.get(2)?,
        told_at: r.get(3)?,
    })
}

/// A search phrase, turned into something the index will actually accept.
///
/// Every word quoted and joined with OR, because what arrives here is ordinary
/// language and ordinary language contains apostrophes, hyphens and the
/// occasional emoji, any one of which is a syntax error to MATCH. A syntax
/// error is not a poor result, it is a tool that fails, and a tool that fails
/// on "what do I know about O'Brien's invoice" is a tool an agent stops
/// reaching for.
///
/// A word has to keep at least one letter or digit after the filtering. One
/// like `--` survives the character filter and then tokenises to an empty
/// phrase, which the index rejects outright.
fn as_a_query(said: &str) -> String {
    let mut words: Vec<String> = said
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|word| word.chars().count() >= 3 && word.chars().any(char::is_alphanumeric))
        .map(|word| format!("\"{word}\"*"))
        .collect();
    // Enough to say what is wanted. A hundred-word question is not a better
    // search, it is a search that matches everything.
    words.truncate(8);
    words.join(" OR ")
}

/// A stable id from something that identifies the row.
///
/// Not randomness: a note is keyed by agent and handle, and giving the same
/// note the same id makes the row easy to reason about in the store.
fn uuid_like(from: &str) -> String {
    let mut hash: u128 = 0xcbf2_9ce4_8422_2325;
    for byte in from.as_bytes() {
        hash = hash.wrapping_mul(0x1000_0000_01b3) ^ u128::from(*byte);
    }
    format!("{hash:032x}")
}

/// One conversation, from a row that selected its columns in order.
fn read_conversation(r: &rusqlite::Row) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: r.get(0)?,
        agent: r.get(1)?,
        name: r.get(2)?,
        opened: r.get::<_, i64>(3)? != 0,
        started_at: r.get(4)?,
        spoke_at: r.get(5)?,
        runs_at: r.get(6)?,
        runs_what: r.get(7)?,
        ran_at: r.get(8)?,
        routine_off: r.get::<_, i64>(30).unwrap_or(0) != 0,
        asked_by: r.get(9)?,
        came_from: r.get(10)?,
        carries_on: r.get::<_, i64>(11)? != 0,
        carries_on_at: r.get(12)?,
        watches: r.get(13)?,
        watches_what: r.get(14)?,
        saw: r.get(15)?,
        saw_note: r.get(16)?,
        seeing: r.get(17)?,
        looked_at: r.get(18)?,
        woke_at: r.get(19)?,
        woke_today: r.get(20)?,
        woke_on: r.get(21)?,
        unsettled: r.get(22)?,
        misses: r.get(23)?,
        paused: r.get(24)?,
        goal: r.get(25)?,
        goal_at: r.get(26)?,
        goal_tries: r.get(27)?,
        goal_left: r.get(28)?,
        goal_over: r.get(29)?,
    })
}

/// What somebody typed, as a LIKE pattern that means only what they typed.
///
/// A search for `50%` or `a_b` is somebody looking for those characters, not
/// writing a pattern. Unescaped, the first matches everything.
fn like_for(looking_for: &str) -> String {
    format!(
        "%{}%",
        looking_for
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

/// Enough of a line to recognise it by, centred on what was found.
///
/// A whole answer in a search result is a wall to read; the first forty
/// characters of one are usually "Here is what I found:" for every hit. What
/// somebody needs is the words they searched for with enough either side to
/// know which of their conversations this was.
fn around(line: &str, looking_for: &str) -> String {
    const EITHER_SIDE: usize = 60;
    let found = line
        .to_lowercase()
        .find(&looking_for.to_lowercase())
        .unwrap_or(0);
    // Cut on character boundaries, not bytes. A conversation with an em dash or
    // an emoji in it is ordinary, and slicing one in half panics.
    let start = line
        .char_indices()
        .map(|(at, _)| at)
        .take_while(|at| *at <= found.saturating_sub(EITHER_SIDE))
        .last()
        .unwrap_or(0);
    let end = line
        .char_indices()
        .map(|(at, _)| at)
        .find(|at| *at >= found + looking_for.len() + EITHER_SIDE)
        .unwrap_or(line.len());
    let mut said = String::new();
    if start > 0 {
        said.push('\u{2026}');
    }
    said.push_str(line[start..end].trim());
    if end < line.len() {
        said.push('\u{2026}');
    }
    said
}

/// The picture names on a line, from what was written down.
///
/// Nothing at all where there is nothing, and where the column holds something
/// this cannot read. A line whose pictures cannot be parsed is a line with no
/// pictures, not a conversation that refuses to open.
fn named(said: Option<String>) -> Vec<String> {
    said.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// An id for a row nobody else names.
fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Step;

    /// An agent with one conversation, which is the ordinary case.
    ///
    /// Same id for both, as the migration does for every agent that existed
    /// before conversations did: a conversation id is an engine session id, and
    /// the first one has to keep the id the engine already knows.
    fn one(s: &Store, id: &str, cwd: &str) {
        s.begin(id, NOT_YET_NAMED, Path::new(cwd)).unwrap();
        s.begin_conversation(id, id, "First").unwrap();
    }

    fn step(call: &str, what: &str) -> Event {
        Event::Doing(Step {
            what: what.into(),
            tool: "Bash".into(),
            call: call.into(),
        })
    }

    #[test]
    fn nothing_can_be_said_in_a_conversation_nobody_has_begun() {
        // The failure somebody actually saw, on the first thing they ever typed
        // into this app: "FOREIGN KEY constraint failed", in red, where the
        // answer should have been. The line was written before the rows it
        // points at, and nothing anywhere wrote them first.
        let s = Store::in_memory().unwrap();
        assert!(
            s.asked("never-begun", "Show me my unread mail").is_err(),
            "a line was written for a conversation that does not exist"
        );

        // And with the one call that makes an agent exist, the same line lands.
        s.make_sure_it_exists("new-one", NOT_YET_NAMED, Path::new("/tmp/new-one"))
            .unwrap();
        s.asked("new-one", "Show me my unread mail")
            .expect("the first thing said to a new agent");

        // Said twice, because it is called on every message and only the first
        // one finds nothing there.
        s.make_sure_it_exists("new-one", NOT_YET_NAMED, Path::new("/tmp/new-one"))
            .unwrap();
        assert_eq!(s.agents().unwrap().len(), 1, "a second agent appeared");
        assert_eq!(
            s.conversations("new-one").unwrap().len(),
            1,
            "a second conversation appeared"
        );
        // And what was said is still there, once.
        assert_eq!(s.lines("new-one").unwrap().len(), 1);
    }

    #[test]
    fn a_search_says_which_line_it_found_and_not_only_which_agent() {
        // The expensive half was already being done and thrown away: the query
        // finds the exact line, inside a subquery, and selected agent columns
        // only. Somebody searching for a phrase they remember was dropped into
        // whichever conversation spoke most recently, with no highlight.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("a", NOT_YET_NAMED, Path::new("/tmp/a"))
            .unwrap();
        let first = s.conversations("a").unwrap()[0].id.clone();
        s.begin_conversation("second", "a", "Again").unwrap();
        s.the_app_says_about(&first, "said", "The rent receipt is filed.", "")
            .unwrap();
        s.the_app_says_about("second", "said", "Nothing about money here.", "")
            .unwrap();

        let hits = s.hits("rent receipt").unwrap();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].conversation, first, "the wrong conversation");
        assert!(hits[0].seq > 0, "{hits:?}");
        assert!(hits[0].snippet.contains("rent receipt"), "{hits:?}");
    }

    #[test]
    fn a_conversation_that_says_a_word_forty_times_is_one_place_to_go_and_look() {
        // Forty rows for one conversation is a result list nobody reads. The
        // most recent is the one somebody means.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("a", NOT_YET_NAMED, Path::new("/tmp/a"))
            .unwrap();
        let id = s.conversations("a").unwrap()[0].id.clone();
        for n in 1..=5 {
            s.the_app_says_about(&id, "said", &format!("bitcoin, number {n}"), "")
                .unwrap();
        }
        let hits = s.hits("bitcoin").unwrap();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(
            hits[0].snippet.contains("number 5"),
            "not the latest: {hits:?}"
        );
    }

    #[test]
    fn searching_for_a_percent_sign_looks_for_a_percent_sign() {
        // Unescaped, `%` in LIKE matches everything, so a search for "50%"
        // returns every conversation there is and reads as the search being
        // broken in the other direction.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("a", NOT_YET_NAMED, Path::new("/tmp/a"))
            .unwrap();
        let id = s.conversations("a").unwrap()[0].id.clone();
        s.the_app_says_about(&id, "said", "It is up 50% today.", "")
            .unwrap();
        s.the_app_says_about(&id, "said", "Nothing else to report.", "")
            .unwrap();
        assert_eq!(s.hits("50%").unwrap().len(), 1);
        assert!(s.hits("99%").unwrap().is_empty());
    }

    #[test]
    fn a_snippet_is_cut_on_a_letter_rather_than_a_byte() {
        // A conversation with an em dash or an emoji in it is ordinary, and
        // slicing one in half panics rather than looking wrong.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("a", NOT_YET_NAMED, Path::new("/tmp/a"))
            .unwrap();
        let id = s.conversations("a").unwrap()[0].id.clone();
        let long = format!(
            "{} needle {}",
            "\u{2014}".repeat(80),
            "\u{1f680}".repeat(80)
        );
        s.the_app_says_about(&id, "said", &long, "").unwrap();
        let hits = s.hits("needle").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("needle"), "{hits:?}");
        // Cut at both ends, and said to be cut.
        assert!(
            hits[0].snippet.starts_with('\u{2026}'),
            "{:?}",
            hits[0].snippet
        );
        assert!(
            hits[0].snippet.ends_with('\u{2026}'),
            "{:?}",
            hits[0].snippet
        );
    }

    #[test]
    fn a_turn_the_app_was_closed_during_is_known_about_when_it_opens_again() {
        // Nothing anywhere knew this. Quitting Errand mid-turn killed the
        // engine and left a question with no answer and nothing saying why,
        // which from the window is indistinguishable from an app still
        // thinking about it, and stays that way for ever.
        let at = std::env::temp_dir().join("errand-cut-off-test.db");
        let _ = std::fs::remove_file(&at);
        {
            let s = Store::open(&at).unwrap();
            s.make_sure_it_exists("who", NOT_YET_NAMED, Path::new("/tmp/who"))
                .unwrap();
            s.asked("who", "Show me the most important news of today")
                .unwrap();
            s.a_turn_began("who").unwrap();
            assert_eq!(
                s.turns_that_were_cut_off().unwrap(),
                vec!["who".to_string()]
            );
        }

        // The process is gone, which is the whole point: the mark is on disk
        // and not in the memory of the thing that died.
        let s = Store::open(&at).unwrap();
        assert_eq!(
            s.turns_that_were_cut_off().unwrap(),
            vec!["who".to_string()],
            "the app came back not knowing it had been interrupted"
        );

        // And a turn that finished leaves nothing behind to say sorry for.
        s.a_turn_ended("who").unwrap();
        assert!(s.turns_that_were_cut_off().unwrap().is_empty());
        let _ = std::fs::remove_file(&at);
    }

    #[test]
    fn a_turn_that_finished_before_the_app_closed_is_not_apologised_for() {
        // The other half, and the one that would be tiresome to get wrong: a
        // line saying "this was interrupted" under every conversation that ever
        // finished is worse than saying nothing.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("who", NOT_YET_NAMED, Path::new("/tmp/who"))
            .unwrap();
        assert!(s.turns_that_were_cut_off().unwrap().is_empty());
        s.a_turn_began("who").unwrap();
        s.a_turn_ended("who").unwrap();
        assert!(s.turns_that_were_cut_off().unwrap().is_empty());
    }

    #[test]
    fn a_picture_somebody_sent_is_still_on_the_line_when_it_is_read_back() {
        // It used to reach the engine and be thrown away, so the line said
        // "(with a picture)" and a conversation that had been about a picture
        // read afterwards as a conversation about nothing.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("who", NOT_YET_NAMED, Path::new("/tmp/who"))
            .unwrap();
        let line = s.asked("who", "What is wrong with this screen?").unwrap();
        assert!(
            line.pictures.is_empty(),
            "pictures before any were attached"
        );

        s.pictures_with("who", line.seq, &["1-0.png".into(), "1-1.jpg".into()])
            .unwrap();
        let back = s.lines("who").unwrap();
        assert_eq!(back[0].pictures, vec!["1-0.png", "1-1.jpg"]);
        // And the words are the words. The count used to be glued onto the end
        // of what somebody typed, which is then what the transcript says they
        // said.
        assert_eq!(back[0].text, "What is wrong with this screen?");
        assert!(!back[0].text.contains("with a picture"));
    }

    #[test]
    fn a_line_with_nothing_readable_in_its_pictures_is_a_line_with_no_pictures() {
        // A conversation that refuses to open because one column holds
        // something odd is a far worse failure than a picture not showing.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("who", NOT_YET_NAMED, Path::new("/tmp/who"))
            .unwrap();
        let line = s.asked("who", "hello").unwrap();
        s.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE lines SET pictures = 'not json at all' WHERE conversation = 'who' AND seq = ?",
                params![line.seq],
            )
            .unwrap();
        let back = s.lines("who").unwrap();
        assert_eq!(back.len(), 1);
        assert!(back[0].pictures.is_empty());
    }

    #[test]
    fn saying_a_picture_belongs_to_a_line_that_is_not_there_is_an_error() {
        // Quietly updating no rows is how a picture ends up on disk with
        // nothing pointing at it and nobody the wiser.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("who", NOT_YET_NAMED, Path::new("/tmp/who"))
            .unwrap();
        assert!(s.pictures_with("who", 999, &["1-0.png".into()]).is_err());
    }

    #[test]
    fn a_model_that_turns_out_to_hold_more_is_corrected_everywhere_it_was_written_down() {
        // An agent's settings are a copy taken when the model was chosen, and
        // nothing has ever gone back to correct one. So fixing the line in the
        // picker alone leaves every agent already on that model still sending
        // the old number, which is the same silence moved one table over.
        let s = Store::in_memory().unwrap();
        let was = r#"{"provider":"llamacpp","base_url":"http://192.168.1.25:8081","model":"qwen","context_window":32768,"max_tokens":4096}"#;
        s.offer(&Offered {
            id: "o1".into(),
            engine: "local".into(),
            label: "Qwen on the box".into(),
            settings: Some(was.into()),
            backend: None,
            sort: 1,
            mark: String::new(),
        })
        .unwrap();
        s.make_sure_it_exists("who", NOT_YET_NAMED, Path::new("/tmp/who"))
            .unwrap();
        s.use_engine("who", "local", Some(was)).unwrap();

        let mark = what_makes_it_the_same("local", Some(was));
        assert_eq!(s.it_holds(&mark, 65_536).unwrap(), 2, "not both rows");

        // Found by its mark rather than by position: a fresh store already has
        // the Claude lines in the picker, and those have no settings at all.
        let mine = s
            .offered()
            .unwrap()
            .into_iter()
            .find(|o| what_makes_it_the_same(&o.engine, o.settings.as_deref()) == mark)
            .expect("the line is still in the picker");
        let picker: serde_json::Value =
            serde_json::from_str(mine.settings.as_deref().unwrap()).unwrap();
        assert_eq!(picker["context_window"], 65_536);
        // And the reply ceiling with it, which is the half that silently cut
        // long answers off in the middle.
        assert_eq!(picker["max_tokens"], 16_384);
        // Nothing else about it was touched.
        assert_eq!(picker["base_url"], "http://192.168.1.25:8081");
        assert_eq!(picker["provider"], "llamacpp");

        let theirs: serde_json::Value = serde_json::from_str(
            s.agent("who")
                .unwrap()
                .unwrap()
                .engine_settings
                .as_deref()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(theirs["context_window"], 65_536);

        // Saying the same thing again changes nothing, so nothing is said. An
        // app that announced this at every start would be worse than one that
        // never noticed.
        assert_eq!(s.it_holds(&mark, 65_536).unwrap(), 0);
    }

    #[test]
    fn a_model_that_turns_out_to_hold_less_is_corrected_the_same_way() {
        // The direction that fails loudly rather than quietly: too large is
        // refused outright and reads as a broken model.
        let s = Store::in_memory().unwrap();
        let was = r#"{"base_url":"http://box:8081","model":"m","context_window":65536,"max_tokens":16384}"#;
        s.make_sure_it_exists("who", NOT_YET_NAMED, Path::new("/tmp/who"))
            .unwrap();
        s.use_engine("who", "local", Some(was)).unwrap();
        let mark = what_makes_it_the_same("local", Some(was));
        assert_eq!(s.it_holds(&mark, 8_192).unwrap(), 1);
        let theirs: serde_json::Value = serde_json::from_str(
            s.agent("who")
                .unwrap()
                .unwrap()
                .engine_settings
                .as_deref()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(theirs["context_window"], 8_192);
        assert_eq!(theirs["max_tokens"], 4_096, "the floor is the floor");
    }

    #[test]
    fn correcting_one_model_leaves_every_other_model_alone() {
        // Marks are how two settings are known to be the same model, and a
        // careless match here would resize somebody's whole picker.
        let s = Store::in_memory().unwrap();
        let mine = r#"{"base_url":"http://a:1","model":"one","context_window":32768}"#;
        let yours = r#"{"base_url":"http://b:2","model":"two","context_window":32768}"#;
        s.make_sure_it_exists("a", NOT_YET_NAMED, Path::new("/tmp/a"))
            .unwrap();
        s.make_sure_it_exists("b", NOT_YET_NAMED, Path::new("/tmp/b"))
            .unwrap();
        s.use_engine("a", "local", Some(mine)).unwrap();
        s.use_engine("b", "local", Some(yours)).unwrap();

        s.it_holds(&what_makes_it_the_same("local", Some(mine)), 65_536)
            .unwrap();
        let untouched: serde_json::Value = serde_json::from_str(
            s.agent("b")
                .unwrap()
                .unwrap()
                .engine_settings
                .as_deref()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(untouched["context_window"], 32_768, "the other model moved");
    }

    #[test]
    fn a_routine_switched_off_is_still_there_when_it_is_switched_back_on() {
        // Going away for a week used to destroy the routine: the only stop
        // there was cleared the schedule, what it says and when it last ran, in
        // one statement, so coming back meant setting it up again from memory.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("brief", NOT_YET_NAMED, Path::new("/tmp/brief"))
            .unwrap();
        let id = s.conversations("brief").unwrap()[0].id.clone();
        s.runs(&id, Some("daily 07:00"), Some("What moved overnight"))
            .unwrap();
        assert_eq!(s.routines().unwrap().len(), 1);

        s.routine_off(&id, true).unwrap();
        // The clock walks past it.
        assert!(
            s.routines().unwrap().is_empty(),
            "a paused routine still ran"
        );
        // And nothing about it was thrown away.
        let still = s.conversations("brief").unwrap()[0].clone();
        assert_eq!(still.runs_at.as_deref(), Some("daily 07:00"));
        assert_eq!(still.runs_what.as_deref(), Some("What moved overnight"));
        assert!(still.routine_off);

        s.routine_off(&id, false).unwrap();
        assert_eq!(s.routines().unwrap().len(), 1);
    }

    #[test]
    fn a_routine_keeps_a_record_of_what_it_actually_did() {
        // The question about a standing job is not when it is next but whether
        // it has been working, and one `ran_at` column cannot answer it. Three
        // failed mornings leave a conversation looking merely quiet.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("brief", NOT_YET_NAMED, Path::new("/tmp/brief"))
            .unwrap();
        let id = s.conversations("brief").unwrap()[0].id.clone();
        assert!(s.how_it_has_been_going(&id, 10).unwrap().is_empty());

        let monday = s.a_run_began(&id, "clock").unwrap();
        s.a_run_ended(monday, "done").unwrap();
        let tuesday = s.a_run_began(&id, "clock").unwrap();
        s.a_run_ended(tuesday, "the model server is not answering")
            .unwrap();
        // Started and never finished, which is its own outcome: the app was
        // quit, or the machine slept.
        s.a_run_began(&id, "hand").unwrap();

        let went = s.how_it_has_been_going(&id, 10).unwrap();
        // Three, not one. All three landed in the same millisecond here, which
        // is the case a key made out of the clock quietly turns into one run.
        assert_eq!(went.len(), 3, "{went:?}");
        // Newest first, because that is the one being asked about.
        assert_eq!(went[0].why, "hand");
        assert_eq!(went[0].outcome, None);
        assert_eq!(
            went[1].outcome.as_deref(),
            Some("the model server is not answering")
        );
        assert_eq!(went[2].outcome.as_deref(), Some("done"));

        // And it does not grow without bound in front of somebody.
        assert_eq!(s.how_it_has_been_going(&id, 2).unwrap().len(), 2);
    }

    #[test]
    fn the_only_conversation_an_agent_has_cannot_be_deleted_out_from_under_it() {
        // An agent with no conversation is a row the window cannot open: it
        // would have to invent one to show, which is the app quietly replacing
        // the thing somebody just deleted.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("solo", NOT_YET_NAMED, Path::new("/tmp/solo"))
            .unwrap();
        let only = s.conversations("solo").unwrap();
        assert_eq!(only.len(), 1);
        assert!(
            s.forget_conversation(&only[0].id).is_err(),
            "the last conversation was deleted"
        );

        // With a second one, either can go, and what was said in it goes too.
        s.begin_conversation("second", "solo", "Again").unwrap();
        s.the_app_says_about("second", "said", "Something.", "")
            .unwrap();
        s.forget_conversation("second").unwrap();
        assert_eq!(s.conversations("solo").unwrap().len(), 1);
        assert!(s.lines("second").unwrap().is_empty());

        // And one that is not there is said so rather than passing quietly.
        assert!(s.forget_conversation("second").is_err());
    }

    #[test]
    fn an_answer_that_arrived_while_nobody_was_looking_is_new_until_it_is_read() {
        // The whole point of this app is errands that run while nobody is
        // looking, and the window had no way at all to say that one had. An
        // agent that produced a briefing at seven this morning looked exactly
        // like one that had not run in a month.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("morning", NOT_YET_NAMED, Path::new("/tmp/morning"))
            .unwrap();
        assert!(
            s.what_is_new().unwrap().is_empty(),
            "new before anything happened"
        );

        // What somebody typed is never new. They read it as they wrote it, and
        // counting it means a conversation just finished is unread on closing.
        s.asked("morning", "Every morning, tell me what moved")
            .unwrap();
        assert!(
            s.what_is_new().unwrap().is_empty(),
            "somebody's own words came back as something they had not read"
        );

        s.the_app_says_about("morning", "said", "BTC is flat.", "")
            .unwrap();
        s.the_app_says_about("morning", "said", "And gold is up.", "")
            .unwrap();
        let new = s.what_is_new().unwrap();
        assert_eq!(new.get("morning").map(|f| f.lines), Some(2), "{new:?}");
        assert!(new["morning"].at > 0, "{new:?}");

        s.seen("morning").unwrap();
        assert!(
            s.what_is_new().unwrap().is_empty(),
            "reading it did not clear it"
        );

        // And the next one is new again, which is the state that matters most:
        // an agent that keeps working after somebody has looked away.
        s.the_app_says_about("morning", "said", "Silver moved too.", "")
            .unwrap();
        assert_eq!(s.what_is_new().unwrap()["morning"].lines, 1);
    }

    #[test]
    fn nothing_in_a_conversation_nobody_has_opened_counts_as_read() {
        // Marked from the newest line rather than from the clock. Between
        // reading a conversation and writing this down an agent can say
        // something else, and dating it now would mark that line read without
        // anybody having laid eyes on it.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("late", NOT_YET_NAMED, Path::new("/tmp/late"))
            .unwrap();
        s.the_app_says_about("late", "said", "One.", "").unwrap();
        s.seen("late").unwrap();

        s.the_app_says_about("late", "said", "Two, after you looked away.", "")
            .unwrap();
        assert_eq!(
            s.what_is_new().unwrap()["late"].lines,
            1,
            "a line said after the last look was counted as read"
        );
    }

    #[test]
    fn nothing_is_connected_until_somebody_connects_it() {
        // The switch is what protects a person's mail, not the wall: these are
        // run by the app rather than by the walled engine. So the starting
        // state has to be off, and being on has to be something somebody did.
        let s = Store::in_memory().unwrap();
        assert!(s.connected().unwrap().is_empty());

        s.connect("mail", true).unwrap();
        assert_eq!(s.connected().unwrap(), vec!["mail".to_string()]);
        // Twice is once. A switch that counts presses is a switch that can be
        // half on.
        s.connect("mail", true).unwrap();
        assert_eq!(s.connected().unwrap(), vec!["mail".to_string()]);

        s.connect("mail", false).unwrap();
        assert!(s.connected().unwrap().is_empty());
        // And turning off something already off is the state being asked for.
        s.connect("mail", false).unwrap();
    }

    #[test]
    fn a_new_agent_is_walled_in_rather_than_asking_about_everything() {
        // The cost of asking, watched: somebody gave the same errand to this
        // and to something else, and this one stopped four times before it
        // reached the thing it was asked to do. Half of these run at seven in
        // the morning with nobody at the window, where a card is not a question
        // but a refusal.
        //
        // Not "no permission model": asking and a wall are the two mechanisms,
        // and turning one off is exactly when the other has to be on.
        let s = Store::in_memory().unwrap();
        s.make_sure_it_exists("new", NOT_YET_NAMED, Path::new("/tmp/new"))
            .unwrap();
        assert_eq!(s.agent("new").unwrap().expect("an agent").asks, "auto");

        // And it is still somebody's to change afterwards.
        s.asks("new", "ask").unwrap();
        assert_eq!(s.agent("new").unwrap().expect("an agent").asks, "ask");
    }

    #[test]
    fn a_setting_written_against_nothing_says_so_rather_than_reporting_success() {
        // The quiet half of the same fault. SQLite answers "0 rows" and calls
        // it success, so a routine set on an agent that did not exist yet was
        // accepted here, agreed to on screen, and kept by nobody: the panel
        // went straight back to saying "this runs only when you ask it to", and
        // the only way to find out was the morning it did not happen.
        //
        // Seven of those were found in one afternoon, which is the argument for
        // this being a failure rather than a rule to remember.
        let s = Store::in_memory().unwrap();
        assert!(s
            .runs("nobody", Some("daily 07:00"), Some("Morning"))
            .is_err());
        assert!(s
            .watch("nobody", Some("~/Downloads every 10m"), Some("Tell me"))
            .is_err());
        assert!(s.aim_at("nobody", Some("Get it done"), 1).is_err());
        assert!(s.call_it("nobody", "First").is_err());
        assert!(s.rename("nobody", "Name", "Title", "About").is_err());
        assert!(s.pin("nobody", true).is_err());
        assert!(s.hide("nobody", true).is_err());
        assert!(s.use_engine("nobody", "claude", None).is_err());

        // And every one of them lands the moment the agent exists, which is the
        // other half: a guard that refuses the ordinary case is worse than none.
        s.make_sure_it_exists("here", NOT_YET_NAMED, Path::new("/tmp/here"))
            .unwrap();
        s.runs("here", Some("daily 07:00"), Some("Morning"))
            .unwrap();
        s.watch("here", Some("~/Downloads every 10m"), Some("Tell me"))
            .unwrap();
        s.aim_at("here", Some("Get it done"), 1).unwrap();
        s.call_it("here", "First").unwrap();
        s.rename("here", "Name", "Title", "About").unwrap();
        s.pin("here", true).unwrap();
        s.hide("here", true).unwrap();
        s.use_engine("here", "claude", None).unwrap();

        // Said, not merely accepted.
        let c = s.conversation("here").unwrap().expect("it is there");
        assert_eq!(c.runs_at.as_deref(), Some("daily 07:00"));
        assert_eq!(c.goal.as_deref(), Some("Get it done"));
        assert_eq!(s.agent("here").unwrap().expect("an agent").name, "Name");
    }

    #[test]
    fn a_change_answers_for_what_it_broke_and_not_for_what_it_found_broken() {
        // A store can carry rows pointing at nothing from long before -- a
        // delete made somewhere with foreign keys off is all it takes. Judging
        // each change on the total rather than on the difference makes the next
        // change to come along answer for all of it, and the way that shows up
        // is the app refusing to start with a message naming the one change
        // that is innocent.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.asked("a1", "Something").unwrap();

        {
            let conn = s.conn.lock().unwrap();
            conn.pragma_update(None, "foreign_keys", false).unwrap();
            conn.execute("DELETE FROM conversations WHERE id = 'a1'", [])
                .unwrap();
            conn.pragma_update(None, "foreign_keys", true).unwrap();
            assert!(
                points_at_nothing(&conn).unwrap() > 0,
                "the damage this test is about did not happen"
            );
        }

        // Bringing it up to date again applies nothing, and must still not
        // treat what it found as something it did.
        s.bring_up_to_date()
            .expect("it blamed itself for damage that was already there");
    }

    #[test]
    fn setting_a_watch_forgets_everything_the_old_one_had_seen() {
        // What a watch saw is only meaningful against the thing it watched.
        // Kept across a change it would be compared against something else
        // entirely, which is a change that never happened.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.watch("a1", Some("/tmp every 10m"), Some("sort them"))
            .unwrap();
        s.woke("a1", "files aaa", "one.pdf", 20260829).unwrap();
        assert_eq!(
            s.conversation("a1").unwrap().unwrap().saw.as_deref(),
            Some("files aaa")
        );

        s.watch("a1", Some("https://example.com every 1h"), Some("read it"))
            .unwrap();
        let after = s.conversation("a1").unwrap().unwrap();
        assert_eq!(after.saw, None, "it kept a folder's mark against a page");
        assert_eq!(after.seeing, None);
        assert_eq!(after.woke_today, 0);
        assert_eq!(after.paused, None);
    }

    #[test]
    fn a_watch_and_a_schedule_on_one_conversation_do_not_read_each_other() {
        // The whole argument for not putting the interval in `runs_at`: a
        // briefing every morning that also wakes when the folder it reports on
        // changes is an ordinary thing to want.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.runs("a1", Some("daily 07:00"), Some("the briefing"))
            .unwrap();
        s.watch("a1", Some("/tmp every 10m"), Some("sort them"))
            .unwrap();

        let both = s.conversation("a1").unwrap().unwrap();
        assert_eq!(both.runs_at.as_deref(), Some("daily 07:00"));
        assert_eq!(both.watches.as_deref(), Some("/tmp every 10m"));
        assert_eq!(both.runs_what.as_deref(), Some("the briefing"));
        assert_eq!(both.watches_what.as_deref(), Some("sort them"));
    }

    #[test]
    fn waking_counts_up_within_a_day_and_starts_again_on_the_next() {
        // The count is what stops a watch costing more than an hourly routine,
        // so it has to be a count of today and not of ever.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.watch("a1", Some("/tmp every 10m"), Some("go")).unwrap();

        s.woke("a1", "files a", "", 20260829).unwrap();
        s.woke("a1", "files b", "", 20260829).unwrap();
        assert_eq!(s.conversation("a1").unwrap().unwrap().woke_today, 2);

        s.woke("a1", "files c", "", 20260830).unwrap();
        let tomorrow = s.conversation("a1").unwrap().unwrap();
        assert_eq!(tomorrow.woke_today, 1, "yesterday's count carried over");
        assert_eq!(tomorrow.woke_on, Some(20260830));
    }

    #[test]
    fn a_watch_that_stopped_is_not_looked_at_until_somebody_says_so() {
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.watch("a1", Some("/tmp every 10m"), Some("go")).unwrap();
        assert_eq!(s.watching().unwrap().len(), 1);

        s.pause_watch("a1", "it is different every time I look")
            .unwrap();
        assert!(
            s.watching().unwrap().is_empty(),
            "a stopped watch kept looking"
        );
        assert_eq!(
            s.watchers().unwrap().len(),
            1,
            "it vanished instead of stopping"
        );

        s.look_again("a1").unwrap();
        assert_eq!(s.watching().unwrap().len(), 1);
        assert_eq!(s.conversation("a1").unwrap().unwrap().unsettled, 0);
    }

    #[test]
    fn a_carried_on_conversation_inherits_no_watch() {
        // The same danger as an inherited schedule: a second thing looking at
        // the world and spending money that nobody set up.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.watch("a1", Some("/tmp every 10m"), Some("go")).unwrap();
        s.asked("a1", "something").unwrap();

        s.carry_on("fork", "a1", 1, "First, again", None).unwrap();
        let made = s.conversation("fork").unwrap().unwrap();
        assert_eq!(made.watches, None, "it inherited a watch");
        assert_eq!(made.watches_what, None);
    }

    #[test]
    fn a_conversation_that_has_run_is_no_longer_told_to_carry_on_from_anywhere() {
        // The instruction is for the first launch and nothing else. Left set,
        // a conversation that has spoken would be told to carry on from its
        // parent again, throwing away everything said in it and starting from
        // somebody else's words. Where it came from is kept for ever; the
        // instruction is not.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.asked("a1", "something").unwrap();
        s.carry_on("fork", "a1", 1, "First, again", Some("earlier"))
            .unwrap();
        assert!(s.conversation("fork").unwrap().unwrap().carries_on);

        s.happened(
            "fork",
            &Event::Started {
                session: "fork".into(),
                model: "claude-opus-5".into(),
            },
        )
        .unwrap();

        let after = s.conversation("fork").unwrap().unwrap();
        assert!(!after.carries_on, "it would carry on from its parent again");
        assert_eq!(after.carries_on_at, None);
        assert!(after.opened);
        assert_eq!(
            after.came_from.as_deref(),
            Some("a1"),
            "the way back was thrown away with the instruction"
        );
    }

    #[test]
    fn carrying_a_conversation_on_copies_what_came_before_and_removes_nothing() {
        // The whole safety of going back to an earlier point: it makes a
        // second conversation rather than shortening the first, so being wrong
        // about where to go back to costs nothing but a conversation nobody
        // uses.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.asked("a1", "first thing").unwrap();
        s.asked("a1", "second thing").unwrap();
        s.asked("a1", "third thing").unwrap();

        s.carry_on("fork", "a1", 2, "First, again", Some("uuid-2"))
            .unwrap();

        let kept = s.lines("fork").unwrap();
        assert_eq!(
            kept.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            ["first thing", "second thing"],
            "it took the wrong slice"
        );
        assert_eq!(
            kept.iter().map(|l| l.seq).collect::<Vec<_>>(),
            [1, 2],
            "seq was re-derived, so the next line will land in the wrong place"
        );
        assert_eq!(
            s.lines("a1").unwrap().len(),
            3,
            "it took something out of the conversation it came from"
        );

        let made = s.conversation("fork").unwrap().expect("the new one");
        assert_eq!(made.agent, "a1", "it did not land under the same agent");
        assert_eq!(made.came_from.as_deref(), Some("a1"));
        assert!(made.carries_on, "the first launch has nothing to go on");
        assert_eq!(made.carries_on_at.as_deref(), Some("uuid-2"));
        assert!(!made.opened, "it claimed to have run already");
    }

    #[test]
    fn a_carried_on_conversation_inherits_no_schedule_and_no_chain() {
        // The two most dangerous things to inherit. A carried-on routine is a
        // second thing firing at seven every morning that nobody set up, and
        // an inherited chain makes it refuse hand-offs it has nothing to do
        // with.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.begin_conversation_for("c2", "a1", "Asked by somebody", Some("a1"))
            .unwrap();
        s.runs("c2", Some("daily 07:00"), Some("the briefing"))
            .unwrap();
        s.asked("c2", "something").unwrap();

        s.carry_on("fork", "c2", 1, "Asked by somebody, again", None)
            .unwrap();
        let made = s.conversation("fork").unwrap().expect("the new one");
        assert_eq!(made.runs_at, None, "it inherited a schedule");
        assert_eq!(made.runs_what, None);
        assert_eq!(made.asked_by, None, "it inherited a delegation chain");
    }

    #[test]
    fn a_name_that_is_taken_gets_a_number_rather_than_a_collision() {
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        assert_eq!(s.a_name_like("a1", "First, again").unwrap(), "First, again");
        s.begin_conversation("c2", "a1", "First, again").unwrap();
        assert_eq!(
            s.a_name_like("a1", "First, again").unwrap(),
            "First, again 2"
        );
    }

    #[test]
    fn an_ordinary_conversation_carries_on_from_nothing() {
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        let made = s.conversation("a1").unwrap().unwrap();
        assert_eq!(made.came_from, None);
        assert!(!made.carries_on);
    }

    #[test]
    fn a_note_written_twice_under_one_handle_is_one_note_and_a_correction() {
        // Without a handle there is no key, so nothing can ever be corrected
        // and both the old answer and the new one sit there for ever with no
        // way to settle which is true.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.remember("a1", "where_the_briefing_goes", "By email to Sarah")
            .unwrap();
        s.remember("a1", "where_the_briefing_goes", "By Telegram, not email")
            .unwrap();

        let kept = s.remembers("a1", 10).unwrap();
        assert_eq!(kept.len(), 1, "it kept both answers: {kept:?}");
        assert_eq!(kept[0].note, "By Telegram, not email");
        assert_eq!(kept[0].told, 2, "it did not count as a confirmation");

        // And the old wording must be gone from the search as well as the
        // table. This is the update trigger, and without it a correction
        // leaves the thing it corrected findable for ever.
        assert!(
            s.recall("a1", "email Sarah", 5)
                .unwrap()
                .iter()
                .all(|m| m.note.contains("Telegram")),
            "the wording it replaced is still findable"
        );
    }

    #[test]
    fn looking_something_up_finds_it_by_what_it_is_about_before_its_wording() {
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.remember("a1", "invoice_template", "Use the Acme one")
            .unwrap();
        s.remember("a1", "how_to_export", "It needs --force or it says nothing")
            .unwrap();

        let found = s.recall("a1", "which invoice template", 5).unwrap();
        assert_eq!(
            found.first().map(|m| m.about.as_str()),
            Some("invoice_template")
        );
    }

    #[test]
    fn looking_up_something_nobody_ever_said_finds_nothing_rather_than_the_newest_thing() {
        // The failure this prevents is the quiet one: handing back whatever was
        // most recent when nothing matched, so the agent cannot tell what it
        // was told from what happened to be lying around, and acts on the
        // second as though it were the first.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.remember("a1", "invoice_template", "Use the Acme one")
            .unwrap();
        assert!(s.recall("a1", "pelican husbandry", 5).unwrap().is_empty());
    }

    #[test]
    fn one_agents_notes_are_never_found_by_another() {
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.begin("a2", "Other", Path::new("/tmp/two")).unwrap();
        s.remember("a1", "invoice_template", "Use the Acme one")
            .unwrap();

        assert!(s.recall("a2", "invoice template", 5).unwrap().is_empty());
        assert!(s.remembers("a2", 10).unwrap().is_empty());
    }

    #[test]
    fn a_search_made_of_nothing_but_punctuation_finds_nothing_rather_than_failing() {
        // A syntax error here is not a poor result, it is a tool that fails,
        // and a tool that fails is one an agent stops reaching for.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.remember("a1", "invoice_template", "Use the Acme one")
            .unwrap();
        for nonsense in ["--", "?!", "  ", "\"", "a", "O'Brien's"] {
            let said = s.recall("a1", nonsense, 5);
            assert!(said.is_ok(), "{nonsense:?} was an error: {said:?}");
        }
    }

    #[test]
    fn forgetting_an_agent_takes_its_notes_out_of_the_search_as_well_as_the_table() {
        // Asserted on the search and not only on the table, because the whole
        // reason the notes are an ordinary table with an index over it is that
        // an index has no foreign keys: notes kept only in one would survive
        // the agent, still findable, with nothing able to notice.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.remember("a1", "invoice_template", "Use the Acme one")
            .unwrap();
        assert_eq!(s.recall("a1", "invoice", 5).unwrap().len(), 1);

        s.forget("a1").unwrap();
        s.begin("a1", "Reused", Path::new("/tmp/one")).unwrap();
        assert!(
            s.recall("a1", "invoice", 5).unwrap().is_empty(),
            "a forgotten agent's notes are still findable"
        );
    }

    #[test]
    fn taking_one_note_back_says_whether_there_was_one_to_take() {
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.remember("a1", "invoice_template", "Use the Acme one")
            .unwrap();
        assert!(s.forget_note("a1", "invoice_template").unwrap());
        assert!(!s.forget_note("a1", "invoice_template").unwrap());
        assert!(s.remembers("a1", 10).unwrap().is_empty());
    }

    #[test]
    fn how_often_something_has_come_up_is_what_puts_it_at_the_top() {
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.remember("a1", "rarely", "once").unwrap();
        s.remember("a1", "often", "twice").unwrap();
        s.remember("a1", "often", "twice").unwrap();

        let kept = s.remembers("a1", 10).unwrap();
        assert_eq!(kept.first().map(|m| m.about.as_str()), Some("often"));
    }

    #[test]
    fn searching_for_something_that_was_said_finds_the_agent_that_said_it() {
        // The whole point of the search box, and it errored outright on every
        // non-empty search: the subquery asked `conversations` for a column
        // called `conversation`, which that table has never had. Nothing tested
        // it, and a search that returns an error looks from the window exactly
        // like a search that found nothing.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.asked("a1", "the quarterly invoice for Acme").unwrap();
        s.begin("a2", "Other", Path::new("/tmp/two")).unwrap();
        s.begin_conversation("c2", "a2", "First").unwrap();
        s.asked("c2", "something else entirely").unwrap();

        let found = s
            .matching("quarterly")
            .expect("searching must not be an error");
        assert_eq!(
            found.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            ["a1"],
            "it did not find the agent whose conversation contains the word"
        );

        // And a word nobody said finds nobody, rather than everybody.
        assert!(s.matching("pelican").unwrap().is_empty());
    }

    #[test]
    fn an_always_granted_under_one_engine_is_still_in_force_under_the_other() {
        // The allowlist is the app's, not an engine's, and the two engines name
        // the same tool differently on the wire. Both are converted to the
        // plain name before they reach here, so one row serves both and a yes
        // given while Claude Code was answering still holds when a local model
        // is. If this ever fails, somebody has started writing the prefixed
        // name down and the window will show two entries for one permission.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        s.allow("a1", "ask", "").unwrap();

        assert!(s
            .already_allowed("a1", "ask", "Scribe: Draft a reply to Sarah.")
            .unwrap());
        assert!(s
            .already_allowed("a1", "ask", "Somebody Else: anything at all")
            .unwrap());
        assert!(
            !s.already_allowed("a1", "mcp__errand__ask", "Scribe: ...")
                .unwrap(),
            "the prefixed name reached the table, so there are now two of everything"
        );
    }

    #[test]
    fn an_agent_that_was_asked_by_somebody_can_see_who_is_already_waiting() {
        // Two agents that each think the other should handle a job will hand it
        // back and forth for ever, and every round costs a conversation, a
        // process and ten minutes. This is what a hand-off is checked against.
        let s = Store::in_memory().unwrap();
        one(&s, "first", "/tmp/first");
        s.begin("second", "Second", Path::new("/tmp/second"))
            .unwrap();
        s.begin("third", "Third", Path::new("/tmp/third")).unwrap();

        // first asks second, second asks third.
        s.begin_conversation_for("c2", "second", "Asked by First", Some("first"))
            .unwrap();
        s.begin_conversation_for("c3", "third", "Asked by Second", Some("c2"))
            .unwrap();

        assert_eq!(s.who_is_waiting("first").unwrap(), ["first"]);
        assert_eq!(s.who_is_waiting("c2").unwrap(), ["second", "first"]);
        assert_eq!(
            s.who_is_waiting("c3").unwrap(),
            ["third", "second", "first"],
            "the whole chain, or a job can be handed back to somebody two steps up"
        );
    }

    #[test]
    fn an_ordinary_conversation_was_asked_for_by_nobody() {
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp/one");
        assert_eq!(s.conversation("a1").unwrap().unwrap().asked_by, None);
        assert_eq!(s.who_is_waiting("a1").unwrap(), ["a1"]);
    }

    #[test]
    fn a_chain_that_somehow_loops_stops_rather_than_following_it_for_ever() {
        // Reading a chain out of rows is exactly the shape of thing that spins
        // for ever if a row is ever wrong, and this walk happens on the path of
        // every hand-off.
        let s = Store::in_memory().unwrap();
        s.begin("a", "A", Path::new("/tmp/a")).unwrap();
        s.begin_conversation_for("one", "a", "One", Some("two"))
            .unwrap();
        s.begin_conversation_for("two", "a", "Two", Some("one"))
            .unwrap();

        let walked = s.who_is_waiting("one").unwrap();
        assert!(walked.len() <= 12, "it followed the loop: {walked:?}");
    }

    #[test]
    fn a_thread_and_what_was_said_in_it_are_still_there_afterwards() {
        let s = Store::in_memory().unwrap();
        one(&s, "t1", "/tmp/one");
        s.asked("t1", "Check my mail").unwrap();
        s.happened(
            "t1",
            &Event::Started {
                session: "t1".into(),
                model: "claude-opus-5".into(),
            },
        )
        .unwrap();
        s.happened(
            "t1",
            &Event::Said {
                text: "I'll look now.".into(),
                settled: true,
            },
        )
        .unwrap();

        let lines = s.lines("t1").unwrap();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            (lines[0].kind.as_str(), lines[0].text.as_str()),
            ("mine", "Check my mail")
        );
        assert_eq!(lines[1].kind, "said");
        assert_eq!(lines[0].seq, 1, "positions start at one and count up");
        assert_eq!(lines[1].seq, 2);

        let a = s.agent("t1").unwrap().unwrap();
        assert_eq!(a.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(a.cwd, "/tmp/one", "resume is scoped to it, so it is kept");

        let c = s.conversation("t1").unwrap().unwrap();
        assert!(c.opened, "starting is what says a session now exists");
        assert_eq!(c.agent, "t1");
    }

    #[test]
    fn an_outcome_finds_the_step_it_belongs_to_even_after_a_reload() {
        // The bug this exists to prevent: a step carried a tool's name and its
        // outcome carried a call id, so the two could not be joined and a
        // reopened thread showed every step as though it had never answered.
        let s = Store::in_memory().unwrap();
        one(&s, "t1", "/tmp");
        s.happened("t1", &step("toolu_01", "Reading the mail"))
            .unwrap();
        s.happened("t1", &step("toolu_02", "Writing a note"))
            .unwrap();
        s.happened(
            "t1",
            &Event::Did {
                call: "toolu_01".into(),
                outcome: "17 messages".into(),
            },
        )
        .unwrap();

        let lines = s.lines("t1").unwrap();
        assert_eq!(lines[0].outcome.as_deref(), Some("17 messages"));
        assert_eq!(lines[1].outcome, None, "it landed on the wrong step");
    }

    #[test]
    fn what_is_shown_but_not_worth_keeping_is_not_kept() {
        let s = Store::in_memory().unwrap();
        one(&s, "t1", "/tmp");
        // A sentence still being written: shown, so it reads as thinking; not
        // kept, or the history holds every prefix of every sentence.
        s.happened(
            "t1",
            &Event::Said {
                text: "I'll look n".into(),
                settled: false,
            },
        )
        .unwrap();
        // The end of a turn repeats the last thing said.
        s.happened(
            "t1",
            &Event::Done {
                cost: None,
                said: "All done".into(),
            },
        )
        .unwrap();
        assert!(s.lines("t1").unwrap().is_empty());
    }

    #[test]
    fn threads_come_back_with_the_one_spoken_to_last_at_the_top() {
        let s = Store::in_memory().unwrap();
        one(&s, "old", "/tmp");
        one(&s, "new", "/tmp");
        s.asked("old", "say something").unwrap();

        let order: Vec<String> = s.agents().unwrap().into_iter().map(|t| t.id).collect();
        assert_eq!(order, vec!["old", "new"], "speaking to one moves it up");
    }

    #[test]
    fn a_store_built_from_nothing_ends_up_where_a_migrated_one_does() {
        // The failure this guards was made once already: a rename applied
        // across the whole file, change 1 included, so a fresh store created a
        // table the later change was about to rename anyway and fell over. A
        // migration that has run on somebody's machine is history, not a
        // description, and editing it makes new installs diverge from old ones
        // with nothing to say so.
        let store = Store::in_memory().unwrap();
        store
            .begin("a1", NOT_YET_NAMED, std::path::Path::new("/tmp"))
            .unwrap();
        store
            .settled_on(
                "a1",
                &Settled {
                    name: "Scribe".into(),
                    title: "Mail".into(),
                    about: "The inbox.".into(),
                    mark: "mail".into(),
                    hue: "blue".into(),
                },
            )
            .unwrap();

        let agent = store.agent("a1").unwrap().unwrap();
        assert_eq!(agent.name, "Scribe");
        assert_eq!(agent.title.as_deref(), Some("Mail"));
        assert_eq!(agent.mark.as_deref(), Some("mail"));
        assert!(!agent.pinned && !agent.hidden);
    }

    #[test]
    fn an_agent_that_has_been_named_by_hand_is_not_renamed_by_itself() {
        // It settles on a name once. Somebody who then calls it something else
        // has overruled it, and the next errand must not quietly undo that.
        let store = Store::in_memory().unwrap();
        store
            .begin("a1", NOT_YET_NAMED, std::path::Path::new("/tmp"))
            .unwrap();
        store.rename("a1", "Postie", "Mail", "My inbox.").unwrap();

        store
            .settled_on(
                "a1",
                &Settled {
                    name: "Scribe".into(),
                    title: "Correspondence".into(),
                    about: "Something else.".into(),
                    mark: "mail".into(),
                    hue: "blue".into(),
                },
            )
            .unwrap();

        let agent = store.agent("a1").unwrap().unwrap();
        assert_eq!(agent.name, "Postie", "it overwrote a name somebody chose");
        assert_eq!(agent.title.as_deref(), Some("Mail"));
        // The mark was never set by hand, so it is still the agent's to fill.
        assert_eq!(agent.mark.as_deref(), Some("mail"));
    }

    #[test]
    fn an_agent_can_hold_more_than_one_conversation_and_they_do_not_mix() {
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp");
        s.begin_conversation("c2", "a1", "Something else").unwrap();

        s.asked("a1", "the first thing").unwrap();
        s.asked("c2", "the second thing").unwrap();

        assert_eq!(s.lines("a1").unwrap().len(), 1);
        assert_eq!(s.lines("c2").unwrap()[0].text, "the second thing");
        assert_eq!(s.conversations("a1").unwrap().len(), 2);
        // Both start at one. The position is per conversation, and sharing a
        // counter would put the second conversation's first line second.
        assert_eq!(s.lines("c2").unwrap()[0].seq, 1);
    }

    #[test]
    fn an_agent_speaks_whenever_any_of_its_conversations_does() {
        // The list is ordered by when an agent last spoke, and an agent does
        // not speak: its conversations do. Without this an agent used daily
        // through a second conversation sinks to the bottom of the list.
        let s = Store::in_memory().unwrap();
        one(&s, "quiet", "/tmp");
        one(&s, "busy", "/tmp");
        s.begin_conversation("later", "quiet", "A new one").unwrap();
        s.asked("later", "something").unwrap();

        let order: Vec<String> = s.agents().unwrap().into_iter().map(|a| a.id).collect();
        assert_eq!(order.first().map(String::as_str), Some("quiet"));
    }

    #[test]
    fn forgetting_an_agent_takes_its_conversations_and_everything_in_them() {
        // Two cascades deep, which is the one the foreign keys have to be on
        // for. Before conversations there was only one.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp");
        s.begin_conversation("c2", "a1", "Another").unwrap();
        s.asked("c2", "something").unwrap();

        s.forget("a1").unwrap();
        assert!(s.conversations("a1").unwrap().is_empty());
        assert!(s.lines("c2").unwrap().is_empty(), "its lines outlived it");
    }

    #[test]
    fn a_conversation_whose_session_is_gone_can_be_started_again() {
        // What actually happened: the session Claude Code held was no longer
        // there, so every run asked to resume it, failed in the same words,
        // and the routine that had worked for a week never worked again.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp");
        s.happened(
            "a1",
            &Event::Started {
                session: "a1".into(),
                model: "claude".into(),
            },
        )
        .unwrap();
        assert!(s.conversation("a1").unwrap().unwrap().opened);

        s.start_it_again("a1").unwrap();
        assert!(
            !s.conversation("a1").unwrap().unwrap().opened,
            "it has to be able to start over, or it is stuck for good"
        );
    }

    #[test]
    fn changing_engine_forgets_that_every_conversation_had_been_opened() {
        // Not just the one on screen. `opened` decides whether the next start
        // is `--session-id` or `--resume`, and an engine that has never run in
        // a conversation has no session there to resume -- which fails by
        // telling somebody their history is gone.
        let s = Store::in_memory().unwrap();
        one(&s, "a1", "/tmp");
        s.begin_conversation("c2", "a1", "Another").unwrap();
        for c in ["a1", "c2"] {
            s.happened(
                c,
                &Event::Started {
                    session: c.into(),
                    model: "qwen".into(),
                },
            )
            .unwrap();
            assert!(s.conversation(c).unwrap().unwrap().opened);
        }

        s.use_engine("a1", "claude", None).unwrap();
        for c in ["a1", "c2"] {
            assert!(
                !s.conversation(c).unwrap().unwrap().opened,
                "{c} still claims an engine has run in it"
            );
        }
    }

    #[test]
    fn a_pinned_agent_comes_first_however_long_ago_it_spoke() {
        let store = Store::in_memory().unwrap();
        for id in ["old", "new"] {
            store.begin(id, id, std::path::Path::new("/tmp")).unwrap();
        }
        store.pin("old", true).unwrap();
        let order: Vec<String> = store.agents().unwrap().into_iter().map(|a| a.id).collect();
        assert_eq!(order.first().map(String::as_str), Some("old"));
    }

    #[test]
    fn forgetting_a_thread_takes_everything_said_in_it() {
        let s = Store::in_memory().unwrap();
        one(&s, "t1", "/tmp");
        s.asked("t1", "hello").unwrap();
        s.forget("t1").unwrap();
        assert!(s.agents().unwrap().is_empty());
        assert!(
            s.lines("t1").unwrap().is_empty(),
            "the lines outlived the thread"
        );
    }

    #[test]
    fn the_picker_shows_what_was_put_in_it_and_nothing_else() {
        // The whole point of the change: what the dropdown shows is a list
        // somebody keeps, not the result of a search run every time it opens.
        let store = Store::in_memory().expect("a store");

        // A fresh store already has Claude in it, because a picker that starts
        // empty is one that cannot be used until it has been set up, and
        // nobody should have to set up "the thing it came with".
        let seeded = store.offered().expect("the seed");
        assert!(
            seeded
                .iter()
                .any(|o| o.engine == "claude" && o.settings.is_none()),
            "the default was not seeded: {seeded:?}"
        );

        let mine = Offered {
            id: "one".into(),
            engine: "local".into(),
            label: "qwen2.5 on the Mac Studio".into(),
            settings: Some(r#"{"base_url":"http://box:11434","model":"qwen2.5"}"#.into()),
            backend: None,
            sort: 20,
            mark: String::new(),
        };
        store.offer(&mine).expect("offered");
        assert!(store.offered().unwrap().iter().any(|o| o.id == "one"));

        // The same model chosen twice is one line, even when the settings
        // around it differ. That is the case that actually happens: one row
        // seeded from an agent already using it, carrying a context window and
        // a temperature, and one from somebody ticking it in the list. Compared
        // as text those are two models, and the picker showed the same one
        // twice on the first use of the screen built to stop that.
        let again = Offered {
            id: "two".into(),
            label: "the same one, renamed".into(),
            settings: Some(
                r#"{"base_url":"http://box:11434","model":"qwen2.5","context_window":32768}"#
                    .into(),
            ),
            ..mine.clone()
        };
        store.offer(&again).expect("offered again");
        let now = store.offered().unwrap();
        assert_eq!(
            now.iter().filter(|o| o.engine == "local").count(),
            1,
            "the same model went in twice: {now:?}"
        );
        assert_eq!(
            now.iter().find(|o| o.engine == "local").unwrap().label,
            "the same one, renamed"
        );

        store.stop_offering("one").expect("removed");
        assert!(!store.offered().unwrap().iter().any(|o| o.engine == "local"));
    }

    #[test]
    fn the_picker_can_be_put_in_the_order_somebody_wants() {
        // The list is what the dropdown shows, top to bottom, so the order is
        // the point rather than a nicety: what somebody uses most belongs where
        // their eye lands.
        let store = Store::in_memory().expect("a store");
        let names = || {
            store
                .offered()
                .unwrap()
                .into_iter()
                .map(|o| o.label)
                .collect::<Vec<_>>()
        };
        let was = names();
        assert!(was.len() >= 4, "{was:?}");

        let third = store.offered().unwrap()[2].id.clone();
        store.move_it(&third, true).expect("moved");
        let now = names();
        assert_eq!(now[1], was[2], "it did not move up: {now:?}");
        assert_eq!(
            now[2], was[1],
            "the one above it did not move down: {now:?}"
        );

        store.move_it(&third, false).expect("moved back");
        assert_eq!(names(), was, "moving back did not put it back");

        // Pressing up on the top line means nothing by it, and must not be a
        // failure or a silent reshuffle.
        let top = store.offered().unwrap()[0].id.clone();
        store.move_it(&top, true).expect("nothing to do");
        assert_eq!(names(), was);
    }

    #[test]
    fn a_line_in_the_picker_can_be_called_something_somebody_chose() {
        // The seeded ones are named after whatever agent happened to be using
        // them, which is not what a model is called.
        let store = Store::in_memory().expect("a store");
        let one = store.offered().unwrap()[0].id.clone();
        store
            .call_it_something(&one, "The good one")
            .expect("named");
        assert!(store
            .offered()
            .unwrap()
            .iter()
            .any(|o| o.label == "The good one"));
    }

    #[test]
    fn forgetting_a_backend_takes_what_it_was_offering_with_it() {
        // Otherwise the picker keeps a line pointing at an address that is no
        // longer configured, which fails at the moment somebody chooses it
        // rather than at the moment they removed it.
        let store = Store::in_memory().expect("a store");
        store
            .add_backend(&Backend {
                id: "b1".into(),
                label: "DeepSeek".into(),
                provider: "openai-compat".into(),
                base_url: "https://api.deepseek.com".into(),
                has_key: true,
                wire: "openai".into(),
                added_at: 1,
            })
            .expect("added");
        store
            .offer(&Offered {
                id: "o1".into(),
                engine: "local".into(),
                label: "deepseek-chat".into(),
                settings: Some(r#"{"model":"deepseek-chat"}"#.into()),
                backend: Some("b1".into()),
                sort: 5,
                mark: String::new(),
            })
            .expect("offered");

        store.forget_backend("b1").expect("forgotten");
        assert!(store.backends().unwrap().is_empty());
        assert!(
            !store.offered().unwrap().iter().any(|o| o.id == "o1"),
            "the picker kept a line pointing at a backend that is gone"
        );
    }

    #[test]
    fn what_was_spent_is_kept_per_turn_so_any_stretch_of_time_can_be_asked_about() {
        // A running total answers neither "today" nor "this month", and those
        // are the two questions anybody actually has.
        let store = Store::in_memory().expect("a store");
        store
            .begin("a1", NOT_YET_NAMED, std::path::Path::new("/tmp"))
            .expect("an agent");
        store.rename("a1", "Bitcoin Desk", "Markets", "Briefs").ok();

        let day = 24 * 60 * 60 * 1000;
        let now = now();
        store.spent("a1", "c1", 0.20, 2, now).expect("written");
        store.spent("a1", "c2", 0.30, 1, now).expect("written");
        // Older than a day, so a question about today must not count it.
        store
            .spent("a1", "c3", 5.00, 9, now - 3 * day)
            .expect("written");

        let today = store.spending_since(now - day).expect("readable");
        assert_eq!(today.len(), 1, "{today:?}");
        assert!((today[0].dollars - 0.50).abs() < 1e-9, "{today:?}");
        assert_eq!(today[0].turns, 3);
        assert_eq!(today[0].errands, 2);
        assert_eq!(today[0].who, "Bitcoin Desk");

        let everything = store.spending_since(0).expect("readable");
        assert!(
            (everything[0].dollars - 5.50).abs() < 1e-9,
            "{everything:?}"
        );
    }

    #[test]
    fn a_model_on_this_machine_costs_no_dollars_rather_than_zero_of_them() {
        // A row of zeroes would make every total a lie by omission of what it
        // is a total of: five local errands and one paid one is not six.
        let store = Store::in_memory().expect("a store");
        store.spent("a1", "c1", 0.0, 1, now()).expect("nothing");
        store.spent("a1", "c1", -1.0, 1, now()).expect("nothing");
        assert!(store.spending_since(0).expect("readable").is_empty());
    }

    #[test]
    fn what_was_spent_outlives_the_thread_it_was_spent_on() {
        // The money went whether or not the conversation was kept, and losing
        // the record with the thread would quietly understate the total.
        let store = Store::in_memory().expect("a store");
        store
            .begin("a1", NOT_YET_NAMED, std::path::Path::new("/tmp"))
            .expect("an agent");
        store
            .begin_conversation("gone", "a1", "Doomed")
            .expect("a conversation");
        store.spent("a1", "gone", 0.42, 1, now()).expect("written");

        store.forget("gone").expect("forgotten");
        let still = store.spending_since(0).expect("readable");
        assert_eq!(
            still.len(),
            1,
            "the spending went with the thread: {still:?}"
        );
        assert!((still[0].dollars - 0.42).abs() < 1e-9);
    }

    #[test]
    fn a_store_is_readable_by_its_owner_and_by_nobody_else() {
        // Every conversation anybody has ever had with this app is in here.
        // It was being made world-readable, which on a shared machine is every
        // errand, every answer and every note, to anybody.
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("errand-modes-{}", now()));
        let at = beside(&dir);
        let store = Store::open(&at).expect("a store");
        // Something written, so the log beside it exists and is covered too:
        // that file holds the most recent of everything.
        store
            .begin("a1", "First", std::path::Path::new("/tmp"))
            .expect("something written");
        drop(store);
        // Re-opened, because the check happens on open and the files beside it
        // are made by the first write.
        let store = Store::open(&at).expect("opened again");
        drop(store);

        for what in [at.clone(), wal(&at), shm(&at)] {
            if !what.exists() {
                continue;
            }
            let mode = std::fs::metadata(&what).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode,
                0o600,
                "{} is readable by somebody else",
                what.display()
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_store_that_already_exists_is_opened_rather_than_rebuilt() {
        let dir = std::env::temp_dir().join(format!("errand-store-{}", now()));
        let at = beside(&dir);
        {
            let s = Store::open(&at).unwrap();
            one(&s, "t1", "/tmp");
            s.asked("t1", "still here?").unwrap();
        }
        let s = Store::open(&at).unwrap();
        assert_eq!(s.agents().unwrap().len(), 1);
        assert_eq!(s.lines("t1").unwrap()[0].text, "still here?");
        std::fs::remove_dir_all(&dir).ok();
    }
}
