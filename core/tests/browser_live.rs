//! The browser connector against the Chrome on this machine.
//!
//! Ignored by default, because it needs Chrome running, macOS permission to
//! drive it, and one switch inside Chrome that only the person at the keyboard
//! can turn on: View, then Developer, then "Allow JavaScript from Apple
//! Events". Run them by hand:
//!
//!     cargo test -p errand-core --test browser_live -- --ignored --nocapture \
//!         --test-threads=1
//!
//! One thread, and it is not a nicety. Several of these say what the browser
//! looked like before and after, and two of them running at once means one is
//! counting the other's tab: the run that found that reported a tab left behind
//! by a call that had refused the address without opening anything at all.
//!
//! It exists because the thing that is hard here cannot be tested without a
//! browser. Everything about the shape of the answer can be checked in the unit
//! tests and none of it says whether a page that draws itself in JavaScript was
//! read before it had drawn, whether the tab was really closed afterwards, or
//! whether somebody was put back on the tab they were reading. Those are what
//! these look at.

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

/// The one browser, held by whichever test is using it.
///
/// Asking for one thread in the comment above is a request; this is the
/// guarantee. A poisoned lock is still the lock: a test that panicked mid-read
/// must not take every later test down with it, so the guard is recovered
/// rather than unwrapped.
fn the_browser() -> MutexGuard<'static, ()> {
    static ONE_CHROME: Mutex<()> = Mutex::new(());
    ONE_CHROME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

use serde_json::json;

