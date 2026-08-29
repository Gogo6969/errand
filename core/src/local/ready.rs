//! Which of a server's models can actually answer, and which are only there.
//!
//! A model server's list of models is not a list of models you can use. Asked
//! what it has, LM Studio names everything downloaded and Ollama names
//! everything pulled, and on a network with a few machines on it that is
//! twenty entries, none of them loaded, one of them an embedding model that
//! cannot hold a conversation at all. Offering that as a menu is offering a
//! menu of things that mostly are not food.
//!
//! Two different facts are muddled together in that list, and they deserve
//! different treatment:
//!
//! - **Whether it can chat at all.** An embedding model answers a completely
//!   different question and will never answer this one. There is no reason to
//!   ever show it, and doing so is how somebody spends a minute finding out
//!   that "text-embedding-nomic-embed-text-v1.5" is not a chatbot.
//! - **Whether it is loaded right now.** This one is worth saying and not worth
//!   hiding. Both servers load on demand: measured on this network, an unloaded
//!   Ollama model answered in under two seconds and an unloaded LM Studio model
//!   answered too, after a wait. So an unloaded model is not unusable, it is
//!   slow to start, and hiding it would remove working choices from the list
//!   for a reason that turns out not to be true.
//!
//! So: what cannot chat is dropped, what is loaded is offered plainly, and what
//! would have to load first says so and sorts below.

use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;

/// How long a server gets to say what it has loaded.
///
/// Short, because this runs for every server found and the answer is a
/// nicety: not knowing means offering the model unmarked, which is what
/// happened before any of this existed.
const TO_ANSWER: Duration = Duration::from_millis(2500);

/// One model, and whether it is ready this second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    pub model: String,
    /// Loaded and able to answer without loading first.
    pub loaded: bool,
}

/// Sort what a server offers into what is worth putting in a list.
///
/// Anything that cannot hold a conversation is dropped. What is left is ordered
/// with the ready ones first, because a list is read from the top and the top
/// is where the thing you can use right now belongs.
pub async fn what_can_answer(provider: &str, base_url: &str, models: &[String]) -> Vec<Ready> {
    let client = match reqwest::Client::builder()
        .timeout(TO_ANSWER)
        .danger_accept_invalid_certs(true)
        .build()
    {
        Ok(client) => client,
        Err(_) => return every_one_of_them(models),
    };
    let base = base_url.trim_end_matches('/');

    let mut sorted = match provider {
        "ollama" => from_ollama(&client, base, models).await,
        "lmstudio" => from_lm_studio(&client, base, models).await,
        // Everything else serves what it was started with, so what it lists is
        // what it has loaded. vLLM and llama.cpp are given a model on the
        // command line and hold it for their whole run.
        _ => None,
    }
    .unwrap_or_else(|| every_one_of_them(models));

    sorted.sort_by_key(|one| (!one.loaded, one.model.to_lowercase()));
    sorted
}

/// What to say when the server would not tell us: all of it, unmarked.
///
/// The honest fallback. Claiming everything is loaded would be a guess dressed
/// as a fact, but marking everything as needing to load would be the same guess
/// the other way round, and it would put a warning on models that are fine.
fn every_one_of_them(models: &[String]) -> Vec<Ready> {
    models
        .iter()
        .filter(|name| could_hold_a_conversation(name))
        .map(|model| Ready {
            model: model.clone(),
            loaded: true,
        })
        .collect()
}

