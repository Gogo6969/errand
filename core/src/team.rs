//! One agent asking another.
//!
//! The thing in the walkthrough that looks like magic: a delegator hands the
//! research to one agent, which hands the writing to another, and you can open
//! the exchange between two of them afterwards and read a conversation you were
//! never part of.
//!
//! The mechanism is smaller than it looks. There is one tool, `ask`, and the
//! app answers it by opening a conversation with the named agent, saying the
//! request into it, and handing back what came out. The exchange is stored the
//! way every other conversation is, so reading it later is not a feature, it is
//! the absence of one.
//!
//! What matters here is that this is the *app's* tool and not an engine's.
//! Neither engine can route a message to another agent -- only the thing
//! holding all of them can -- so both reach the same function, one in-process
//! and one over a socket, and there is exactly one implementation of what
//! asking somebody else actually means.

use serde_json::{json, Value};
use tokio::sync::oneshot;

/// What an engine wants the app to do on its behalf.
///
/// Sent up rather than handled where it arrives, because an engine knows about
/// its own conversation and nothing else. The answer goes back down the
/// oneshot, so the engine's loop waits exactly as it would for any other tool.
#[derive(Debug)]
pub struct Wants {
    /// Which of the app's tools, by the name the model called.
    pub tool: String,
    pub args: Value,
    /// The conversation asking, so the app can refuse an agent asking itself.
    pub from: String,
    pub answer: oneshot::Sender<anyhow::Result<String>>,
}

/// The tools the app provides, as an engine has to declare them.
///
/// Two, and the second exists because of the first: an agent that can hand work
/// to somebody has to be able to find out who there is.
pub fn declarations() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "ask",
                "description":
                    "Hand part of this job to another agent and wait for its answer. Use this \
                     when the work belongs to somebody else's speciality rather than yours. \
                     The other agent has its own memory and tools and does not see this \
                     conversation, so say everything it needs in the request.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "agent": { "type": "string", "description": "Which agent, by name" },
                        "request": {
                            "type": "string",
                            "description": "What you need from them, in full, as you would say it to a colleague"
                        }
                    },
                    "required": ["agent", "request"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "who_else",
                "description":
                    "List the other agents you can hand work to, with what each one handles. \
                     Call this before deciding a job cannot be done here.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
    ]
}

/// Is this one of the app's own tools?
pub fn ours(tool: &str) -> bool {
    matches!(tool, "ask" | "who_else")
}

/// What a step is doing, in words.
pub fn in_plain_words(tool: &str, args: &Value) -> String {
    let get = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("");
    match tool {
        "ask" => format!("Asking {}", get("agent")),
        "who_else" => "Looking for somebody to hand this to".to_string(),
        other => format!("Using {other}"),
    }
}

/// The thing itself, for a question that has to be judged.
pub fn the_thing_itself(tool: &str, args: &Value) -> String {
    match tool {
        "ask" => format!(
            "{}: {}",
            args.get("agent").and_then(|v| v.as_str()).unwrap_or("?"),
            args.get("request").and_then(|v| v.as_str()).unwrap_or("")
        ),
        _ => String::new(),
    }
}

/// Does using this need somebody's say-so first?
///
/// Handing work to another agent does, because it spends somebody's time and
/// money and the other agent may do anything its own permissions allow. Looking
/// at the list of who exists does not.
pub fn asks_first(tool: &str) -> bool {
    tool == "ask"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handing_work_to_somebody_asks_first_and_looking_at_the_list_does_not() {
        // Asking somebody spends their time and runs under their permissions,
        // which is a bigger thing than the sentence makes it look.
        assert!(asks_first("ask"));
        assert!(!asks_first("who_else"));
    }

    #[test]
    fn a_question_about_delegation_shows_who_and_what_rather_than_just_the_tool() {
        let args = json!({ "agent": "Scribe", "request": "Draft a reply to Sarah." });
        assert_eq!(in_plain_words("ask", &args), "Asking Scribe");
        assert_eq!(
            the_thing_itself("ask", &args),
            "Scribe: Draft a reply to Sarah.",
            "a card that hides who is being asked is not a question anybody can answer"
        );
    }

    #[test]
    fn both_tools_are_declared_the_way_an_engine_expects_them() {
        let declared = declarations();
        let named: Vec<&str> = declared
            .iter()
            .filter_map(|d| d.pointer("/function/name")?.as_str())
            .collect();
        assert_eq!(named, ["ask", "who_else"]);
        assert!(declared.iter().all(|d| d["type"] == "function"));
        assert!(ours("ask") && ours("who_else") && !ours("run_command"));
    }
}
