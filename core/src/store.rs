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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// The conversation that asked for this one, if it was delegated.
    pub asked_by: Option<String>,
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
];

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

    /// Start a thread, or say nothing if it is already there.
    pub fn begin(&self, id: &str, name: &str, cwd: &Path) -> Result<()> {
        let now = now();
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO agents (id, name, cwd, opened, started_at, spoke_at)
             VALUES (?, ?, ?, 0, ?, ?)",
            params![id, name, cwd.to_string_lossy(), now, now],
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
                    runs_at, runs_what, ran_at, asked_by
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
                    runs_at, runs_what, ran_at, asked_by
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
            .any(|a| a.tool == tool && (a.rule.is_empty() || doing.starts_with(&a.rule))))
    }

    /// Give a conversation a schedule, or take one away.
    ///
    /// `ran_at` is cleared with it. A schedule that has just been set has never
    /// run, whatever the conversation did before, and counting the first run
    /// from an old timestamp would either fire it at once or hold it back by
    /// however long it happened to be since.
    pub fn runs(&self, conversation: &str, at: Option<&str>, what: Option<&str>) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations SET runs_at = ?, runs_what = ?, ran_at = NULL WHERE id = ?",
            params![at, what, conversation],
        )?;
        Ok(())
    }

    /// Say that a routine has just run.
    pub fn ran(&self, conversation: &str, at: i64) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations SET ran_at = ? WHERE id = ?",
            params![at, conversation],
        )?;
        Ok(())
    }

    /// Every conversation with a schedule on it, whichever agent it belongs to.
    pub fn routines(&self) -> Result<Vec<Conversation>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, agent, name, opened, started_at, spoke_at,
                    runs_at, runs_what, ran_at, asked_by
               FROM conversations
              WHERE runs_at IS NOT NULL AND runs_what IS NOT NULL
              ORDER BY spoke_at DESC",
        )?;
        let rows = q.query_map([], read_conversation)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Give a conversation the name it will be picked out by.
    pub fn call_it(&self, conversation: &str, name: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE conversations SET name = ? WHERE id = ?",
            params![name, conversation],
        )?;
        Ok(())
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
            "SELECT seq, at, kind, text, call, tool, outcome
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
                conn.execute(
                    "UPDATE conversations SET opened = 1, spoke_at = ? WHERE id = ?",
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
        conn.execute(
            "UPDATE agents SET name = ?2, title = ?3, about = ?4 WHERE id = ?1",
            params![conversation, name, title, about],
        )?;
        Ok(())
    }

    /// Keep it at the top of the list, or stop.
    pub fn pin(&self, conversation: &str, pinned: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE agents SET pinned = ? WHERE id = ?",
            params![pinned as i64, conversation],
        )?;
        Ok(())
    }

    /// Take it out of the list. It keeps working; it is only out of the way.
    pub fn hide(&self, conversation: &str, hidden: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE agents SET hidden = ? WHERE id = ?",
            params![hidden as i64, conversation],
        )?;
        Ok(())
    }

    /// Forget a thread and everything in it.
    pub fn forget(&self, conversation: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM agents WHERE id = ?", [conversation])?;
        Ok(())
    }

    /// The one place a line is written, so the one place a position is decided.
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
    pub fn matching(&self, looking_for: &str) -> Result<Vec<Agent>> {
        let looking_for = looking_for.trim();
        if looking_for.is_empty() {
            return self.agents();
        }
        let like = format!(
            "%{}%",
            looking_for
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
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
        conn.execute(
            "UPDATE agents SET engine = ?, engine_settings = ?, model = NULL WHERE id = ?",
            params![engine, settings, id],
        )?;
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
        asked_by: r.get(9)?,
    })
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
