//! The other wire format.
//!
//! Errand spoke one protocol: OpenAI chat completions. That is the one almost
//! everything offers, and it was a reasonable place to stop until it wasn't:
//! DeepSeek serves a second surface at `/anthropic`, Z.ai serves one at
//! `/api/anthropic`, and somebody running Errand beside another tool that
//! already points at those has a working address that Errand simply could not
//! use. "It works everywhere else and not here" is not an answer.
//!
//! Everything below turns that format into the one the rest of the app already
//! speaks. Nothing downstream of `ChatDelta` knows there are two protocols,
//! which is the point: a second format is worth having only if it is not a
//! second everything.
//!
//! The differences that matter, all of which are silent failures if missed:
//!
//!   * the system prompt is a field of its own, not a message with a role
//!   * `max_tokens` is required rather than optional
//!   * a tool result is a *user* message holding a `tool_result` block, not a
//!     role of its own, and consecutive ones belong in a single message
//!   * the key rides in `x-api-key`, not `Authorization`
//!   * tools are `{name, description, input_schema}`, not wrapped in `function`
//!   * a tool call's arguments arrive as a stream of JSON fragments that mean
//!     nothing until the block ends

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::stream::{ChatDelta, StreamHandle, ToolCallAccum};
use super::{ChatMessage, LlmSettings, ToolDef};

/// What this app was written against.
///
/// Pinned rather than tracking the newest, because a version header is a
/// promise about the shape of what comes back and following one that has moved
/// is how a client starts guessing.
const WRITTEN_AGAINST: &str = "2023-06-01";

/// What to ask for when nobody said.
///
/// Required by this format, unlike the other one, and a request without it is
/// refused. Large enough not to cut anybody's answer off in the middle.
const AS_MUCH_AS_ANYBODY_NEEDS: usize = 8192;

/// Everything that goes in the body, built from what the rest of the app has.
pub fn what_to_send(
    settings: &LlmSettings,
    messages: &[ChatMessage],
    tools: &[ToolDef],
    max_tokens: Option<usize>,
    force_tool: bool,
) -> Value {
    let (system, turns) = sort_out(messages);

    let mut body = json!({
        "model": settings.model,
        "max_tokens": max_tokens.unwrap_or(AS_MUCH_AS_ANYBODY_NEEDS),
        "messages": turns,
        "stream": true,
    });
    if !system.is_empty() {
        body["system"] = Value::String(system);
    }
    if let Some(warmth) = settings.temperature {
        body["temperature"] = json!(warmth);
    }
    let offered: Vec<Value> = tools.iter().filter_map(|t| as_a_tool(&t.schema)).collect();
    if !offered.is_empty() {
        body["tools"] = Value::Array(offered);
        // "any" is this format's word for the other one's "required": some
        // tool, not a named one.
        body["tool_choice"] = json!({ "type": if force_tool { "any" } else { "auto" } });
    }
    body
}

/// One tool, from the shape the other format wants into this one's.
///
/// Returns nothing for anything that is not a function declaration, rather than
/// sending a malformed tool and having the whole request refused for it.
fn as_a_tool(openai: &Value) -> Option<Value> {
    let f = openai.get("function")?;
    Some(json!({
        "name": f.get("name")?,
        "description": f.get("description").cloned().unwrap_or_else(|| json!("")),
        "input_schema": f
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
    }))
}

