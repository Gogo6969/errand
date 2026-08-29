//! Commands that outlive the step that started them.
//!
//! Everything else an errand does finishes while somebody waits for it. A
//! build, a download, a server, a long script does not, and running one the
//! ordinary way means the errand sits on `.output()` until the command is done:
//! no output, no progress, no way to stop it, and if the command never ends
//! then neither does the errand. That happened, and from outside it looked
//! exactly like an agent that had stopped thinking.
//!
//! So a command can be started instead of run. It gets a handle, it keeps
//! running on its own, and it can be asked what it has printed since last time.
//! The asking is deliberate rather than a stream: a model that is handed every
//! line of a build log has spent its context on a build log.
//!
//! They keep running while Errand is open and no longer. A background process
//! that outlives the only thing that knows about it is not a background
//! process, it is a leak, and the one thing worse than a command nobody can see
//! is a command nobody can see or stop.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use serde::Serialize;

/// How much of a command's output is kept.
///
/// The tail rather than the head: what a long-running command is doing now is
/// what anybody wants, and the first kilobyte of a build is the part nobody
/// reads. What falls off the front is counted and said, because output that
/// vanishes silently is worse than output that is missing loudly.
const KEEP_AT_MOST: usize = 64 * 1024;

/// One command that is running on its own.
struct Job {
    what: String,
    command: String,
    conversation: String,
    started: i64,
    said: Arc<Mutex<Kept>>,
    /// Some(code) once it has ended.
    over: Arc<Mutex<Option<i32>>>,
    /// Held so it can be killed. Taken when it ends.
    child: Arc<Mutex<Option<tokio::process::Child>>>,
}

/// What a command has printed, and what had to be let go.
#[derive(Default)]
struct Kept {
    text: String,
    /// How much of `text` has already been handed over.
    given: usize,
    /// Bytes dropped off the front since the last time anybody asked.
    lost: usize,
}

impl Kept {
    fn add(&mut self, more: &str) {
        self.text.push_str(more);
        if self.text.len() <= KEEP_AT_MOST {
            return;
        }
        let over = self.text.len() - KEEP_AT_MOST;
        // On a character boundary, or the string is no longer a string.
        let cut = (over..self.text.len())
            .find(|i| self.text.is_char_boundary(*i))
            .unwrap_or(self.text.len());
        self.text.drain(..cut);
        self.given = self.given.saturating_sub(cut);
        self.lost += cut;
    }

    fn whats_new(&mut self) -> (String, usize) {
        let new = self.text[self.given..].to_string();
        self.given = self.text.len();
        (new, std::mem::take(&mut self.lost))
    }
}

/// A command that has been started.
#[derive(Debug, Clone, Serialize)]
pub struct Started {
    pub handle: String,
    pub what: String,
}

/// What a command has done since anybody last asked.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub handle: String,
    pub what: String,
    /// Everything printed since the last look.
    pub said: String,
    /// How many bytes of older output had to be let go, if any.
    pub lost: usize,
    /// The exit code, once there is one.
    pub over: Option<i32>,
}

/// One running command, as the window shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Running {
    pub handle: String,
    pub what: String,
    pub command: String,
    pub conversation: String,
    pub started: i64,
}

fn table() -> &'static Mutex<HashMap<String, Job>> {
    static JOBS: OnceLock<Mutex<HashMap<String, Job>>> = OnceLock::new();
    JOBS.get_or_init(Default::default)
}

