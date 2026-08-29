//! Can you watch a helper work?
//!
//! Handing part of an errand to a subagent is one step of it, and the window
//! showed that step and then nothing at all until the helper finished. For a
//! long piece of work that is indistinguishable from a hang, which is the
//! single thing this app tries hardest not to look like.
//!
//!     cargo test -p errand-core --test helper_live -- --ignored --nocapture

use std::time::Duration;

use errand_core::{claude::Claude, claude::PickUp, Engine, Event};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "spends a Claude subscription; run with --ignored"]
async fn what_a_helper_is_doing_shows_under_the_step_that_started_it() {
    let here = std::env::temp_dir().join(format!("errand-helper-{}", std::process::id()));
    std::fs::create_dir_all(&here).unwrap();
    std::fs::write(here.join("one.txt"), "x").unwrap();
    std::fs::write(here.join("two.txt"), "x").unwrap();
    let id = format!("aaaaaaaa-bbbb-4ccc-8ddd-{:012x}", std::process::id());

    let (mut it, events) =
        Claude::open(&id, &here, PickUp::New, "auto", None, None, "").expect("starting claude");

    it.say(
        "Use the Task tool to launch one general-purpose subagent that counts \
         the files in this directory. Then tell me the number.",
        &[],
    )
    .expect("asking");

    let deadline = std::time::Instant::now() + Duration::from_secs(240);
    let mut handed_off: Option<String> = None;
    let mut under_it: Vec<String> = Vec::new();
    let mut answered = String::new();

    while std::time::Instant::now() < deadline {
        match events.recv_timeout(Duration::from_secs(5)) {
            Ok(Event::Doing(step)) => {
                println!("  · {}", step.what);
                // By the tool and not by the wording: a step is named from
                // the tool's own description where it has one, which is right
                // and means the wording here is not what to look for.
                if step.tool == "Agent" || step.tool == "Task" {
                    handed_off = Some(step.call.clone());
                }
            }
            Ok(Event::Did { call, outcome }) => {
                if handed_off.as_deref() == Some(call.as_str()) {
                    println!(
                        "      helper: {}",
                        outcome
                            .replace('\n', " ")
                            .chars()
                            .take(80)
                            .collect::<String>()
                    );
                    under_it.push(outcome);
                }
            }
            Ok(Event::Said {
                text,
                settled: true,
            }) => answered.push_str(&text),
            Ok(event) => {
                if event.ends_the_turn() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        }
    }
    it.stop().ok();
    let _ = std::fs::remove_dir_all(&here);

    assert!(handed_off.is_some(), "it never handed anything to a helper");
    assert!(
        !under_it.is_empty(),
        "the helper worked and the window would have shown nothing under the step"
    );
    assert!(
        under_it.iter().any(|o| o.len() > 5),
        "what came back under the step said nothing: {under_it:?}"
    );
    assert!(answered.contains('2'), "it did not answer: {answered}");
}
