//! What the doctor says about the machine it is run on.
//!
//! Ignored by default because it starts every MCP server somebody has
//! configured and probes the network, which is not a thing a test suite should
//! do on its own.
//!
//!     cargo test -p errand-core --test doctor_here -- --ignored --nocapture

use errand_core::{doctor, Store};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "touches this machine's real setup; run with --ignored"]
async fn what_is_wrong_with_this_setup() {
    let here = std::path::PathBuf::from(std::env::var("HOME").unwrap())
        .join("Library/Application Support/Errand");
    let store = Store::open(&errand_core::store::beside(&here)).expect("the real store");

    // Whether macOS shows this app's notifications is a thing only the app can
    // ask, from inside its own bundle; a test has no bundle to ask from.
    let may = doctor::Notifying::Unknown;
    for said in doctor::everything(&store, &here, &here, may).await {
        println!(
            "{:8} {:34} {}",
            format!("{:?}", said.how).to_uppercase(),
            said.what,
            said.said
        );
        if !said.fix.is_empty() {
            println!("         -> {}", said.fix);
        }
    }
}
