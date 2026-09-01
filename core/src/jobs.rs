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
    /// Held so something can be typed at it. A y/N is the commonest
    /// interactive prompt there is, and with nothing able to answer it the
    /// symptom is indistinguishable from an agent that stopped thinking.
    typing: Arc<tokio::sync::Mutex<Option<tokio::process::ChildStdin>>>,
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

    /// The end of what it has printed, without taking any of it.
    ///
    /// Deliberately not `whats_new`. That advances how much has been handed
    /// over, so a panel calling it would eat the output the model is about to
    /// be given, and the model calling it would blank the panel.
    fn last_few(&self) -> String {
        const A_GLANCE: usize = 600;
        if self.text.len() <= A_GLANCE {
            return self.text.clone();
        }
        let over = self.text.len() - A_GLANCE;
        let cut = (over..self.text.len())
            .find(|i| self.text.is_char_boundary(*i))
            .unwrap_or(self.text.len());
        self.text[cut..].to_string()
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
    /// The last few lines it has printed.
    ///
    /// Read without taking, unlike `look`. Somebody watching a build should not
    /// be able to steal the output the model is about to be given, and the
    /// model asking should not blank the panel. Until this, only the model
    /// could see what a long command was doing, which is the wrong way round
    /// for the one person who can decide to stop it.
    pub tail: String,
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
        // Kept open, so a y/N can be answered. It used to be closed on the
        // grounds that nothing would type at it, which was true and made the
        // commonest interactive prompt there is into an automatic dead end
        // whose symptom is indistinguishable from an agent that stopped
        // thinking. A command that waits for input nobody sends still hangs,
        // but now there is something to send.
        .stdin(Stdio::piped())
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
    // Taken before the child is put away, because after that the only thing
    // holding it is the thing that kills it.
    let typing = Arc::new(tokio::sync::Mutex::new(child.stdin.take()));
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
            typing,
        },
    );
    Ok(Started {
        handle,
        what: what.to_string(),
    })
}

/// Wait for a job, up to a point. Nothing if it is still going.
///
/// For a command started as an ordinary one that turned out to be a long one.
/// Waiting on `.output()` and throwing the process away at a deadline is what
/// this replaces: a build, an install or a long download reached two minutes,
/// everything it had done was discarded, and the model was told to start again
/// with a different tool -- where it hit the same wall at the same place.
pub async fn wait_up_to(handle: &str, patience: std::time::Duration) -> Option<Ended> {
    let until = std::time::Instant::now() + patience;
    loop {
        // Read without taking, so a command that ends is still here for
        // whatever asks about it next.
        let ended = {
            let jobs = table().lock().unwrap();
            // Gone from the table means somebody stopped it while this was
            // waiting, which is an answer: there is nothing left to wait for.
            let job = jobs.get(handle)?;
            let over = *job.over.lock().unwrap();
            over.map(|code| {
                let (said, lost) = job.said.lock().unwrap().whats_new();
                Ended { code, said, lost }
            })
        };
        if let Some(ended) = ended {
            return Some(ended);
        }
        if std::time::Instant::now() >= until {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    }
}

/// A command that finished while somebody was still waiting for it.
#[derive(Debug, Clone)]
pub struct Ended {
    pub code: i32,
    pub said: String,
    /// Bytes of older output that had to be let go, if any.
    pub lost: usize,
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
            tail: j.said.lock().unwrap().last_few(),
        })
        .collect();
    found.sort_by_key(|r| r.started);
    found
}

