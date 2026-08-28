//! One turn on a local model, with every event printed.
//!
//! A diagnostic more than a test: when a turn produces nothing, the question is
//! which of the seven events did not arrive, and that is not visible from
//! either the window or the terminal harness.
//!
//!     cargo test -p errand-core --test local_turn -- --ignored --nocapture

use std::time::{Duration, Instant};

use errand_core::local::{LlmSettings, Local};
use errand_core::{Engine, Event};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a local model; run with --ignored"]
async fn one_turn_with_every_event_it_produces() {
    let home = std::env::temp_dir().join("errand-local-turn");
    std::fs::create_dir_all(&home).unwrap();

    let model = std::env::var("ERRAND_MODEL").unwrap_or_else(|_| "qwen2.5:7b-instruct".into());
    let (mut it, events) = Local::open(
        LlmSettings {
            model,
            ..Default::default()
        },
        home,
    )
    .expect("opening");

    let asking = std::env::var("ERRAND_ASK")
        .unwrap_or_else(|_| "List the files in the working directory.".into());
    println!("--> {asking}");
    it.say(&asking).unwrap();

    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        match events.recv_timeout(Duration::from_secs(10)) {
            Ok(Event::Said { settled: false, .. }) => {}
            Ok(Event::Said { text, .. }) => println!("SAID     {text}"),
            Ok(Event::Doing(step)) => println!("DOING    {} [{}]", step.what, step.tool),
            Ok(Event::Did { outcome, .. }) => println!("DID      {outcome}"),
            Ok(Event::NeedsYou(ask)) => {
                println!("ASKS     {} :: {}", ask.asking, ask.detail);
                it.answer(&ask.call, errand_core::Answer::Yes).unwrap();
            }
            Ok(Event::Started { model, .. }) => println!("STARTED  {model}"),
            Ok(Event::Done { said }) => {
                println!("DONE     {said:?}");
                return;
            }
            Ok(Event::Failed { why }) => {
                println!("FAILED   {why}");
                panic!("the turn failed: {why}");
            }
            Err(_) => println!("         (nothing for ten seconds)"),
        }
    }
    panic!("nothing ended the turn");
}