/// Split the conversation into the system prompt and the turns.
///
/// The two things this format wants that the other does not: the system prompt
/// lifted out of the list entirely, and tool results folded into user messages
/// rather than standing on their own.
fn sort_out(messages: &[ChatMessage]) -> (String, Vec<Value>) {
    let mut system = Vec::new();
    let mut turns: Vec<Value> = Vec::new();

    for m in messages {
        match m {
            ChatMessage::System { content } => system.push(content.clone()),

            ChatMessage::User {
                content,
                image_data_urls,
                ..
            } => turns.push(json!({
                "role": "user",
                "content": what_a_person_said(content, image_data_urls),
            })),

            ChatMessage::Assistant {
                content,
                tool_calls,
                reasoning,
            } => {
                let mut blocks = Vec::new();
                // Kept and handed back, the same as the other format, and for
                // the same reason: a model that thinks out loud wants its own
                // working back on the next turn.
                if let Some(shown) = reasoning {
                    if !shown.trim().is_empty() {
                        blocks.push(json!({ "type": "thinking", "thinking": shown }));
                    }
                }
                if !content.trim().is_empty() {
                    blocks.push(json!({ "type": "text", "text": content }));
                }
                for call in tool_calls {
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": call.id,
                        "name": call.function.name,
                        // Sent as the object it is. This format wants the
                        // arguments parsed, where the other one wants them as a
                        // string, and sending the string is refused.
                        "input": serde_json::from_str::<Value>(&call.function.arguments)
                            .unwrap_or_else(|_| json!({})),
                    }));
                }
                // An assistant turn with nothing in it at all is refused, and
                // an empty turn is a thing that happens.
                if blocks.is_empty() {
                    blocks.push(json!({ "type": "text", "text": "" }));
                }
                turns.push(json!({ "role": "assistant", "content": blocks }));
            }

            ChatMessage::Tool {
                content,
                tool_call_id,
            } => {
                let result = json!({
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": content,
                });
                // Folded into the user message before it where there is one, so
                // two tools answered in the same turn arrive as one message.
                // Sent separately they are two user turns in a row, which this
                // format refuses.
                match turns.last_mut() {
                    Some(last)
                        if last["role"] == "user"
                            && last["content"].as_array().is_some_and(|c| {
                                c.first().is_some_and(|b| b["type"] == "tool_result")
                            }) =>
                    {
                        if let Some(blocks) = last["content"].as_array_mut() {
                            blocks.push(result);
                        }
                    }
                    _ => turns.push(json!({ "role": "user", "content": [result] })),
                }
            }
        }
    }
    (system.join("\n\n"), turns)
}

/// What somebody said, with anything they attached.
fn what_a_person_said(content: &str, pictures: &[String]) -> Value {
    if pictures.is_empty() {
        return Value::String(content.to_string());
    }
    let mut blocks = Vec::new();
    for one in pictures {
        // `data:image/png;base64,AAAA` split into the two things this format
        // wants separately. Anything not in that shape is dropped rather than
        // sent as something it is not.
        if let Some(rest) = one.strip_prefix("data:") {
            if let Some((kind, data)) = rest.split_once(";base64,") {
                blocks.push(json!({
                    "type": "image",
                    "source": { "type": "base64", "media_type": kind, "data": data },
                }));
            }
        }
    }
    if !content.is_empty() {
        blocks.push(json!({ "type": "text", "text": content }));
    }
    Value::Array(blocks)
}

/// Ask, and turn what comes back into the deltas the rest of the app knows.
pub async fn stream(
    client: &reqwest::Client,
    settings: &LlmSettings,
    messages: &[ChatMessage],
    tools: &[ToolDef],
    max_tokens: Option<usize>,
    cancel: tokio_util::sync::CancellationToken,
    force_tool: bool,
) -> Result<StreamHandle> {
    let url = settings.reach("messages");
    let body = what_to_send(settings, messages, tools, max_tokens, force_tool);

    let mut req = client
        .post(&url)
        .header("content-type", "application/json")
        .header("anthropic-version", WRITTEN_AGAINST)
        .json(&body);
    // This format's own header. Sending it as a bearer instead is the mistake
    // that reads as a bad key when the key is fine.
    if let Some(key) = &settings.api_key {
        req = req.header("x-api-key", key.clone());
    }

    super::stream::open_as(req, cancel, super::stream::Wire::Anthropic).await
}

