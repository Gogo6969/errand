//! A whole turn in the other protocol, against a server that speaks it.
//!
//! The pieces of this format each have a test of their own, and pieces passing
//! is not the same as a turn working: the request has to be one that a server
//! accepts, the answer has to be read back, and a tool call has to come out the
//! far end whole. None of that is provable by looking at one function.
//!
//! So there is a server here. It is about forty lines of raw TCP because this
//! crate has no HTTP server in it and adding one to run a test is a poor trade,
//! and it does the one thing that matters: it takes the request Errand actually
//! sends, keeps it for the test to look at, and answers in the shape a real one
//! answers in.
//!
//! No key, no network, no account. It runs everywhere and it runs in a second.

use std::sync::{Arc, Mutex};

use errand_core::local::stream::ChatDelta;
use errand_core::local::talk::LlmClient;
use errand_core::local::{ChatMessage, LlmSettings, ToolDef};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A server that answers the way this format answers.
///
/// Returns where it is listening and somewhere the request will appear, so a
/// test can check what was actually sent as well as what came back.
type WhatArrived = Arc<Mutex<(String, String)>>;

async fn a_server_that_speaks_it(events: Vec<(&'static str, String)>) -> (String, WhatArrived) {
    let listening = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a port");
    let at = format!("http://{}", listening.local_addr().unwrap());
    // The request line and the body. The line matters as much as the body:
    // asking the right question at the wrong address is the failure this whole
    // format was added to stop.
    let asked: WhatArrived = Arc::default();
    let keeping = asked.clone();

    tokio::spawn(async move {
        let Ok((mut them, _)) = listening.accept().await else {
            return;
        };
        // Enough of the request to find the body. The test's requests are small
        // and arrive at once; a real server would not be this trusting.
        let mut got = vec![0u8; 64 * 1024];
        let read = them.read(&mut got).await.unwrap_or(0);
        let whole = String::from_utf8_lossy(&got[..read]).to_string();
        if let Some((head, body)) = whole.split_once("\r\n\r\n") {
            let line = head.lines().next().unwrap_or_default().to_string();
            *keeping.lock().unwrap() = (line, body.to_string());
        }

        let mut said = String::from(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
        );
        for (kind, data) in events {
            said.push_str(&format!("event: {kind}\ndata: {data}\n\n"));
        }
        let _ = them.write_all(said.as_bytes()).await;
        let _ = them.flush().await;
    });

    (at, asked)
}

fn talking_to(at: &str) -> LlmClient {
    LlmClient::new(LlmSettings {
        provider: "openai-compat".into(),
        base_url: format!("{at}/v1"),
        model: "deepseek-v4-flash".into(),
        wire: "anthropic".into(),
        ..Default::default()
    })
}

async fn everything(mut handle: errand_core::local::stream::StreamHandle) -> Vec<ChatDelta> {
    let mut all = Vec::new();
    while let Some(one) = handle.rx.recv().await {
        let over = matches!(one, ChatDelta::Done { .. });
        all.push(one);
        if over {
            break;
        }
    }
    all
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_in_the_other_protocol_is_asked_and_answered() {
    let (at, asked) = a_server_that_speaks_it(vec![
        ("message_start", r#"{"type":"message_start"}"#.into()),
        (
            "content_block_start",
            r#"{"index":0,"content_block":{"type":"text"}}"#.into(),
        ),
        (
            "content_block_delta",
            r#"{"index":0,"delta":{"type":"text_delta","text":"Hello"}}"#.into(),
        ),
        (
            "content_block_delta",
            r#"{"index":0,"delta":{"type":"text_delta","text":" there"}}"#.into(),
        ),
        ("content_block_stop", r#"{"index":0}"#.into()),
        (
            "message_delta",
            r#"{"delta":{"stop_reason":"end_turn"}}"#.into(),
        ),
        ("message_stop", "{}".into()),
    ])
    .await;

    let said = everything(
        talking_to(&at)
            .stream(
                &[
                    ChatMessage::System {
                        content: "Be brief.".into(),
                    },
                    ChatMessage::User {
                        content: "Say hello".into(),
                        name: None,
                        image_data_urls: vec![],
                    },
                ],
                &[],
                None,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .expect("it asked"),
    )
    .await;

    // What came back.
    let words: String = said
        .iter()
        .filter_map(|d| match d {
            ChatDelta::Token(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(words, "Hello there");
    assert!(
        said.iter()
            .any(|d| matches!(d, ChatDelta::Done { reason } if reason == "end_turn")),
        "the turn never ended: {said:?}"
    );
    // Said once, though the server says it twice: this format sends a reason
    // and then a close, and both arriving would end the turn twice.
    assert_eq!(
        said.iter()
            .filter(|d| matches!(d, ChatDelta::Done { .. }))
            .count(),
        1,
        "the turn ended more than once: {said:?}"
    );

    // And what went out.
    let (line, body) = asked.lock().unwrap().clone();
    // The address, not just the body. `/v1/messages`, not the other format's
    // `/v1/chat/completions`, and a POST.
    assert_eq!(
        line, "POST /v1/messages HTTP/1.1",
        "asked at the wrong address"
    );
    let sent: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    assert_eq!(sent["system"], "Be brief.", "{sent:#}");
    assert_eq!(sent["messages"][0]["role"], "user");
    assert!(sent["max_tokens"].is_number(), "{sent:#}");
    assert_eq!(sent["stream"], true);
    // Nothing about sampling, which is what stops this failing outright
    // against a model that pins it.
    assert!(sent.get("temperature").is_none(), "{sent:#}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_call_survives_the_whole_round_trip() {
    let (at, asked) = a_server_that_speaks_it(vec![
        (
            "content_block_start",
            r#"{"index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"read_file"}}"#
                .into(),
        ),
        (
            "content_block_delta",
            r#"{"index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#.into(),
        ),
        (
            "content_block_delta",
            r#"{"index":0,"delta":{"type":"input_json_delta","partial_json":"\"notes.txt\"}"}}"#
                .into(),
        ),
        ("content_block_stop", r#"{"index":0}"#.into()),
        (
            "message_delta",
            r#"{"delta":{"stop_reason":"tool_use"}}"#.into(),
        ),
    ])
    .await;

    let tool = ToolDef {
        name: "read_file".into(),
        description: "Read a file".into(),
        schema: serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        }),
    };

    let said = everything(
        talking_to(&at)
            .stream(
                &[ChatMessage::User {
                    content: "Read notes.txt".into(),
                    name: None,
                    image_data_urls: vec![],
                }],
                &[tool],
                None,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .expect("it asked"),
    )
    .await;

    let call = said
        .iter()
        .find_map(|d| match d {
            ChatDelta::ToolCall(c) => Some(c),
            _ => None,
        })
        .expect("a tool call came back");
    assert_eq!(call.name.as_deref(), Some("read_file"));
    assert_eq!(call.id.as_deref(), Some("toolu_1"));
    // Whole, from fragments that were not JSON on their own.
    let args: serde_json::Value = serde_json::from_str(&call.arguments).expect("whole JSON");
    assert_eq!(args["path"], "notes.txt");

    // The tool went out in this format's shape, not the other one's.
    let (line, body) = asked.lock().unwrap().clone();
    // The address, not just the body. `/v1/messages`, not the other format's
    // `/v1/chat/completions`, and a POST.
    assert_eq!(
        line, "POST /v1/messages HTTP/1.1",
        "asked at the wrong address"
    );
    let sent: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    assert_eq!(sent["tools"][0]["name"], "read_file", "{sent:#}");
    assert!(
        sent["tools"][0]["input_schema"].is_object(),
        "the tool went out wrapped in the other format's shape: {sent:#}"
    );
    assert!(
        sent["tools"][0].get("function").is_none(),
        "the other format's wrapper went out: {sent:#}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_thinking_model_shows_is_kept_apart_from_its_answer() {
    let (at, _asked) = a_server_that_speaks_it(vec![
        (
            "content_block_start",
            r#"{"index":0,"content_block":{"type":"thinking"}}"#.into(),
        ),
        (
            "content_block_delta",
            r#"{"index":0,"delta":{"type":"thinking_delta","thinking":"weighing it up"}}"#.into(),
        ),
        ("content_block_stop", r#"{"index":0}"#.into()),
        (
            "content_block_start",
            r#"{"index":1,"content_block":{"type":"text"}}"#.into(),
        ),
        (
            "content_block_delta",
            r#"{"index":1,"delta":{"type":"text_delta","text":"Four."}}"#.into(),
        ),
        ("content_block_stop", r#"{"index":1}"#.into()),
        (
            "message_delta",
            r#"{"delta":{"stop_reason":"end_turn"}}"#.into(),
        ),
    ])
    .await;

    let said = everything(
        talking_to(&at)
            .stream(
                &[ChatMessage::User {
                    content: "What is two and two?".into(),
                    name: None,
                    image_data_urls: vec![],
                }],
                &[],
                None,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .expect("it asked"),
    )
    .await;

    let answer: String = said
        .iter()
        .filter_map(|d| match d {
            ChatDelta::Token(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    let working: String = said
        .iter()
        .filter_map(|d| match d {
            ChatDelta::Reasoning(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();

    // The two must not run together. Joined, the model's private working ends
    // up in what somebody reads and in everything the agent later summarises.
    assert_eq!(answer, "Four.");
    assert_eq!(working, "weighing it up");
}
