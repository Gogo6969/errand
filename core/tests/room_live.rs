//! Does an agent say when it can no longer see the start of a conversation?
//!
//! Dropping the oldest turns to make a request fit is the right thing to do and
//! was always being done. Doing it in silence was the problem: an agent that
//! has quietly forgotten the first half of a conversation is indistinguishable
//! from one that read it and ignored it, and the person is left re-explaining
//! something they are certain they said.
//!
//! Forced rather than waited for. A real conversation reaches this after a very
//! long time, so the window is made small instead.
//!
//!     cargo test -p errand-core --test room_live -- --ignored --nocapture

use std::time::Duration;

use errand_core::local::{LlmSettings, Local};
use errand_core::{Engine, Event};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a local model; run with --ignored"]
async fn an_agent_says_when_the_start_of_the_conversation_falls_out_of_reach() {
    let home = std::env::temp_dir().join("errand-room-live");
    std::fs::create_dir_all(&home).unwrap();
    let model = std::env::var("ERRAND_MODEL").unwrap_or_else(|_| "llama3.2:1b".into());

    let (mut it, events) = Local::open(
        LlmSettings {
            model,
            // Small enough that a few short turns will not fit, which is the
            // whole point: this is the same code path a long conversation
            // reaches, arrived at in seconds instead of hours.
            context_window: 900,
            max_tokens: 96,
            ..Default::default()
        },
        home,
        "auto",
        "",
        None,
    )
    .expect("opening");

    let mut said_it = false;
    for round in 1..=6 {
        it.say(&format!("Say the number {round} and nothing else."), &[])
            .expect("saying");
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        while std::time::Instant::now() < deadline {
            match events.recv_timeout(Duration::from_secs(5)) {
                Ok(Event::Doing(step)) if step.tool == "context" => {
                    println!("round {round}: {}", step.what);
                    said_it = true;
                }
                Ok(event) => {
                    if event.ends_the_turn() {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            }
        }
        if said_it {
            break;
        }
    }
    it.stop().ok();
    assert!(
        said_it,
        "it forgot the start of the conversation and never mentioned it"
    );
}
