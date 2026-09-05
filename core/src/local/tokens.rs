// Lifted from KinAI, where it has been keeping family conversations inside a
// context window for a couple of hundred releases, and left in its own style.
// The comment inside `trim_to_fit` about re-encoding is the reason to take this
// rather than write one: it is a mistake already made and already fixed.
//! Trim messages to fit the model's context window.
//!
//! Rules:
//!   1. Keep the system prompts at the front.
//!   2. Always keep the last message (current user turn) intact.
//!   3. Reserve `max_tokens` for the response.
//!   4. Drop the oldest non-system messages until we fit.
//!   5. Last resort: truncate the current message's content.

use once_cell::sync::Lazy;
use tiktoken_rs::{cl100k_base, CoreBPE};

use super::ChatMessage;

const PER_MESSAGE_OVERHEAD: usize = 4;

static BPE: Lazy<CoreBPE> = Lazy::new(|| cl100k_base().expect("cl100k tokenizer"));

pub fn count_tokens(s: &str) -> usize {
    BPE.encode_with_special_tokens(s).len()
}

pub fn estimate_messages(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .map(|m| count_tokens(m.content()) + PER_MESSAGE_OVERHEAD)
        .sum::<usize>()
        + 2
}

pub fn trim_to_fit(messages: &mut Vec<ChatMessage>, budget: usize) {
    let budget = budget.max(512);

    // Tokenize each message ONCE up front. The previous loop called
    // `estimate_messages` (a full BPE encode of every message) on every
    // iteration, so dropping K messages from N re-encoded O(N·K) bodies --
    // the dominant cost of a long-thread turn. A per-message cost vector
    // kept in lock-step with `messages` makes the trim O(N): removing a
    // message just subtracts its precomputed cost. Token cost of a message
    // is invariant under removal of *other* messages, so the result is
    // identical (same messages dropped, same order).
    let mut costs: Vec<usize> = messages
        .iter()
        .map(|m| count_tokens(m.content()) + PER_MESSAGE_OVERHEAD)
        .collect();
    let mut total: usize = costs.iter().sum::<usize>() + 2;

    if total <= budget {
        return;
    }

    // The turn being worked on starts at the last thing the person said, and
    // everything from there is what this turn has done so far.
    //
    // Dropping the oldest turns is right until the oldest turns are this one.
    // A turn's own tool results are the biggest things in the window, so a long
    // errand trims away its own evidence: first the sentence that started it,
    // then the result proving the work was already done. A model left holding
    // the request and no record of having done it does the only sensible thing
    // it can see, which is to do it again. On a standing job that ran every
    // five minutes, somebody found twenty-four copies of a file meant to be
    // written once, and after protecting only the request, two.
    //
    // So the history before this turn goes first, all of it, before anything
    // belonging to the turn is even considered.
    let turn_begins = messages
        .iter()
        .rposition(|m| matches!(m, ChatMessage::User { .. }));

    if let Some(mut begins) = turn_begins {
        let mut i = 0;
        while total > budget && i < begins && messages.len() > 2 {
            if matches!(messages.get(i), Some(ChatMessage::System { .. })) {
                i += 1;
                continue;
            }
            messages.remove(i);
            total -= costs.remove(i);
            begins -= 1;
        }
    }

    // Only then the turn itself, oldest first, keeping the request and the
    // thing just said. A turn that does not fit on its own is a turn whose
    // tool results are enormous, and something has to go.
    let mut anchor = messages
        .iter()
        .rposition(|m| matches!(m, ChatMessage::User { .. }));

    let mut i = 0;
    while total > budget && messages.len() > 2 {
        let last_idx = messages.len().saturating_sub(1);
        let is_system = matches!(messages.get(i), Some(ChatMessage::System { .. }));
        if i == last_idx || is_system || Some(i) == anchor {
            i += 1;
            if i >= messages.len() {
                break;
            }
            continue;
        }
        messages.remove(i);
        total -= costs.remove(i);
        // Everything after the hole moved down one, the anchor included.
        if let Some(at) = anchor {
            if at > i {
                anchor = Some(at - 1);
            }
        }
    }

    // A tool result whose call went with the trim is a request no provider will
    // take: "Messages with role 'tool' must be a response to a preceding message
    // with 'tool_calls'", a 400, and the turn ends there. It killed a five
    // minute errand that had done all its work and was assembling the answer.
    //
    // Dropping the orphan is right rather than clever. The call it answered is
    // gone, so the result is a reply to a question nobody in the conversation
    // asked, and it reads that way to the model too.
    let mut answered: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut orphan = Vec::new();
    for (at, message) in messages.iter().enumerate() {
        match message {
            ChatMessage::Assistant { tool_calls, .. } => {
                answered.extend(tool_calls.iter().map(|call| call.id.clone()));
            }
            ChatMessage::Tool { tool_call_id, .. } if !answered.contains(tool_call_id) => {
                orphan.push(at);
            }
            _ => {}
        }
    }
    for at in orphan.into_iter().rev() {
        messages.remove(at);
        total -= costs.remove(at);
    }

    if total > budget {
        if let Some(last) = messages.last_mut() {
            let content = match last {
                ChatMessage::System { content } => content,
                ChatMessage::User { content, .. } => content,
                ChatMessage::Assistant { content, .. } => content,
                ChatMessage::Tool { content, .. } => content,
            };
            let allowed = (budget.saturating_sub(128) * 3).max(512);
            if content.chars().count() > allowed {
                *content = content.chars().take(allowed).collect();
                content.push_str("\n…(truncated)");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(said: &str) -> ChatMessage {
        ChatMessage::User {
            content: said.to_string(),
            name: None,
            image_data_urls: Vec::new(),
        }
    }

    fn answering(call: &str, said: &str) -> ChatMessage {
        ChatMessage::Tool {
            content: said.to_string(),
            tool_call_id: call.to_string(),
        }
    }

    fn calling(call: &str) -> ChatMessage {
        ChatMessage::Assistant {
            content: String::new(),
            tool_calls: vec![crate::local::ToolCall {
                id: call.to_string(),
                kind: "function".into(),
                function: crate::local::ToolCallFunction {
                    name: "run_command".into(),
                    arguments: "{}".into(),
                },
            }],
            reasoning: None,
        }
    }

    #[test]
    fn making_room_never_drops_the_request_that_is_being_worked_on() {
        // What actually happened: a standing job wrote one file every five
        // minutes into the same conversation. After a few hours its own tool
        // results filled the window, the trim reached the sentence that had
        // started the turn, and the model went on repeating its last action
        // for twenty-four rounds. The person found twenty-four files.
        let mut talk = vec![
            ChatMessage::System {
                content: "you are an errand".repeat(20),
            },
            user("something asked hours ago"),
            calling("call-old"),
            answering("call-old", &"old output ".repeat(400)),
            user("Write ONE pulse file to the disk"),
            calling("call-write"),
            answering("call-write", &"the output of the write ".repeat(400)),
            calling("call-check"),
            answering("call-check", &"the output of the check ".repeat(400)),
        ];
        trim_to_fit(&mut talk, 600);

        let kept: Vec<&str> = talk.iter().map(|m| m.content()).collect();
        assert!(
            kept.iter().any(|c| c.contains("Write ONE pulse file")),
            "the request being worked on was trimmed away: {kept:?}"
        );
        // And the instructions, which were never in question.
        assert!(matches!(talk.first(), Some(ChatMessage::System { .. })));
    }

    #[test]
    fn a_tool_result_is_never_left_without_the_call_it_answered() {
        // What actually happened: a five minute errand did all its work, the
        // trim took the assistant turn that had made a call while keeping the
        // result that answered it, and the provider refused the whole request
        // with "Messages with role 'tool' must be a response to a preceding
        // message with 'tool_calls'". The turn ended there, with everything it
        // had found thrown away.
        let mut talk = vec![
            ChatMessage::System {
                content: "instructions".into(),
            },
            user(&"an old conversation ".repeat(200)),
            calling("call-old"),
            answering("call-old", &"what that call returned ".repeat(200)),
            user("the request being worked on"),
            calling("call-now"),
            answering("call-now", "the result of this turn"),
        ];
        trim_to_fit(&mut talk, 700);

        let mut offered: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for message in &talk {
            match message {
                ChatMessage::Assistant { tool_calls, .. } => {
                    offered.extend(tool_calls.iter().map(|c| c.id.as_str()));
                }
                ChatMessage::Tool { tool_call_id, .. } => assert!(
                    offered.contains(tool_call_id.as_str()),
                    "a result for {tool_call_id} survived without its call"
                ),
                _ => {}
            }
        }
        // And this turn's own work is still there, which is the other half.
        let kept: Vec<&str> = talk.iter().map(|m| m.content()).collect();
        assert!(kept
            .iter()
            .any(|c| c.contains("the request being worked on")));
        assert!(kept.iter().any(|c| c.contains("the result of this turn")));
    }

    #[test]
    fn what_this_turn_already_did_outlives_the_conversation_before_it() {
        // The exact sequence off somebody's disk: write the file, say it
        // landed, make a note, and then the note's result is kept while the
        // proof of the write is dropped, so it writes the file again.
        let mut talk = vec![
            ChatMessage::System {
                content: "instructions".into(),
            },
            user(&"a conversation from hours ago ".repeat(200)),
            calling("call-old"),
            answering("call-old", &"and its output ".repeat(200)),
            user("Write ONE pulse file"),
            calling("call-write"),
            answering(
                "call-write",
                "the pulse file was written: errand-pulse-081717.txt",
            ),
            calling("call-note"),
            answering("call-note", "the note was made"),
        ];
        trim_to_fit(&mut talk, 700);
        let kept: Vec<&str> = talk.iter().map(|m| m.content()).collect();
        assert!(
            kept.iter().any(|c| c.contains("pulse file was written")),
            "the proof the work was done was dropped: {kept:?}"
        );
        assert!(kept.iter().any(|c| c.contains("Write ONE pulse file")));
        assert!(
            !kept.iter().any(|c| c.contains("hours ago")),
            "the old conversation should have gone first"
        );
    }

    #[test]
    fn the_oldest_turns_still_go_first_when_there_is_something_to_drop() {
        let mut talk = vec![
            ChatMessage::System {
                content: "instructions".into(),
            },
            user("the oldest thing"),
            calling("call-old"),
            answering("call-old", &"filler ".repeat(500)),
            user("the newest thing"),
            calling("call-new"),
            answering("call-new", "small"),
        ];
        trim_to_fit(&mut talk, 400);
        let kept: Vec<&str> = talk.iter().map(|m| m.content()).collect();
        assert!(!kept.iter().any(|c| c.contains("the oldest thing")));
        assert!(kept.iter().any(|c| c.contains("the newest thing")));
    }
}
