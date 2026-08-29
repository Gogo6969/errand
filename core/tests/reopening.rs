//! Does a thread closed yesterday come back as the same conversation?
//!
//! The whole store rests on one answer, and it is an answer no unit test can
//! give: it needs a real Claude Code, two real processes, and the second one
//! remembering what the first was told. So this talks to the real thing, and is
//! ignored by default because it costs money and several seconds.
//!
//!     cargo test -p errand-core --test reopening -- --ignored --nocapture
//!
//! Multi-threaded on purpose. These wait for events by blocking, and a
//! current-thread runtime cannot both be blocked and be running the task that
//! feeds it -- which looks exactly like the agent saying nothing at all.
//!
//! What it is guarding is a mistake that is very easy to make and completely
//! silent when made: a session is started with `--session-id` exactly once, and
//! every reopening after that is `--resume`. Get it the wrong way round and the
//! process refuses to launch with nothing at all on stdout -- a thread that sits
//! there looking like it is thinking, for ever.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use errand_core::{claude::Claude, Engine, Event};

/// Wait for the turn to end, collecting everything said along the way.
fn until_done(events: &Receiver<Event>) -> String {
    let mut said = String::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        match events.recv_timeout(Duration::from_secs(5)) {
            Ok(Event::Said {
                text,
                settled: true,
            }) => {
                said.push_str(&text);
                said.push(' ');
            }
            Ok(Event::Done { .. }) => return said,
            Ok(Event::Failed { why }) => panic!("it could not: {why}"),
            Ok(_) => continue,
            Err(_) => continue,
        }
    }
    panic!("nothing ended the turn within two minutes; said so far: {said:?}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "talks to the real Claude Code; run with --ignored"]
async fn a_thread_reopened_tomorrow_is_the_same_conversation() {
    let id = format!("{}-1111-4111-8111-{:012x}", "9e5d0a3c", std::process::id());
    let home: PathBuf = std::env::temp_dir().join(format!("errand-reopen-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();

    // First time: a session that does not exist yet.
    let (mut first, events) =
        Claude::open(&id, &home, false, "ask", None, None).expect("starting a thread");
    first
        .say("Remember the word MANGO. Reply with just OK.")
        .unwrap();
    let answered = until_done(&events);
    assert!(
        answered.to_uppercase().contains("OK"),
        "expected it to answer; got {answered:?}"
    );
    first.stop().unwrap();
    drop(events);
    // Let the process actually go, so the second one is not racing it.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Tomorrow: the same thread, reopened. This is the flag that matters.
    let (mut again, events) =
        Claude::open(&id, &home, true, "ask", None, None).expect("reopening the thread");
    again
        .say("What word did I ask you to remember? Reply with just that word.")
        .unwrap();
    let remembered = until_done(&events);
    again.stop().unwrap();

    assert!(
        remembered.to_uppercase().contains("MANGO"),
        "the thread did not remember itself; it said {remembered:?}"
    );
    std::fs::remove_dir_all(&home).ok();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "talks to the real Claude Code; run with --ignored"]
async fn reopening_something_that_was_never_there_says_so_rather_than_hanging() {
    // The failure this is really about is the other one -- starting a session
    // that already exists -- which produces an exit code, one line on stderr,
    // and not one byte on stdout. A reader waiting for the usual opening event
    // waits for ever. Both failures must arrive as something a person can read.
    let home = std::env::temp_dir();
    let never = "00000000-0000-4000-8000-000000000000";
    let (_it, events) =
        Claude::open(never, &home, true, "ask", None, None).expect("spawning at all");

    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if let Ok(Event::Failed { why }) = events.recv_timeout(Duration::from_secs(5)) {
            assert!(!why.trim().is_empty(), "it failed without saying anything");
            return;
        }
    }
    panic!("reopening a thread that never existed neither worked nor complained");
}