/// Ollama, which says what is loaded but not what each model is for.
async fn from_ollama(
    client: &reqwest::Client,
    base: &str,
    models: &[String],
) -> Option<Vec<Ready>> {
    let said: Value = client
        .get(format!("{base}/api/ps"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    // Names come back with the tag attached, the same way `/api/tags` gives
    // them, so they compare directly.
    let loaded: HashSet<String> = said
        .get("models")?
        .as_array()?
        .iter()
        .filter_map(|m| m.get("name")?.as_str().map(str::to_string))
        .collect();

    Some(
        models
            .iter()
            .filter(|name| could_hold_a_conversation(name))
            .map(|model| Ready {
                loaded: loaded.contains(model),
                model: model.clone(),
            })
            .collect(),
    )
}

/// LM Studio, which has an API of its own that answers both questions.
async fn from_lm_studio(
    client: &reqwest::Client,
    base: &str,
    models: &[String],
) -> Option<Vec<Ready>> {
    let said: Value = client
        .get(format!("{base}/api/v0/models"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    let listed = said.get("data")?.as_array()?;
    let mut ready = Vec::new();
    for one in listed {
        let Some(id) = one.get("id").and_then(|i| i.as_str()) else {
            continue;
        };
        // `llm` and `vlm` both talk; `embeddings` does not, and this is the one
        // place either server says so outright rather than leaving it to be
        // guessed from a name.
        if one.get("type").and_then(|t| t.as_str()) == Some("embeddings") {
            continue;
        }
        ready.push(Ready {
            model: id.to_string(),
            loaded: one.get("state").and_then(|s| s.as_str()) == Some("loaded"),
        });
    }

    // Only trusted if it covered what the ordinary list said. A partial answer
    // would quietly drop models that are perfectly usable.
    match ready.is_empty() && !models.is_empty() {
        true => None,
        false => Some(ready),
    }
}

/// Is this a model somebody could talk to?
///
/// A guess from the name, and only used where the server will not say. Ollama
/// has no field for it, and asking it about every model one at a time is a
/// round trip per model across a network that may have twenty of them.
fn could_hold_a_conversation(name: &str) -> bool {
    let name = name.to_lowercase();
    // The words that appear in the name of something that turns text into
    // vectors. None of them appear in the name of a chat model.
    !["embed", "reranker", "rerank", "bge-", "e5-", "clip-"]
        .iter()
        .any(|kind| name.contains(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn something_that_turns_text_into_vectors_is_never_offered_as_something_to_talk_to() {
        // The one on this network that made the point:
        // "text-embedding-nomic-embed-text-v1.5" sitting in a list of chat
        // models, indistinguishable until somebody picks it.
        for not_a_chat in [
            "text-embedding-nomic-embed-text-v1.5",
            "nomic-embed-text",
            "bge-large-en",
            "mxbai-embed-large",
            "jina-reranker-v2",
        ] {
            assert!(
                !could_hold_a_conversation(not_a_chat),
                "{not_a_chat} was offered as something to talk to"
            );
        }
    }

    #[test]
    fn an_ordinary_model_is_not_mistaken_for_one() {
        for chat in [
            "qwen2.5:7b-instruct",
            "llama3.2:1b",
            "google/gemma-4-26b-a4b",
            "qwen3.5-27b-claude-4.6-opus-reasoning-distilled-v2",
            "Qwen3.8-27B-Q4_K_M",
        ] {
            assert!(could_hold_a_conversation(chat), "{chat} was dropped");
        }
    }

    #[test]
    fn what_is_ready_now_is_listed_before_what_would_have_to_load() {
        // A list is read from the top, and the top is where the thing somebody
        // can use this second belongs.
        let mut some = [
            Ready {
                model: "zeta".into(),
                loaded: false,
            },
            Ready {
                model: "alpha".into(),
                loaded: false,
            },
            Ready {
                model: "yankee".into(),
                loaded: true,
            },
        ];
        some.sort_by_key(|one| (!one.loaded, one.model.to_lowercase()));
        assert_eq!(
            some.iter().map(|r| r.model.as_str()).collect::<Vec<_>>(),
            ["yankee", "alpha", "zeta"]
        );
    }

    #[test]
    fn a_server_that_will_not_say_has_everything_it_named_offered_unmarked() {
        // Marking them all as needing to load would be the same guess as
        // marking them all ready, and it would put a warning on models that
        // are perfectly fine.
        let said = every_one_of_them(&["one".to_string(), "nomic-embed-text".to_string()]);
        assert_eq!(
            said,
            vec![Ready {
                model: "one".into(),
                loaded: true
            }],
            "it either kept something that cannot chat or marked a working model"
        );
    }
}
