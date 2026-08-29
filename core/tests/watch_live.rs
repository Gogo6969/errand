//! Does a watch tell a change from a difference, against real things?
//!
//! Everything about the rule is unit-tested. This is the part that is not: that
//! a real page fetched twice, unchanged, produces the same mark. If it does
//! not, the watch wakes somebody on every single look for ever, and every
//! waking is paid for. It is the one failure that cannot be caught by
//! reasoning, because it depends on what a particular server does.
//!
//!     cargo test -p errand-core --test watch_live -- --ignored --nocapture

use errand_core::watch::{compare, look, Look, Next, Watch};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "reaches the network; run with --ignored"]
async fn a_page_fetched_twice_without_changing_looks_the_same_both_times() {
    // Pages chosen because they are the awkward kind: busy, cached, and known
    // to put a fresh token in every response.
    for url in [
        "https://github.com",
        "https://news.ycombinator.com",
        "https://example.com",
    ] {
        let at = Look::Away(url.to_string());
        let Ok(once) = look(&at).await else {
            println!("{url}: could not be reached, skipped");
            continue;
        };
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let twice = look(&at).await.expect("the second look");

        println!("{url}\n  1: {}\n  2: {}", once.mark, twice.mark);
        assert_eq!(
            compare(Some(&once.mark), None, &twice.mark),
            Next::Same,
            "{url} looks different every time, so a watch on it would wake \
             somebody on every single look"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "touches the disk; run with --ignored"]
async fn a_folder_says_it_changed_when_a_file_arrives_and_not_before() {
    let here = std::env::temp_dir().join(format!("errand-watch-{}", std::process::id()));
    std::fs::create_dir_all(&here).unwrap();
    let watch = Watch::read(&format!("{} every 10m", here.display())).expect("a watch");

    let first = look(&watch.look).await.expect("looking");
    assert_eq!(compare(None, None, &first.mark), Next::FirstSight);

    // Nothing has happened, so nothing has changed.
    let again = look(&watch.look).await.expect("looking");
    assert_eq!(compare(Some(&first.mark), None, &again.mark), Next::Same);

    // A part-downloaded file is not a file that arrived.
    std::fs::write(here.join("invoice.pdf.crdownload"), "half").unwrap();
    let during = look(&watch.look).await.expect("looking");
    assert_eq!(
        compare(Some(&first.mark), None, &during.mark),
        Next::Same,
        "a part-downloaded file woke somebody"
    );

    // And now it really arrives.
    std::fs::remove_file(here.join("invoice.pdf.crdownload")).unwrap();
    std::fs::write(here.join("invoice.pdf"), "whole").unwrap();
    let arrived = look(&watch.look).await.expect("looking");
    assert_eq!(
        compare(Some(&first.mark), None, &arrived.mark),
        Next::Settling,
        "the first sight of a difference is not yet a change"
    );
    let steady = look(&watch.look).await.expect("looking");
    assert_eq!(
        compare(Some(&first.mark), Some(&arrived.mark), &steady.mark),
        Next::Changed,
        "seen twice the same way, it is a change"
    );

    println!(
        "what it would say:\n{}",
        errand_core::watch::what_to_say(
            &watch,
            Some(&first.note),
            &steady.note,
            Some("an hour ago")
        )
    );
    let _ = std::fs::remove_dir_all(&here);
}
