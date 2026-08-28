//! Is there anything on this machine to talk to?
//!
//! The picker in the window is only as good as this: it offers what is actually
//! running, found by asking, rather than a list somebody has to keep up to
//! date. Offering a model that stopped yesterday means choosing it and finding
//! out later, somewhere much less obvious than a dropdown.
//!
//! Ignored by default because it needs a model server running and there is no
//! honest way to assert on what somebody else's machine happens to have.
//!
//!     cargo test -p errand-core --test engines -- --ignored --nocapture

use errand_core::local::{find, LlmSettings};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a local model server; run with --ignored"]
async fn whatever_is_running_on_this_machine_is_found_by_asking_rather_than_by_a_list() {
    let found = find::detect_all().await;
    for backend in &found {
        println!(
            "{} at {} ({}): {}",
            backend.label,
            backend.base_url,
            backend.provider,
            match backend.models.is_empty() {
                true => "no models loaded".to_string(),
                false => backend.models.join(", "),
            }
        );
    }
    assert!(
        !found.is_empty(),
        "nothing answered on any of the usual ports; is Ollama or LM Studio running?"
    );

    // A backend that answers but has nothing loaded is a backend the picker
    // must not offer, because choosing it looks like it worked and then fails
    // on the first turn.
    let usable: Vec<_> = found.iter().filter(|b| !b.models.is_empty()).collect();
    assert!(!usable.is_empty(), "found servers, but none with a model");

    // And what it named must be reachable under that name, or the picker is
    // offering something that cannot be selected.
    let first = usable[0];
    let settings = LlmSettings {
        provider: first.provider.clone(),
        base_url: first.base_url.clone(),
        model: first.models[0].clone(),
        ..Default::default()
    };
    assert!(
        find::probe_alive(&settings).await,
        "{} was offered but does not answer",
        settings.model
    );
}
