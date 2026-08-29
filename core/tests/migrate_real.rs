//! Does the real store come forward cleanly?
//!
//!     ERRAND_DB=/path/to/copy cargo test -p errand-core --test migrate_real -- --ignored --nocapture

#[test]
#[ignore = "needs a copy of a real store; set ERRAND_DB"]
fn a_copy_of_the_real_store_comes_up_to_date() {
    let at = std::env::var("ERRAND_DB").expect("ERRAND_DB");
    let store = errand_core::Store::open(std::path::Path::new(&at)).expect("opening it");
    let agents = store.agents().expect("reading it");
    println!("{} agents", agents.len());
    for a in &agents {
        println!("  {} asks={}", a.name, a.asks);
    }
    assert!(agents.iter().all(|a| !a.asks.is_empty()));
}
