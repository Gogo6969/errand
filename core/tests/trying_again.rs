//! A provider that says "not now", against a server that says it.
//!
//! The decision to try again is made from one string, because that is the only
//! thing every transport here has in common. What that costs is a join nothing
//! else checks: the `retry-after` header has to survive being turned into
//! words, and the words have to be read back into the same number. Both halves
//! have tests of their own and neither proves the join, which is exactly the
//! kind of gap that ships.
//!
//! So there is a server. About forty lines of raw TCP, because this crate has
//! no HTTP server in it and adding one to run a test is a poor trade. No key,
//! no network, no account.

use errand_core::local::talk::LlmClient;
use errand_core::local::{ChatMessage, LlmSettings};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A server that turns everything away with the status and headers given.
async fn a_server_that_says_no(status: &'static str, headers: &'static str) -> String {
    let listening = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a port");
    let at = format!("http://{}", listening.local_addr().unwrap());
    tokio::spawn(async move {
        while let Ok((mut them, _)) = listening.accept().await {
            let mut got = vec![0u8; 8 * 1024];
            let _ = them.read(&mut got).await;
            let body = "{\"error\":{\"message\":\"slow down\"}}";
            let said = format!(
                "HTTP/1.1 {status}\r\n{headers}Content-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = them.write_all(said.as_bytes()).await;
            let _ = them.flush().await;
        }
    });
    at
}

fn talking_to(at: &str) -> LlmClient {
    LlmClient::new(LlmSettings {
        provider: "openai-compat".into(),
        base_url: format!("{at}/v1"),
        model: "whatever".into(),
        ..Default::default()
    })
}

async fn what_it_said(at: &str) -> String {
    talking_to(at)
        .stream(
            &[ChatMessage::User {
                content: "hello".into(),
                name: None,
                image_data_urls: vec![],
            }],
            &[],
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .err()
        .expect("it refused")
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn what_the_server_asked_to_be_waited_reaches_the_thing_that_decides() {
    // The join nothing else covers: a header, turned into words by one file,
    // read back into a number by another. A rate limit at seven in the morning
    // is the whole reason any of this exists.
    let at = a_server_that_says_no("429 Too Many Requests", "retry-after: 7\r\n").await;
    let why = what_it_said(&at).await;
    assert!(why.contains("retry after 7s"), "{why}");

    let waiting = errand_core::local::again::worth_another_go(&why).expect("worth another go");
    assert_eq!(waiting, std::time::Duration::from_secs(7), "{why}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rate_limit_with_no_advice_still_gets_one_more_try() {
    // Plenty of providers send the status and nothing else. Guessing is worse
    // than being told and much better than giving up.
    let at = a_server_that_says_no("429 Too Many Requests", "").await;
    let why = what_it_said(&at).await;
    assert!(!why.contains("retry after"), "{why}");
    assert!(
        errand_core::local::again::worth_another_go(&why).is_some(),
        "{why}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_that_is_wrong_is_not_asked_about_twice() {
    // Trying this again spends somebody's morning arriving at the same answer
    // twice and delays the one line that would let them fix it.
    let at = a_server_that_says_no("401 Unauthorized", "").await;
    let why = what_it_said(&at).await;
    assert_eq!(
        errand_core::local::again::worth_another_go(&why),
        None,
        "{why}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_is_not_there_at_all_is_worth_one_more_try() {
    // The commonest of the lot on this app: a local model server that has not
    // been started, or was restarted a second ago.
    let nowhere = {
        let taken = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let at = format!("http://{}", taken.local_addr().unwrap());
        drop(taken);
        at
    };
    let why = what_it_said(&nowhere).await;
    assert!(
        errand_core::local::again::worth_another_go(&why).is_some(),
        "{why}"
    );
}
