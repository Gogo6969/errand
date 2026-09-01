//! The mail connector against the Mail on this machine.
//!
//! Ignored by default, because it needs Mail, an account in it, and permission
//! macOS only grants after somebody says yes in a dialog. Run it by hand:
//!
//!     cargo test -p errand-core --test mail_live -- --ignored --nocapture
//!
//! It exists because the first version of this connector passed every test it
//! had and then took eight minutes to answer a question, twice. Nothing about
//! that was visible without a real mailbox: the tests all measured the shape of
//! the answer, and what was wrong was how long it took to get one and what it
//! had quietly left out. So this one measures the clock and reads the sentence
//! at the bottom, which are the two things that were wrong.

use std::time::Instant;

use serde_json::json;

#[test]
#[ignore = "needs Mail on this machine, and permission to drive it"]
fn asking_what_is_unread_answers_in_seconds_and_says_what_it_left_out() {
    let began = Instant::now();
    let said = errand_core::connectors::run("unread_mail", &json!({ "at_most": 5 }))
        .expect("Mail answered");
    let took = began.elapsed();
    println!("--- took {took:?} ---\n{said}\n---");

    // The fault this replaces took eight minutes. The deadline inside the
    // script is 25 seconds and the backstop around it is 40, so anything past
    // that means neither of them held.
    assert!(took.as_secs() < 60, "it took {took:?}");

    // And whatever it found, it has to say how much of the whole it was. A list
    // with no sentence under it is the thing that was believed and wrong.
    let bottom = said.lines().last().unwrap_or_default();
    assert!(
        bottom.contains("unread") || bottom.contains("Read ") || bottom.contains("Nothing"),
        "nothing under the list says how much of it this is: {bottom}"
    );
}

#[test]
#[ignore = "needs Mail on this machine, and permission to drive it"]
fn asking_for_more_than_there_are_does_not_come_back_looking_like_a_cap() {
    // The pair of calls that caused the complaint: the same question twice with
    // a different limit, where both came back with exactly fifty and nothing
    // said which was the truth.
    let few = errand_core::connectors::run("unread_mail", &json!({ "at_most": 5 })).expect("Mail");
    let many =
        errand_core::connectors::run("unread_mail", &json!({ "at_most": 500 })).expect("Mail");
    println!("--- few ---\n{few}\n--- many ---\n{many}\n---");

    // Whatever the numbers are, the answer says what they mean. The old one
    // could not be told apart from a mailbox holding exactly the cap.
    for said in [&few, &many] {
        assert!(
            said.contains("Read ") || said.contains("Nothing unread"),
            "no count of the whole: {said}"
        );
    }
}

#[test]
#[ignore = "needs Mail on this machine, and permission to drive it"]
fn searching_the_post_comes_back_rather_than_running_until_somebody_gives_up() {
    let began = Instant::now();
    let said = errand_core::connectors::run("search_mail", &json!({ "about": "receipt" }))
        .expect("Mail answered");
    let took = began.elapsed();
    println!("--- took {took:?} ---\n{said}\n---");
    assert!(took.as_secs() < 60, "it took {took:?}");
    assert!(
        said.contains("match") || said.contains("Nothing"),
        "nothing says how the search went: {said}"
    );
}

#[test]
#[ignore = "needs Calendar on this machine, and permission to drive it"]
fn asking_what_is_on_answers_in_seconds() {
    let began = Instant::now();
    let said = errand_core::connectors::run("what_is_on", &json!({ "when": "today" }))
        .expect("Calendar answered");
    let took = began.elapsed();
    println!("--- took {took:?} ---\n{said}\n---");
    assert!(took.as_secs() < 60, "it took {took:?}");
}

#[test]
#[ignore = "writes the scripts out so they can be run by hand"]
fn the_scripts_it_sends_can_be_read() {
    // For when a live run hangs and the question is whether the script or the
    // machine is at fault. Run it, then run the files it writes through
    // osascript by hand.
    let where_it_is = errand_core::connectors::the_script_for_where_the_unread_is();
    std::fs::write("/tmp/errand-script-where.applescript", &where_it_is).expect("written");
    println!("--- where the unread is ---\n{where_it_is}");
}

#[test]
#[ignore = "writes the reading script out so it can be run by hand"]
fn the_reading_script_can_be_read() {
    let script = errand_core::connectors::the_script_for_reading("Vidfame", "INBOX", 5);
    std::fs::write("/tmp/errand-script-read.applescript", &script).expect("written");
    println!("{script}");
}