/// Short, and short on purpose: a model has to copy it back accurately, and a
/// UUID is a thing models get wrong.
fn next_handle() -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    format!("job-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Start a command and stop waiting for it.
///
/// `walled` builds the process, so the sandbox that applies to an ordinary
/// command applies to this one too. There is no version of this that is worth
/// having without that.
pub fn start(
    walled: tokio::process::Command,
    command: &str,
    what: &str,
    conversation: &str,
    now: i64,
) -> Result<Started> {
    let mut walled = walled;
    let mut child = walled
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Nothing is going to type at it, and a command that waits for input it
        // will never get is a command that hangs until somebody kills it.
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("starting the command")?;

    let said = Arc::new(Mutex::new(Kept::default()));
    let over = Arc::new(Mutex::new(None));

    // Both streams into the one place, in the order they arrive. Keeping them
    // apart would be tidier and would also mean an error message no longer sits
    // next to the line that caused it.
    if let Some(out) = child.stdout.take() {
        keep_reading(tokio::io::BufReader::new(out), said.clone());
    }
    if let Some(err) = child.stderr.take() {
        keep_reading(tokio::io::BufReader::new(err), said.clone());
    }

    let handle = next_handle();
    let waiting = Arc::new(Mutex::new(Some(child)));
    {
        let over = over.clone();
        let waiting = waiting.clone();
        // Asked rather than awaited, so that the one thing holding the child is
        // still holding it when somebody wants it killed. Awaiting it meant the
        // waiter took the child the instant the job started, and stopping a job
        // then found nothing there to stop. That is a hundred milliseconds of
        // lateness on noticing a command has ended, against the whole feature
        // working.
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let mut held = waiting.lock().unwrap();
                let Some(child) = held.as_mut() else {
                    return;
                };
                match child.try_wait() {
                    Ok(None) => continue,
                    Ok(Some(status)) => {
                        *over.lock().unwrap() = Some(status.code().unwrap_or(-1));
                        *held = None;
                        return;
                    }
                    Err(_) => {
                        *over.lock().unwrap() = Some(-1);
                        *held = None;
                        return;
                    }
                }
            }
        });
    }

    table().lock().unwrap().insert(
        handle.clone(),
        Job {
            what: what.to_string(),
            command: command.to_string(),
            conversation: conversation.to_string(),
            started: now,
            said,
            over,
            child: waiting,
        },
    );
    Ok(Started {
        handle,
        what: what.to_string(),
    })
}

/// Read a pipe until it closes, keeping what comes out of it.
fn keep_reading<R>(from: R, into: Arc<Mutex<Kept>>)
where
    R: tokio::io::AsyncBufRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = from.lines();
        // A line at a time rather than a chunk, so what is kept always ends
        // somewhere a person would end it. Line and newline under one lock, or
        // the other pipe writes a line into the middle of this one.
        while let Ok(Some(line)) = lines.next_line().await {
            into.lock().unwrap().add(&format!("{line}\n"));
        }
    });
}

/// What it has printed since anybody last asked, and whether it is over.
pub fn look(handle: &str) -> Option<Progress> {
    let jobs = table().lock().unwrap();
    let job = jobs.get(handle)?;
    let (said, lost) = job.said.lock().unwrap().whats_new();
    let over = *job.over.lock().unwrap();
    Some(Progress {
        handle: handle.to_string(),
        what: job.what.clone(),
        said,
        lost,
        over,
    })
}

/// Everything still running, oldest first.
pub fn running() -> Vec<Running> {
    let jobs = table().lock().unwrap();
    let mut found: Vec<Running> = jobs
        .iter()
        .filter(|(_, j)| j.over.lock().unwrap().is_none())
        .map(|(handle, j)| Running {
            handle: handle.clone(),
            what: j.what.clone(),
            command: j.command.clone(),
            conversation: j.conversation.clone(),
            started: j.started,
        })
        .collect();
    found.sort_by_key(|r| r.started);
    found
}

/// Stop one. True if there was something to stop.
pub fn stop(handle: &str) -> bool {
    let jobs = table().lock().unwrap();
    let Some(job) = jobs.get(handle) else {
        return false;
    };
    // Marked over here rather than left to the waiter to notice, so that a
    // command is gone from the list the moment somebody stops it. The waiter
    // reaps it and writes the real code over this a moment later.
    let was_running = job.over.lock().unwrap().is_none();
    if let Some(child) = job.child.lock().unwrap().as_mut() {
        let _ = child.start_kill();
    }
    *job.over.lock().unwrap() = Some(-1);
    was_running
}