/// Every tab Chrome has open, so a test can say whether it left one behind.
fn tabs_now() -> Vec<String> {
    let said = std::process::Command::new("osascript")
        .arg("-e")
        .arg(
            r#"set out to ""
tell application "Google Chrome"
  repeat with w in windows
    repeat with t in tabs of w
      set out to out & (id of t) & linefeed
    end repeat
  end repeat
end tell
return out"#,
        )
        .output()
        .expect("osascript ran");
    String::from_utf8_lossy(&said.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Which tab is on screen right now, which is what a person is looking at.
fn the_tab_in_front() -> String {
    let said = std::process::Command::new("osascript")
        .arg("-e")
        .arg(
            r#"tell application "Google Chrome"
  return (id of active tab of window 1) as string
end tell"#,
        )
        .output()
        .expect("osascript ran");
    String::from_utf8_lossy(&said.stdout).trim().to_string()
}

#[test]
#[ignore = "needs Chrome running, and JavaScript from Apple Events switched on in it"]
fn the_page_somebody_is_looking_at_is_not_taken_from_them_while_this_reads() {
    let _chrome = the_browser();
    // Not only afterwards. Chrome brings a new tab to the front, and putting
    // them back only at the close meant their view was Errand's for the whole
    // read: up to twenty-five seconds, happening unattended while they are
    // working, and their focus goes with it. Opening a tab of our own satisfies
    // the letter of not disturbing somebody; holding their screen does not.
    let before = the_tab_in_front();
    let watching = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..20 {
            seen.push(the_tab_in_front());
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        seen
    });
    let said = errand_core::connectors::run(
        "read_web_page",
        &json!({ "url": "https://www.google.com/maps/search/emergency+plumber+Denver" }),
    )
    .expect("Chrome answered");
    assert!(
        said.contains("Plumb"),
        "it did not read the page: {said:.200}"
    );

    let seen = watching.join().expect("the watcher finished");
    let moved: Vec<&String> = seen.iter().filter(|t| **t != before).collect();
    println!(
        "--- {} looks, {} of them elsewhere ---",
        seen.len(),
        moved.len()
    );
    assert!(
        moved.is_empty(),
        "their screen was taken from them {} times out of {}: {moved:?}",
        moved.len(),
        seen.len()
    );
    assert_eq!(the_tab_in_front(), before, "it left them somewhere else");
}

#[test]
#[ignore = "needs Chrome running, and JavaScript from Apple Events switched on in it"]
fn a_page_that_draws_itself_in_javascript_is_read_after_it_has_drawn() {
    let _chrome = the_browser();
    // The measurement this connector was built on. Plain fetching this address
    // hits a consent redirect, following that hits a bot wall, and headless
    // Chrome hits a reCAPTCHA. A real browser reads it straight off.
    let began = Instant::now();
    let said = errand_core::connectors::run(
        "read_web_page",
        &json!({ "url": "https://www.google.com/maps/search/emergency+plumber+Denver" }),
    )
    .expect("Chrome answered");
    let took = began.elapsed();
    println!("--- took {took:?} ---\n{said}\n---");

    // Read too early this comes back as the furniture around an empty list.
    // There is no way to assert on a particular plumber, so this asserts on
    // there being a page's worth of anything at all.
    assert!(
        said.chars().count() > 400,
        "that is not a rendered page: {said}"
    );
    assert!(took.as_secs() < 60, "it took {took:?}");
}

#[test]
#[ignore = "needs Chrome running, and JavaScript from Apple Events switched on in it"]
fn reading_a_page_leaves_the_browser_exactly_as_it_found_it() {
    let _chrome = the_browser();
    // The rule that makes this safe to point at somebody's own browser. A tool
    // that reads a page and leaves a tab behind is one nobody will leave on.
    let before = tabs_now();
    let said =
        errand_core::connectors::run("read_web_page", &json!({ "url": "https://example.com" }))
            .expect("Chrome answered");
    println!("--- {said}\n---");
    let after = tabs_now();
    assert_eq!(before, after, "the browser was not put back as it was");
    assert!(said.contains("Example Domain"), "{said}");
}

#[test]
#[ignore = "needs Chrome running, and JavaScript from Apple Events switched on in it"]
fn a_page_too_long_to_hand_over_whole_says_how_to_ask_for_the_rest() {
    let _chrome = the_browser();
    // The failure this inherits from fetch_url: an agent read the first part of
    // a long page, reported exact figures off it, and had no way to ask for the
    // rest. A page read through a browser is exactly as long as any other, so
    // it is cut the same way and says the same thing about it.
    let said = errand_core::connectors::run(
        "read_web_page",
        &json!({ "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)" }),
    )
    .expect("Chrome answered");
    let tail: String = said
        .chars()
        .skip(said.chars().count().saturating_sub(260))
        .collect();
    println!("--- tail ---\n{tail}\n---");
    assert!(said.contains("cut here"), "{tail}");
    assert!(
        said.contains("call read_web_page again"),
        "it named the wrong tool: {tail}"
    );

    // And the offset it gives is one that works, without visiting the page a
    // second time. Every part used to be a fresh navigation: nine of them for a
    // 200,000-character page, each repeating whatever that address does at the
    // far end, and each measuring its offset against a rendering that may have
    // changed underneath it.
    let began = Instant::now();
    let more = errand_core::connectors::run(
        "read_web_page",
        &json!({ "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)", "from": 24000 }),
    )
    .expect("Chrome answered");
    let took = began.elapsed();
    assert!(more.contains("characters 24000 onwards"), "{more:.200}");
    assert!(
        took.as_millis() < 500,
        "that was a second visit, not the rest of the first: {took:?}"
    );
}

#[test]
#[ignore = "needs Chrome running, and JavaScript from Apple Events switched on in it"]
fn a_page_that_finishes_with_nothing_in_it_says_so_without_waiting_out_the_clock() {
    let _chrome = the_browser();
    // A PDF, an iframe-only page and a canvas app all report `complete` with an
    // empty body for ever. This used to cost the full twenty-five seconds and
    // then say both "no text on this page at all" and "still changing, asking
    // again may get more of it", which cannot both be true and which sends an
    // agent back for another twenty-five seconds of the identical answer.
    let began = Instant::now();
    let said = errand_core::connectors::run(
        "read_web_page",
        &json!({ "url": "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf" }),
    )
    .expect("Chrome answered");
    let took = began.elapsed();
    println!("--- took {took:?} ---\n{said}\n---");
    assert!(said.contains("finished loading with no text"), "{said}");
    assert!(!said.contains("still changing"), "{said}");
    assert!(took.as_secs() < 20, "it waited out the clock: {took:?}");
}

#[test]
#[ignore = "needs Chrome running; it should refuse before Chrome is even asked"]
fn nothing_only_this_mac_can_see_is_ever_opened_in_the_browser() {
    let _chrome = the_browser();
    // The browser holds sessions for whatever is listening on this machine and
    // on this network, and it runs their scripts, so pointing it at one reaches
    // an authenticated admin panel a plain fetch could not get past. The
    // refusal has to happen here rather than in Chrome, and no tab may appear.
    let before = tabs_now();
    for refused in [
        "http://127.0.0.1:11434/api/tags",
        "http://192.168.1.1/",
        "http://localhost:3000/",
        "https://printer.local/",
    ] {
        let said = errand_core::connectors::run("read_web_page", &json!({ "url": refused }));
        let why = said.expect_err("it was allowed through").to_string();
        println!("--- {refused}: {why}");
    }
    assert_eq!(before, tabs_now(), "it opened a tab before refusing");
}

#[test]
#[ignore = "needs Chrome running; run it with the JavaScript switch OFF to see the other answer"]
fn what_it_says_when_chrome_will_not_run_the_reading_script() {
    let _chrome = the_browser();
    // Both of the not-working answers are worth reading out loud, because they
    // are the whole of what somebody gets when this does not work. Turn the
    // switch off in Chrome and run this: it must name the menu item, and it
    // must never name a command.
    let said =
        errand_core::connectors::run("read_web_page", &json!({ "url": "https://example.com" }));
    println!("--- {said:?} ---");
    match said {
        Err(why) => {
            let why = why.to_string();
            assert!(
                why.contains("View"),
                "it does not say where the switch is: {why}"
            );
            assert!(why.contains("Developer"), "{why}");
            assert!(
                !why.contains("defaults write"),
                "it told somebody to run a command at their own browser: {why}"
            );
        }
        // The switch is on, so this run says nothing about being refused. It
        // still has something to check: that a page came back, rather than a
        // refusal being handed over as though it were one.
        Ok(read) => assert!(read.contains("Example Domain"), "{read}"),
    }
}

#[test]
#[ignore = "needs Chrome running, but not the JavaScript switch"]
fn every_script_it_sends_is_something_applescript_can_actually_read() {
    let _chrome = the_browser();
    // A script that does not compile fails identically to a page that would not
    // load: one error line out of osascript, and nothing saying which. These
    // are the three that take a tab, run against a tab id that cannot exist, so
    // they walk every window and find nothing rather than touching anything.
    // The JavaScript inside is never reached, which is what lets this run
    // whether or not the switch in Chrome is on.
    for (what, script) in
        errand_core::connectors::the_scripts_for_a_page("https://example.com", "0")
    {
        if what.contains("opening") {
            continue; // It would open a tab, which is the live test's job.
        }
        if what.contains("unreadable") {
            // The one script here that closes by address rather than by id, so
            // it is the one that could shut a real tab: somebody with
            // example.com as the last tab of a window would lose it to a test
            // that only meant to ask whether the script compiles.
            continue;
        }
        let ran = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .expect("osascript ran");
        let said = String::from_utf8_lossy(&ran.stdout);
        let why = String::from_utf8_lossy(&ran.stderr);
        println!("--- {what} ---\n{script}\n--> {said}{why}");
        assert!(
            ran.status.success(),
            "{what} would not compile or run: {why}"
        );
    }
}

#[test]
#[ignore = "writes the scripts out so they can be read and run by hand"]
fn the_scripts_it_sends_can_be_read() {
    let _chrome = the_browser();
    // For when a live run goes odd and the question is whether the script or
    // the machine is at fault. Run it, then paste a file into Script Editor.
    for (what, script) in
        errand_core::connectors::the_scripts_for_a_page("https://example.com", "12345")
    {
        let where_it_went = format!("/tmp/errand-chrome-{}.applescript", what.replace(' ', "-"));
        std::fs::write(&where_it_went, &script).expect("written");
        println!("--- {what} ({where_it_went}) ---\n{script}\n");
    }
}
