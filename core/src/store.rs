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

/// A conversation, as the list of threads shows it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub name: String,
    /// Where the agent works, resolved. Kept because Claude Code scopes a
    /// session to the directory it was started in: reopening from anywhere else
    /// finds no conversation, and reopening the wrong way silently starts an
    /// empty one under the same name. That is a lie a person would not catch.
    pub cwd: String,
    pub model: Option<String>,
    /// Whether Claude has ever run in this thread, which decides how the next
    /// process is spawned. A new session is `--session-id`; every reopen after
    /// that is `--resume`, and getting it the wrong way round is a hard error
    /// with nothing on stdout to explain it.
    pub opened: bool,
    pub started_at: i64,
    pub spoke_at: i64,
    /// `claude`, or `local`.
    pub engine: String,
    /// Where a local model lives and which one, as JSON. Nothing for Claude,
    /// which needs no telling.
    pub engine_settings: Option<String>,
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

    fn bring_up_to_date(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let at: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, change) in CHANGES.iter().enumerate().skip(at as usize) {
            conn.execute_batch(change)
                .with_context(|| format!("applying change {}", i + 1))?;
            conn.pragma_update(None, "user_version", (i + 1) as i64)?;
        }
        Ok(())
    }

    /// Start a thread, or say nothing if it is already there.
    pub fn begin(&self, id: &str, name: &str, cwd: &Path) -> Result<()> {
        let now = now();
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO threads (id, name, cwd, opened, started_at, spoke_at)
             VALUES (?, ?, ?, 0, ?, ?)",
            params![id, name, cwd.to_string_lossy(), now, now],
        )?;
        Ok(())
    }

    /// Every thread, the one spoken to most recently first.
    pub fn threads(&self) -> Result<Vec<Thread>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT id, name, cwd, model, opened, started_at, spoke_at,
                    engine, engine_settings
               FROM threads ORDER BY spoke_at DESC",
        )?;
        let rows = q.query_map([], |r| {
            Ok(Thread {
                id: r.get(0)?,
                name: r.get(1)?,
                cwd: r.get(2)?,
                model: r.get(3)?,
                opened: r.get::<_, i64>(4)? != 0,
                started_at: r.get(5)?,
                spoke_at: r.get(6)?,
                engine: r.get(7)?,
                engine_settings: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn thread(&self, id: &str) -> Result<Option<Thread>> {
        Ok(self.threads()?.into_iter().find(|t| t.id == id))
    }

    /// Everything said in a thread, in the order it was said.
    pub fn lines(&self, thread: &str) -> Result<Vec<Line>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT seq, at, kind, text, call, tool, outcome
               FROM lines WHERE thread = ? ORDER BY seq",
        )?;
        let rows = q.query_map([thread], |r| {
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
    pub fn asked(&self, thread: &str, text: &str) -> Result<Line> {
        self.append(thread, "mine", text, None, None)
    }

    /// Write down what happened, if it is the sort of thing worth keeping.
    ///
    /// Returns the line where one was written. Not everything is: a sentence
    /// still being typed is shown and not kept, or the history would hold every
    /// prefix of every sentence; the end of a turn carries the same words as the
    /// last thing said and would be kept twice; and the start of a thread is
    /// something about the thread rather than something said in it.
    pub fn happened(&self, thread: &str, event: &Event) -> Result<Option<Line>> {
        match event {
            Event::Started { model, .. } => {
                let conn = self.conn.lock().unwrap();
                conn.execute(
                    "UPDATE threads SET model = ?, opened = 1, spoke_at = ? WHERE id = ?",
                    params![model, now(), thread],
                )?;
                Ok(None)
            }
            Event::Said { text, settled } if *settled => {
                self.append(thread, "said", text, None, None).map(Some)
            }
            Event::Said { .. } => Ok(None),
            Event::Doing(step) => self
                .append(
                    thread,
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
                    "UPDATE lines SET outcome = ? WHERE thread = ? AND call = ?
                       AND kind IN ('doing', 'asking')",
                    params![outcome, thread, call],
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
                    "UPDATE lines SET kind = 'asking' WHERE thread = ? AND call = ? AND kind = 'doing'",
                    params![thread, &ask.step],
                )?;
                drop(conn);
                match turned {
                    0 => self
                        .append(
                            thread,
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
            Event::Failed { why } => self.append(thread, "ended", why, None, None).map(Some),
        }
    }

    /// Give a thread the name it will be remembered by.
    pub fn call_it(&self, thread: &str, name: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE threads SET name = ? WHERE id = ?",
            params![name, thread],
        )?;
        Ok(())
    }

    /// Forget a thread and everything in it.
    pub fn forget(&self, thread: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM threads WHERE id = ?", [thread])?;
        Ok(())
    }

    /// The one place a line is written, so the one place a position is decided.
    fn append(
        &self,
        thread: &str,
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
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM lines WHERE thread = ?",
            [thread],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO lines (thread, seq, at, kind, text, call, tool)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![thread, seq, at, kind, text, call, tool],
        )?;
        conn.execute(
            "UPDATE threads SET spoke_at = ? WHERE id = ?",
            params![at, thread],
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
    /// Put this thread on a different engine.
    ///
    /// `opened` goes back to false with it. It records whether *this* engine
    /// has run here before, and the answer for one that has never run is no --
    /// leaving it true would have Claude Code resume a session it never
    /// started, which fails with nothing on stdout to say why.
    pub fn use_engine(&self, id: &str, engine: &str, settings: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE threads SET engine = ?, engine_settings = ?, opened = 0, model = NULL
               WHERE id = ?",
            params![engine, settings, id],
        )?;
        Ok(())
    }

    /// Write down what somebody said to a question.
    ///
    /// Onto the question rather than under it, the same way an outcome goes
    /// onto its step: a question and its answer are one thing that happened,
    /// and splitting them across two lines makes a reopened thread read as
    /// though it were asked twice.
    pub fn answered(&self, thread: &str, step: &str, said: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE lines SET outcome = ? WHERE thread = ? AND call = ? AND kind = 'asking'",
            params![said, thread, step],
        )?;
        Ok(())
    }
}

pub fn beside(data_dir: &Path) -> PathBuf {
    data_dir.join("errand.db")
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

    fn step(call: &str, what: &str) -> Event {
        Event::Doing(Step {
            what: what.into(),
            tool: "Bash".into(),
            call: call.into(),
        })
    }

    #[test]
    fn a_thread_and_what_was_said_in_it_are_still_there_afterwards() {
        let s = Store::in_memory().unwrap();
        s.begin("t1", "New errand", Path::new("/tmp/one")).unwrap();
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

        let t = s.thread("t1").unwrap().unwrap();
        assert_eq!(t.model.as_deref(), Some("claude-opus-5"));
        assert!(t.opened, "starting is what says a session now exists");
        assert_eq!(t.cwd, "/tmp/one", "resume is scoped to it, so it is kept");
    }

    #[test]
    fn an_outcome_finds_the_step_it_belongs_to_even_after_a_reload() {
        // The bug this exists to prevent: a step carried a tool's name and its
        // outcome carried a call id, so the two could not be joined and a
        // reopened thread showed every step as though it had never answered.
        let s = Store::in_memory().unwrap();
        s.begin("t1", "x", Path::new("/tmp")).unwrap();
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
        s.begin("t1", "x", Path::new("/tmp")).unwrap();
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
        s.begin("old", "Older", Path::new("/tmp")).unwrap();
        s.begin("new", "Newer", Path::new("/tmp")).unwrap();
        s.asked("old", "say something").unwrap();

        let order: Vec<String> = s.threads().unwrap().into_iter().map(|t| t.id).collect();
        assert_eq!(order, vec!["old", "new"], "speaking to one moves it up");
    }

    #[test]
    fn forgetting_a_thread_takes_everything_said_in_it() {
        let s = Store::in_memory().unwrap();
        s.begin("t1", "x", Path::new("/tmp")).unwrap();
        s.asked("t1", "hello").unwrap();
        s.forget("t1").unwrap();
        assert!(s.threads().unwrap().is_empty());
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
            s.begin("t1", "Kept", Path::new("/tmp")).unwrap();
            s.asked("t1", "still here?").unwrap();
        }
        let s = Store::open(&at).unwrap();
        assert_eq!(s.threads().unwrap().len(), 1);
        assert_eq!(s.lines("t1").unwrap()[0].text, "still here?");
        std::fs::remove_dir_all(&dir).ok();
    }
}
