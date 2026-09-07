//! What would be saved as a skill from real conversations, and what the next
//! run would read. Reads a copy of a real store and prints; asserts only that
//! the lines an engine actually wrote read back as steps.
//!
//!     ERRAND_DB=/path/to/copy cargo test -p errand-core --test skill_real -- --ignored --nocapture

use errand_core::skill;

#[test]
#[ignore = "needs a copy of a real store; set ERRAND_DB"]
fn the_last_errand_in_real_conversations_reads_back_as_steps_and_a_plan() {
    let at = std::env::var("ERRAND_DB").expect("ERRAND_DB");
    let store = errand_core::Store::open(std::path::Path::new(&at)).expect("opening it");

    let mut shown = 0;
    for agent in store.agents().expect("reading agents") {
        for talk in store
            .conversations(&agent.id)
            .expect("reading conversations")
        {
            let lines = store.lines(&talk.id).expect("reading lines");
            let Some(taught) = skill::from_lines(&lines) else {
                continue;
            };
            let kept = errand_core::store::Skill {
                name: format!("from {}", talk.name),
                request: taught.request.clone(),
                steps: taught.steps.clone(),
                made_at: 0,
            };
            println!(
                "== {} / {} ({} lines, {} steps)\n{}\n",
                agent.name,
                talk.name,
                lines.len(),
                taught.steps.len(),
                skill::the_plan(&kept, "")
            );
            // Every step came from a line with a tool on it, and none of them
            // is the bookkeeping around the task.
            assert!(taught
                .steps
                .iter()
                .all(|s| !s.tool.is_empty() && !s.what.is_empty()));
            shown += 1;
            if shown >= 3 {
                return;
            }
        }
    }
    assert!(shown > 0, "no conversation in this store has a step in it");
}