/// Stop one. True if there was something to stop.
/// Type at a running command.
///
/// Every stdin used to be closed, on the grounds that nothing was going to type
/// at one. That was true, and it made "Continue? [y/N]" -- the commonest
/// interactive prompt there is -- into an automatic dead end: the command sits
/// there for ever, and from outside it is indistinguishable from an agent that
/// stopped thinking.
///
/// A newline is added unless one is already there. A prompt waiting on a line
/// is not answered by a `y` with nothing after it, and that is a mistake that
/// looks exactly like the tool not working.
pub async fn say_to(handle: &str, said: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let typing = {
        let jobs = table().lock().unwrap();
        let job = jobs
            .get(handle)
            .with_context(|| format!("there is nothing running under {handle}"))?;
        job.typing.clone()
    };
    let mut held = typing.lock().await;
    let writing = held
        .as_mut()
        .context("that command is not taking anything typed at it")?;
    let line = match said.ends_with('\n') {
        true => said.to_string(),
        false => format!("{said}\n"),
    };
    writing
        .write_all(line.as_bytes())
        .await
        .context("typing at the command")?;
    writing.flush().await.context("typing at the command")?;
    Ok(())
}

/// Take a finished job out of the table.
///
/// For one that was started as an ordinary command and finished in time: it
/// was never a background job as far as anybody was concerned, and leaving it
/// in the list of what is running would put a finished thing in a panel headed
/// with what is happening now.
pub fn forget(handle: &str) {
    table().lock().unwrap().remove(handle);
}

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

    #[tokio::test(flavor = "multi_thread")]
    async fn a_command_waiting_on_a_y_or_n_can_be_answered() {
        // Every stdin used to be closed, on the grounds that nothing would type
        // at one. That was true, and it made the commonest interactive prompt
        // there is into an automatic dead end whose symptom is exactly the same
        // as an agent that stopped thinking.
        let started = start(
            shell("read answer; echo \"you said $answer\""),
            "read answer",
            "asking something",
            "c-typing",
            0,
        )
        .expect("it started");

        say_to(&started.handle, "yes").await.expect("it took it");
        let over = wait_up_to(&started.handle, std::time::Duration::from_secs(5))
            .await
            .expect("it finished once it had an answer");
        assert_eq!(over.code, 0, "{over:?}");
        assert!(over.said.contains("you said yes"), "{over:?}");
        forget(&started.handle);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_newline_is_added_because_a_prompt_is_waiting_on_a_line() {
        // A prompt waiting on a line is not answered by a `y` with nothing
        // after it, and that mistake looks exactly like the tool not working.
        let started = start(
            shell("read a; read b; echo \"[$a][$b]\""),
            "read twice",
            "asking twice",
            "c-newline",
            0,
        )
        .expect("it started");
        say_to(&started.handle, "one").await.expect("typed");
        // Already ending in one, which must not become two.
        say_to(&started.handle, "two\n").await.expect("typed");
        let over = wait_up_to(&started.handle, std::time::Duration::from_secs(5))
            .await
            .expect("it finished");
        assert!(over.said.contains("[one][two]"), "{over:?}");
        forget(&started.handle);
    }

    #[tokio::test]
    async fn typing_at_something_that_is_not_running_says_so() {
        let why = say_to("nothing-here", "yes")
            .await
            .map(|_| ())
            .expect_err("it claimed to have typed at nothing");
        assert!(why.to_string().contains("nothing running"), "{why}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn waiting_on_a_command_that_finishes_hands_back_what_it_printed() {
        let started = start(
            shell("echo done and dusted"),
            "echo",
            "a quick thing",
            "c-fast",
            0,
        )
        .expect("it started");
        let over = wait_up_to(&started.handle, std::time::Duration::from_secs(5))
            .await
            .expect("it finished inside the time given");
        assert_eq!(over.code, 0);
        assert!(over.said.contains("done and dusted"), "{over:?}");
        forget(&started.handle);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn waiting_on_one_that_does_not_finish_leaves_it_running() {
        // The whole repair: what it has already done is still being done, and
        // still readable, rather than dropped on the floor.
        let started = start(
            shell("echo begun; sleep 30"),
            "sleep 30",
            "a slow thing",
            "c-slow",
            0,
        )
        .expect("it started");
        assert!(
            wait_up_to(&started.handle, std::time::Duration::from_millis(400))
                .await
                .is_none(),
            "it claimed to have finished"
        );
        let so_far = look(&started.handle).expect("still known");
        assert!(so_far.over.is_none());
        assert!(so_far.said.contains("begun"), "{so_far:?}");
        stop(&started.handle);
    }
}
