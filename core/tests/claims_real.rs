//! What the claims check would write under every answer in a real store.
//!
//! Reads a copy of a real store and prints each line the check would have
//! put in a conversation, with the answer it was about, so a false one can be
//! read before it is written under a routine's answer every five minutes.
//! That is how the two it has produced so far were found: a sentence ending
//! in a colon taken for a heading over a listing, and a quoted phrase with a
//! slash in it taken for a file. Asserts only that every answer was read.
//!
//!     ERRAND_DB=/path/to/copy cargo test -p errand-core --test claims_real -- --ignored --nocapture

use errand_core::claims;

#[test]
#[ignore = "needs a copy of a real store; set ERRAND_DB"]
fn what_the_claims_check_would_say_under_every_real_answer() {
    let at = std::env::var("ERRAND_DB").expect("ERRAND_DB");
    let store = errand_core::Store::open(std::path::Path::new(&at)).expect("opening it");
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);

    let mut answers = 0;
    let mut written = 0;
    for agent in store.agents().expect("reading agents") {
        let own = std::path::Path::new(&agent.cwd);
        if !own.is_absolute() {
            continue;
        }
        let also: Vec<std::path::PathBuf> = store
            .folders_allowed(&agent.id)
            .unwrap_or_default()
            .into_iter()
            .filter(|folder| folder.is_absolute())
            .collect();
        let may = claims::Writable {
            own,
            also: &also,
            home: home.as_deref(),
        };
        for talk in store
            .conversations(&agent.id)
            .expect("reading conversations")
        {
            // The turn an answer belongs to began at the last thing said to
            // the agent before it, which is what the app reads off the store
            // at the moment the answer is written down.
            let mut began_at = None;
            for line in store.lines(&talk.id).expect("reading lines") {
                match line.kind.as_str() {
                    "mine" => began_at = Some(line.at),
                    "said" => {
                        answers += 1;
                        let claimed = claims::claimed_written(&line.text, &may);
                        for wrong in claims::not_borne_out(&claimed, began_at) {
                            written += 1;
                            println!(
                                "== {} / {} seq {}\n{}\nLINE: {}\n",
                                agent.name,
                                talk.name,
                                line.seq,
                                line.text,
                                claims::what_to_say(&wrong)
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    println!("{answers} answers read, {written} lines the check would write");
    assert!(answers > 0, "no answer in this store");
}
