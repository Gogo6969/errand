//! Does the app hear about a Claude Code process that dies?
//!
//! The only word the app gets that an engine has gone is its event channel
//! closing. A second sender, kept by the task that writes to the process for
//! asking asides, held that channel open after the process died: a turn in
//! the middle of running was never ended, which left its routine skipped
//! until a restart, and the next thing said was the one marked as stopped
//! part way. Seen on the installed app, then written down here.
//!
//! A stand-in for Claude Code rather than the real one, because what is under
//! test is the pipe and not the model: a script on the PATH that answers every
//! line with the two lines a turn needs, and writes down its own pid so that
//! it can be killed.
//!
//! Its own file, because Claude Code is found on the PATH once per process,
//! and this test has to be the one that decides what is on it.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use errand_core::{claude::Claude, claude::PickUp, Engine, Event};

fn until_done(events: &Receiver<Event>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        match events.recv_timeout(Duration::from_secs(1)) {
            Ok(Event::Done { .. }) => return,
            Ok(Event::Failed { why }) => panic!("the stand-in failed: {why}"),
            Ok(_) | Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => panic!("it went away before it answered"),
        }
    }
    panic!("the stand-in never finished a turn");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_app_hears_when_a_claude_code_process_dies_between_turns() {
    let here = std::env::temp_dir().join(format!("errand-claude-dies-{}", std::process::id()));
    let bin = here.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let pid_file = here.join("pid");
    let stand_in = bin.join("claude");
    std::fs::write(
        &stand_in,
        format!(
            "#!/bin/sh\n\
             echo $$ > '{}'\n\
             while IFS= read -r line; do\n\
             echo '{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"s\",\"model\":\"stand-in\"}}'\n\
             echo '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"ok\"}}'\n\
             done\n",
            pid_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(
        &stand_in,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{path}", bin.display()));
    assert_eq!(
        errand_core::claude::where_claude_is(),
        stand_in,
        "the stand-in is not the Claude Code this test is talking to"
    );

    let session = format!("{}-3333-4333-8333-{:012x}", "7d2e9f1b", std::process::id());
    let (mut talk, events) = Claude::open(
        &session,
        &here,
        PickUp::New,
        "ask",
        None,
        None,
        &errand_core::memory::Knowing::default(),
    )
    .expect("starting the stand-in");
    // The hello it is sent on opening, and then a turn of our own.
    until_done(&events);
    talk.say("hello", &[]).unwrap();
    until_done(&events);

    let pid = std::fs::read_to_string(&pid_file).unwrap();
    let killed = std::process::Command::new("kill")
        .args(["-9", pid.trim()])
        .status()
        .unwrap();
    assert!(killed.success(), "could not kill the stand-in");

    // Heard as the channel closing, and soon: nothing is said to it first.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Err(RecvTimeoutError::Disconnected) => break,
            Ok(_) | Err(RecvTimeoutError::Timeout) => {
                assert!(
                    Instant::now() < deadline,
                    "the process died and the app was never told: its events stayed open"
                );
            }
        }
    }
    drop(talk);
    std::fs::remove_dir_all(&here).ok();
}
