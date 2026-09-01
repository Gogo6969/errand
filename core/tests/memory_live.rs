//! Does a model actually reach for its own notes?
//!
//! Everything else about memory is testable without a model: the schema, the
//! ranking, the handle rules, the budget. This is the part that is not, and it
//! is the part the whole feature rests on. A tool nothing calls is a tool that
//! does not exist, and the only thing standing between the two is the wording
//! of a description, which no assertion reaches.
//!
//!     cargo test -p errand-core --test memory_live -- --ignored --nocapture

use std::time::Duration;

use errand_core::local::{LlmSettings, Local};
use errand_core::{team, Engine, Event};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a local model; run with --ignored"]
async fn a_model_writes_down_what_it_is_told_and_looks_it_up_again() {
    let home = std::env::temp_dir().join("errand-memory-live");
    std::fs::create_dir_all(&home).unwrap();
    let model = std::env::var("ERRAND_MODEL").unwrap_or_else(|_| "qwen2.5:7b-instruct".into());

    // Stands in for the app: answers the tools and records what was asked.
    let (wants, mut asked) = tokio::sync::mpsc::unbounded_channel::<team::Wants>();
    let heard = tokio::spawn(async move {
        let mut seen = Vec::new();
        while let Some(one) = asked.recv().await {
            let which = team::which_of_ours(&one.tool);
            seen.push((one.tool.clone(), one.args.clone()));
            let _ = one.answer.send(Ok(match which {
                Some(team::Ours::Remember) => "Written down.".to_string(),
                Some(team::Ours::Recall) => "- where the briefing goes: Telegram".to_string(),
                _ => "Nothing.".to_string(),
            }));
        }
        seen
    });

    let (mut it, events) = Local::open(
        LlmSettings {
            model,
            ..Default::default()
        },
        home,
        "auto",
        "",
        // No store behind this, so nothing was said before now.
        Vec::new(),
        Some(("live".to_string(), wants)),
    )
    .expect("opening");

    it.say("The morning briefing goes to Telegram, not email. Write that down so you still know it next week.", &[])
        .expect("saying");

    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    while std::time::Instant::now() < deadline {
        match events.recv_timeout(Duration::from_secs(5)) {
            Ok(event) => {
                if let Event::Doing(step) = &event {
                    println!("  · {}", step.what);
                }
                if event.ends_the_turn() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        }
    }
    it.stop().ok();
    drop(it);

    let seen = tokio::time::timeout(Duration::from_secs(10), heard)
        .await
        .expect("the stand-in finished")
        .expect("it did not panic");

    println!(
        "tools it reached for: {:?}",
        seen.iter().map(|(t, _)| t).collect::<Vec<_>>()
    );
    let wrote = seen.iter().find(|(t, _)| t == "remember");
    assert!(
        wrote.is_some(),
        "it was told something worth keeping and never wrote it down. \
         The description is the whole interface, so this failing means the wording \
         needs another pass, not that the plumbing is broken. It reached for: {:?}",
        seen.iter().map(|(t, _)| t).collect::<Vec<_>>()
    );
    let (_, args) = wrote.expect("just checked");
    println!("what it wrote: {args}");
    assert!(
        args.get("about")
            .and_then(|v| v.as_str())
            .is_some_and(|a| !a.is_empty()),
        "it wrote a note with nothing to file it under"
    );
}