/// One SSE event from this format, turned into what the app understands.
///
/// Returns nothing for the events that carry no news, which is most of them.
/// `blocks` remembers what each open content block is, because a delta says
/// only its index and the same delta shape means different things depending on
/// what was opened.
pub fn read_event(kind: &str, data: &str, blocks: &mut Vec<Block>) -> Vec<ChatDelta> {
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return vec![];
    };
    match kind {
        "content_block_start" => {
            let at = v["index"].as_u64().unwrap_or(0) as usize;
            let opened = &v["content_block"];
            let block = match opened["type"].as_str() {
                Some("tool_use") => Block::Tool {
                    id: opened["id"].as_str().unwrap_or_default().to_string(),
                    name: opened["name"].as_str().unwrap_or_default().to_string(),
                    arguments: String::new(),
                },
                Some("thinking") | Some("redacted_thinking") => Block::Thinking,
                _ => Block::Text,
            };
            while blocks.len() <= at {
                blocks.push(Block::Text);
            }
            blocks[at] = block;
            vec![]
        }

        "content_block_delta" => {
            let at = v["index"].as_u64().unwrap_or(0) as usize;
            let delta = &v["delta"];
            match delta["type"].as_str() {
                Some("text_delta") => match delta["text"].as_str().unwrap_or_default() {
                    "" => vec![],
                    said => vec![ChatDelta::Token(said.to_string())],
                },
                Some("thinking_delta") => match delta["thinking"].as_str().unwrap_or_default() {
                    "" => vec![],
                    said => vec![ChatDelta::Reasoning(said.to_string())],
                },
                // Arguments arrive as fragments of JSON that are not JSON until
                // the block closes, so they are only collected here.
                Some("input_json_delta") => {
                    if let Some(Block::Tool { arguments, .. }) = blocks.get_mut(at) {
                        arguments.push_str(delta["partial_json"].as_str().unwrap_or_default());
                    }
                    vec![]
                }
                _ => vec![],
            }
        }

        // Where a tool call becomes whole. Sent here rather than at the end of
        // the message so that a model asking for two tools has the first one
        // on its way while the second is still arriving.
        "content_block_stop" => {
            let at = v["index"].as_u64().unwrap_or(0) as usize;
            match blocks.get(at) {
                Some(Block::Tool {
                    id,
                    name,
                    arguments,
                }) => vec![ChatDelta::ToolCall(ToolCallAccum {
                    // Which content block it was, which is what this format
                    // counts by and is the same idea as the other one's.
                    index: at,
                    id: Some(id.clone()),
                    name: Some(name.clone()),
                    arguments: match arguments.trim().is_empty() {
                        // An argument-less tool sends no fragments at all, and
                        // an empty string is not valid JSON on the far side.
                        true => "{}".to_string(),
                        false => arguments.clone(),
                    },
                })],
                _ => vec![],
            }
        }

        "message_delta" => match v["delta"]["stop_reason"].as_str() {
            Some(why) => vec![ChatDelta::Done {
                reason: why.to_string(),
            }],
            None => vec![],
        },

        "message_stop" => vec![ChatDelta::Done {
            reason: "done".to_string(),
        }],

        // Said rather than swallowed: this is how a bad key, an unknown model
        // and being over a rate limit all arrive, and each wants a different
        // thing done about it.
        "error" => vec![ChatDelta::Error(
            v["error"]["message"]
                .as_str()
                .unwrap_or("the model server sent an error with no message")
                .to_string(),
        )],

        _ => vec![],
    }
}

/// What one open content block is, so its deltas can be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text,
    Thinking,
    Tool {
        id: String,
        name: String,
        arguments: String,
    },
}