/// Stop everything. Called when the app is going, because a command nobody can
/// see or stop is worse than one that ended early.
pub fn stop_everything() {
    let handles: Vec<String> = table().lock().unwrap().keys().cloned().collect();
    for handle in handles {
        stop(&handle);
    }
}

/// What to tell a model when it starts one, in the words it should read back.
pub fn in_plain_words(started: &Started) -> String {
    format!(
        "Started, and still running. Its handle is {}. It keeps running while you get on with \
         something else, and after this errand ends, for as long as Errand is open. Call \
         check_command with that handle to see what it has printed since you last asked, and \
         whether it has finished. Do not wait in a loop: do something else, or say what you have \
         so far and check again next time somebody asks.",
        started.handle
    )
}

/// What to tell a model when it asks how one is getting on.
pub fn how_its_going(p: &Progress) -> String {
    let mut said = String::new();
    if p.lost > 0 {
        said.push_str(&format!(
            "({} bytes of earlier output were let go; this is the most recent part.)\n",
            p.lost
        ));
    }
    match p.said.is_empty() {
        true => said.push_str("Nothing new since you last asked.\n"),
        false => said.push_str(&p.said),
    }
    match p.over {
        None => said.push_str("\nStill running."),
        Some(0) => said.push_str("\nFinished, and it succeeded."),
        Some(code) => said.push_str(&format!("\nFinished, and it exited {code}.")),
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(command: &str) -> tokio::process::Command {
        let mut sh = tokio::process::Command::new("/bin/sh");
        sh.arg("-c").arg(command);
        sh
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_command_that_keeps_running_can_be_asked_what_it_has_done_so_far() {
        // The whole point: something that is not finished can still be reported
        // on, because waiting for it to finish is exactly what did not work.
        let started = start(
            shell("echo one; sleep 30; echo two"),
            "echo one; sleep 30; echo two",
            "counting",
            "a-conversation",
            0,
        )
        .expect("it starts");

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let first = look(&started.handle).expect("it is known");
        assert!(first.said.contains("one"), "said: {:?}", first.said);
        assert_eq!(first.over, None, "it should not be over yet");

        // Asked twice, the same output does not come back twice: a model that
        // is handed the whole log on every look spends its context on the log.
        let again = look(&started.handle).expect("still known");
        assert_eq!(again.said, "", "the same output came back twice");

        assert!(stop(&started.handle), "it should have been stopped");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_command_that_ends_says_how_it_ended() {
        let started =
            start(shell("exit 3"), "exit 3", "failing", "a-conversation", 0).expect("it starts");
        for _ in 0..40 {
            if look(&started.handle).and_then(|p| p.over).is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let done = look(&started.handle).expect("it is known");
        assert_eq!(done.over, Some(3));
        assert!(
            how_its_going(&done).contains("exited 3"),
            "{}",
            how_its_going(&done)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn something_still_running_is_listed_and_something_finished_is_not() {
        let long = start(
            shell("sleep 30"),
            "sleep 30",
            "waiting about",
            "this-one",
            10,
        )
        .expect("it starts");
        assert!(running().iter().any(|r| r.handle == long.handle));

        stop(&long.handle);
        assert!(
            !running().iter().any(|r| r.handle == long.handle),
            "a stopped command is still listed as running"
        );
    }

    #[test]
    fn output_that_runs_away_keeps_the_end_and_says_what_it_let_go() {
        // A command that prints forever must not become a memory leak, and
        // output that vanishes without a word is worse than output that is
        // missing loudly.
        let mut kept = Kept::default();
        for i in 0..4000 {
            kept.add(&format!("line {i} with enough words on it to add up\n"));
        }
        let (said, lost) = kept.whats_new();
        assert!(lost > 0, "nothing was reported as let go");
        assert!(said.contains("line 3999"), "the end was not kept");
        assert!(!said.contains("line 0 "), "the beginning was kept");
        assert!(kept.text.len() <= KEEP_AT_MOST);
    }

    #[test]
    fn asking_about_a_command_that_was_never_started_says_nothing_rather_than_lying() {
        assert!(look("job-nonesuch").is_none());
        assert!(!stop("job-nonesuch"));
    }
}
