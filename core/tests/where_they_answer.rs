//! Where the hosted providers actually answer.
//!
//! Ignored by default, because it goes out to four companies' servers and a
//! test suite that needs the internet is one that fails on a train. Run it when
//! the addresses in the window are in doubt:
//!
//!     cargo test -p errand-core --test where_they_answer -- --ignored --nocapture
//!
//! It needs no keys. The trick is that a wrong path and a right one can be told
//! apart without one, wherever a host routes before it checks the key: ask for
//! the path and for a deliberately nonsense one beside it, and where the two
//! answers differ, the path is proved. Where they are the same, the host checks
//! the key first and nothing can be concluded -- which is itself worth knowing,
//! and is why one of the four is marked unproven in the window rather than
//! quietly guessed at.

use std::time::Duration;

/// What Errand fills in, and what it must not be.
struct Address {
    who: &'static str,
    /// The prefix stored, exactly as the window fills it in.
    base: &'static str,
    /// A prefix that is documented or widely assumed and is wrong.
    wrong: &'static str,
    /// False where the host checks the key before it routes, so nothing can be
    /// proved from outside.
    provable: bool,
}

const THEM: &[Address] = &[
    Address {
        who: "Kimi",
        base: "https://api.moonshot.ai/v1",
        wrong: "https://api.moonshot.ai",
        provable: true,
    },
    Address {
        who: "GLM",
        base: "https://api.z.ai/api/paas/v4",
        wrong: "https://api.z.ai/v1",
        provable: true,
    },
    Address {
        who: "OpenRouter",
        base: "https://openrouter.ai/api/v1",
        wrong: "https://openrouter.ai/v1",
        provable: true,
    },
    Address {
        who: "DeepSeek",
        base: "https://api.deepseek.com",
        wrong: "https://api.deepseek.com/nonsense-xyz",
        provable: false,
    },
];

async fn asking(client: &reqwest::Client, url: &str) -> Option<u16> {
    client
        .post(url)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .ok()
        .map(|r| r.status().as_u16())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "goes out to four companies\' servers"]
async fn every_address_the_window_fills_in_is_where_that_provider_answers() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("a client");

    for one in THEM {
        let right = asking(&client, &format!("{}/chat/completions", one.base)).await;
        let wrong = asking(&client, &format!("{}/chat/completions", one.wrong)).await;
        println!(
            "{:<11} {:?} at {}  |  {:?} at {}",
            one.who, right, one.base, wrong, one.wrong
        );

        let Some(right) = right else {
            panic!("{} could not be reached at all", one.who);
        };

        match one.provable {
            // A route that is there answers about the key. A route that is not
            // answers about the route.
            true => {
                assert_eq!(
                    right, 401,
                    "{} no longer wants a key at {}, so the address has moved",
                    one.who, one.base
                );
                assert_eq!(
                    wrong,
                    Some(404),
                    "{} now answers at {} too, so this test can no longer tell them apart",
                    one.who,
                    one.wrong
                );
            }
            // Nothing to prove, and the point is that it stays that way: if
            // this host ever starts routing first, the window can stop hedging.
            false => {
                assert_eq!(
                    right,
                    wrong.unwrap_or(0),
                    "{} has started routing before it checks the key, so its address \
                     can be proved now and the window should say so",
                    one.who
                );
            }
        }
    }
}