/// Ask what models are served here.
///
/// The same idea as the other format's, at this format's address and with this
/// format's header.
pub async fn list_models(client: &reqwest::Client, settings: &LlmSettings) -> Result<Vec<String>> {
    let url = settings.reach("models");
    let mut req = client
        .get(&url)
        .header("anthropic-version", WRITTEN_AGAINST);
    if let Some(key) = &settings.api_key {
        req = req.header("x-api-key", key.clone());
    }
    let said = req.send().await?;
    if !said.status().is_success() {
        return Err(anyhow!("{url} responded {}", said.status()));
    }
    let body: Value = said.json().await?;
    Ok(body["data"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|m| m["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::{ToolCall, ToolCallFunction};

    fn settings() -> LlmSettings {
        LlmSettings {
            model: "deepseek-v4-flash".into(),
            base_url: "https://api.deepseek.com/anthropic".into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_system_prompt_is_lifted_out_of_the_conversation() {
        // This format has a field for it. Left in the list as a message with a
        // role, it is refused outright.
        let body = what_to_send(
            &settings(),
            &[
                ChatMessage::System {
                    content: "You are careful.".into(),
                },
                ChatMessage::User {
                    content: "Hello".into(),
                    name: None,
                    image_data_urls: vec![],
                },
            ],
            &[],
            None,
            false,
        );
        assert_eq!(body["system"], "You are careful.");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn something_is_always_asked_for_because_this_format_insists() {
        // Optional in the other format, required here, and a request without it
        // is refused rather than defaulted.
        let body = what_to_send(&settings(), &[], &[], None, false);
        assert_eq!(body["max_tokens"], AS_MUCH_AS_ANYBODY_NEEDS);
        let asked = what_to_send(&settings(), &[], &[], Some(64), false);
        assert_eq!(asked["max_tokens"], 64);
    }

    #[test]
    fn two_tool_results_in_a_row_arrive_as_one_turn() {
        // A tool result is a user message in this format, and two user messages
        // in a row are refused. A model that asks for two tools at once is
        // ordinary, so this is the common case rather than an edge.
        let body = what_to_send(
            &settings(),
            &[
                ChatMessage::Assistant {
                    content: String::new(),
                    tool_calls: vec![],
                    reasoning: None,
                },
                ChatMessage::Tool {
                    content: "first".into(),
                    tool_call_id: "a".into(),
                },
                ChatMessage::Tool {
                    content: "second".into(),
                    tool_call_id: "b".into(),
                },
            ],
            &[],
            None,
            false,
        );
        let turns = body["messages"].as_array().unwrap();
        assert_eq!(turns.len(), 2, "{turns:#?}");
        assert_eq!(turns[1]["content"].as_array().unwrap().len(), 2);
        assert_eq!(turns[1]["content"][0]["tool_use_id"], "a");
        assert_eq!(turns[1]["content"][1]["tool_use_id"], "b");
    }

    #[test]
    fn a_tool_call_goes_back_as_an_object_rather_than_a_string() {
        // The other format wants the arguments as a string of JSON. This one
        // wants the JSON. Sending the string is refused.
        let body = what_to_send(
            &settings(),
            &[ChatMessage::Assistant {
                content: "Looking.".into(),
                tool_calls: vec![ToolCall {
                    id: "call-1".into(),
                    kind: "function".into(),
                    function: ToolCallFunction {
                        name: "read_file".into(),
                        arguments: r#"{"path":"notes.txt"}"#.into(),
                    },
                }],
                reasoning: None,
            }],
            &[],
            None,
            false,
        );
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        let call = blocks.iter().find(|b| b["type"] == "tool_use").unwrap();
        assert_eq!(call["input"]["path"], "notes.txt");
    }

    #[test]
    fn a_tool_is_offered_in_this_formats_shape_and_a_malformed_one_is_left_out() {
        let good = ToolDef {
            name: "read_file".into(),
            description: "Read a file".into(),
            schema: json!({
                "type": "function",
                "function": {
                    "name": "read_file",
                    "description": "Read a file",
                    "parameters": { "type": "object", "properties": {} }
                }
            }),
        };
        let bad = ToolDef {
            name: "broken".into(),
            description: String::new(),
            schema: json!({ "nothing": "useful" }),
        };
        let body = what_to_send(&settings(), &[], &[good, bad], None, true);
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1, "a malformed tool went out: {tools:#?}");
        assert_eq!(tools[0]["name"], "read_file");
        assert!(tools[0]["input_schema"].is_object());
        // This format's word for the other one's "required".
        assert_eq!(body["tool_choice"]["type"], "any");
    }

    #[test]
    fn a_tool_calls_arguments_are_whole_before_anybody_is_told_about_it() {
        // They arrive as fragments that are not JSON until the block closes.
        // Handed on early, the far side gets half an object.
        let mut blocks = Vec::new();
        read_event(
            "content_block_start",
            r#"{"index":0,"content_block":{"type":"tool_use","id":"t1","name":"read_file"}}"#,
            &mut blocks,
        );
        for fragment in [r#"{\"pa"#, r#"th\":\"no"#, r#"tes.txt\"}"#] {
            let said = format!(
                r#"{{"index":0,"delta":{{"type":"input_json_delta","partial_json":"{fragment}"}}}}"#
            );
            assert!(
                read_event("content_block_delta", &said, &mut blocks).is_empty(),
                "a fragment was handed on before it was whole"
            );
        }
        let out = read_event("content_block_stop", r#"{"index":0}"#, &mut blocks);
        match out.first() {
            Some(ChatDelta::ToolCall(call)) => {
                assert_eq!(call.name.as_deref(), Some("read_file"));
                assert_eq!(call.id.as_deref(), Some("t1"));
                let args: Value = serde_json::from_str(&call.arguments).expect("whole JSON");
                assert_eq!(args["path"], "notes.txt");
            }
            other => panic!("no tool call came out: {other:?}"),
        }
    }

    #[test]
    fn a_tool_that_takes_nothing_still_sends_something_that_parses() {
        // No fragments arrive at all for one of these, and an empty string is
        // not JSON on the far side.
        let mut blocks = Vec::new();
        read_event(
            "content_block_start",
            r#"{"index":0,"content_block":{"type":"tool_use","id":"t1","name":"whats_the_time"}}"#,
            &mut blocks,
        );
        let out = read_event("content_block_stop", r#"{"index":0}"#, &mut blocks);
        match out.first() {
            Some(ChatDelta::ToolCall(call)) => assert_eq!(call.arguments, "{}"),
            other => panic!("no tool call came out: {other:?}"),
        }
    }

    #[test]
    fn words_and_thinking_are_told_apart() {
        // The same delta shape means different things depending on which block
        // was opened, so what was opened has to be remembered.
        let mut blocks = Vec::new();
        read_event(
            "content_block_start",
            r#"{"index":0,"content_block":{"type":"thinking"}}"#,
            &mut blocks,
        );
        let thought = read_event(
            "content_block_delta",
            r#"{"index":0,"delta":{"type":"thinking_delta","thinking":"hmm"}}"#,
            &mut blocks,
        );
        assert!(matches!(thought.first(), Some(ChatDelta::Reasoning(t)) if t == "hmm"));

        let said = read_event(
            "content_block_delta",
            r#"{"index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
            &mut blocks,
        );
        assert!(matches!(said.first(), Some(ChatDelta::Token(t)) if t == "Hello"));
    }

    #[test]
    fn an_error_from_the_server_is_said_rather_than_swallowed() {
        // A bad key, an unknown model and being over a rate limit all arrive
        // this way, and each wants something different done about it.
        let mut blocks = Vec::new();
        let out = read_event(
            "error",
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
            &mut blocks,
        );
        assert!(matches!(out.first(), Some(ChatDelta::Error(why)) if why.contains("x-api-key")));
    }

    #[test]
    fn the_address_is_this_formats_own() {
        // `/v1/messages`, not `/v1/chat/completions`, off whatever prefix was
        // settled on.
        assert_eq!(
            settings().reach("messages"),
            "https://api.deepseek.com/anthropic/v1/messages"
        );
    }
}
