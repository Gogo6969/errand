//! Does Claude Code actually come through the door?
//!
//! Everything about the doorway that can be checked without a process is
//! checked in the module itself. This is the rest, and it is the half that
//! matters: that Claude Code starts the program, accepts what it says in
//! `initialize`, is willing to list the two tools, puts them in front of the
//! model rather than behind a search, and carries a call all the way to the
//! queue an agent's answer comes out of.
//!
//! None of that can be asserted from a unit test, and all of it fails silently.
//! A server that answers `initialize` without declaring tools connects
//! perfectly and is never asked what it can do. A configuration missing
//! `alwaysLoad` works, slowly and occasionally, because the model has to think
//! to go looking for the ability to delegate. Both look like an agent that
//! chose not to delegate.
//!
//! Ignored by default because it spends somebody's Claude subscription.
//!
//!     cargo build -p errand-app
//!     cargo test -p errand-core --test doorway_live -- --ignored --nocapture

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use errand_core::doorway;
use serde_json::{json, Value};

/// The app's own binary, which is also the doorway.
///
/// Found beside the test rather than configured, because that is the same
/// relationship `current_exe()` gives the running app: the doorway is always
/// the program that is already there.
fn the_program() -> PathBuf {
    let mut at = std::env::current_exe().expect("where this test is");
    at.pop(); // deps
    at.pop(); // debug
    let program = at.join("errand-app");
    assert!(
        program.exists(),
        "build it first: cargo build -p errand-app ({})",
        program.display()
    );
    program
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "spends a Claude subscription; run with --ignored"]
async fn claude_code_finds_both_tools_and_a_call_reaches_the_conversation_that_owns_the_socket() {
    let here = std::env::temp_dir().join(format!("errand-doorway-{}", std::process::id()));
    std::fs::create_dir_all(&here).expect("somewhere to work");
    let socket = here.join("door.sock");

    // The app's half: a listener that stands in for the delegation queue and
    // answers with something no model would invent on its own.
    let (wants, mut asked) = tokio::sync::mpsc::unbounded_channel();
    let door = doorway::listen(socket.clone(), "the-conversation".into(), wants).expect("a door");

    let heard = tokio::spawn(async move {
        let mut seen = Vec::new();
        while let Some(one) = asked.recv().await {
            seen.push((one.from.clone(), one.tool.clone()));
            let _ = one
                .answer
                .send(Ok("You can hand work to: Marmalade (Kitchen)".into()));
        }
        seen
    });

    // Claude Code's half: the real flags the app uses, and the real config the
    // app generates, pointed at the real program.
    let config = doorway::config(&the_program(), door.at());
    let mut child = Command::new("claude")
        .args([
            "--print",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-mode",
            "default",
            "--permission-prompt-tool",
            "stdio",
            "--allowedTools",
            "mcp__errand__who_else",
            "mcp__errand__remember",
            "mcp__errand__recall",
            "mcp__errand__forget",
            "--mcp-config",
            &config,
        ])
        .current_dir(&here)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("starting claude");

    let mut writing = child.stdin.take().expect("its stdin");
    let reading = BufReader::new(child.stdout.take().expect("its stdout"));

    // The handshake that turns on the control channel. Without it the
    // permission question never arrives and a tool that needs one dies as
    // "Stream closed" instead.
    let hello = json!({ "type": "control_request", "request_id": "hello",
                        "request": { "subtype": "initialize" } });
    writeln!(writing, "{hello}").expect("saying hello");
    let say = json!({ "type": "user", "message": { "role": "user", "content": [
        { "type": "text", "text":
          "Call the who_else tool, then repeat its answer back to me word for word. \
           Do not do anything else." } ] } });
    writeln!(writing, "{say}").expect("asking");
    writing.flush().ok();

    let mut called_it = false;
    let mut searched_first = false;
    let mut listed: Vec<String> = Vec::new();
    let mut answer = String::new();

    for line in reading.lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match v.get("type").and_then(|t| t.as_str()).unwrap_or_default() {
            // What Claude Code decided it had, before the model saw anything.
            "system" => {
                if let Some(tools) = v.get("tools").and_then(|t| t.as_array()) {
                    listed = tools
                        .iter()
                        .filter_map(|t| t.as_str())
                        .filter(|t| t.contains("errand"))
                        .map(str::to_string)
                        .collect();
                }
            }
            "assistant" => {
                for part in v
                    .pointer("/message/content")
                    .and_then(|c| c.as_array())
                    .unwrap_or(&vec![])
                {
                    match part.get("name").and_then(|n| n.as_str()) {
                        Some("mcp__errand__who_else") => called_it = true,
                        Some("ToolSearch") => searched_first = true,
                        _ => {}
                    }
                }
            }
            "result" => {
                answer = v
                    .get("result")
                    .and_then(|r| r.as_str())
                    .unwrap_or_default()
                    .to_string();
                break;
            }
            _ => {}
        }
    }
    let _ = child.kill();
    // Reaped, not just killed. A test that leaves a zombie behind is a test
    // that leaves a zombie behind every time it is run.
    let _ = child.wait();

    // Closed before the collector is waited on. It only finishes when every
    // sender is gone, and the door holds one: waiting first and closing after
    // is a five second timeout and an empty list that looks exactly like a call
    // that never arrived.
    drop(door);
    let seen = tokio::time::timeout(std::time::Duration::from_secs(5), heard)
        .await
        .expect("the collector finished")
        .expect("it did not panic");

    // Every one of them, by name. `.any()` would pass with four of the five
    // missing, and a tool the model cannot see is a tool that does not exist.
    for wanted in ["ask", "who_else", "remember", "recall", "forget"] {
        assert!(
            listed.iter().any(|t| t.ends_with(&format!("__{wanted}"))),
            "`{wanted}` was not in front of the model at the start. It was offered: {listed:?}"
        );
    }
    assert!(
        !searched_first,
        "it had to go looking for the tools, which means alwaysLoad stopped working"
    );
    assert!(called_it, "it never called who_else; it said: {answer}");
    assert_eq!(
        seen,
        vec![("the-conversation".to_string(), "who_else".to_string())],
        "the call has to arrive as the conversation that owns the socket, under \
         the plain name"
    );
    assert!(
        answer.contains("Marmalade"),
        "the answer did not come back through: {answer}"
    );

    // The socket went with the conversation above, or a process that outlived
    // its conversation would still have a way in.
    assert!(!socket.exists());
    let _ = std::fs::remove_dir_all(&here);
}
