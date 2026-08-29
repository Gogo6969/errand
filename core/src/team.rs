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

/// The name Claude Code has to reach these tools under.
///
/// It will only take a tool from an MCP server, and it names every tool after
/// the server it came from. So this is the server's name, and the first half of
/// what Claude Code calls the two tools below.
pub const DOORWAY: &str = "errand";

/// Which of the app's tools this is, whichever engine named it.
///
/// The same delegation arrives as `ask` from a local model and as
/// `mcp__errand__ask` from Claude Code, because one of them is inside our tool
/// loop and the other reaches us through a server. Both are the same tool, and
/// the plain name is the one written down: the allowlist is the app's and is
/// shared between engines, so a yes remembered while Claude Code was answering
/// has to still hold when a local model is, and it only does if there is one
/// name in the table rather than two.
///
/// Deliberately not folded into `ours`. A widened `ours` would also match a
/// genuinely third-party server that happened to be called `errand`, and
/// answer its tools with "there is nobody else here to ask" instead of calling
/// them. Converting at the two edges where a prefixed name can appear is both
/// smaller and safer than accepting it everywhere.
pub fn which_of_ours(tool: &str) -> Option<&'static str> {
    let plain = tool
        .strip_prefix(&format!("mcp__{DOORWAY}__"))
        .unwrap_or(tool);
    match plain {
        "ask" => Some("ask"),
        "who_else" => Some("who_else"),
        _ => None,
    }
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
    fn an_ask_is_the_same_tool_whether_or_not_claude_code_prefixed_it() {
        // The whole of why an "always" given under one engine still holds
        // under the other: both arrive at the allowlist as one name.
        assert_eq!(which_of_ours("ask"), Some("ask"));
        assert_eq!(which_of_ours("mcp__errand__ask"), Some("ask"));
        assert_eq!(which_of_ours("who_else"), Some("who_else"));
        assert_eq!(which_of_ours("mcp__errand__who_else"), Some("who_else"));
    }

    #[test]
    fn a_tool_from_somebody_elses_server_is_not_ours() {
        // Including one from a server that happens to share our name. Ours are
        // two, they are named, and anything else belongs to whoever offered it.
        assert_eq!(which_of_ours("mcp__errand__something_else"), None);
        assert_eq!(which_of_ours("mcp__peekaboo__ask"), None);
        assert_eq!(which_of_ours("run_command"), None);
        assert_eq!(which_of_ours(""), None);
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
