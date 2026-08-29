//! The way in, for an engine that runs outside this app.
//!
//! The local engine sits inside our own tool loop, so handing it `ask` is a
//! function call. Claude Code does not: it holds its own loop and will only
//! take a tool from an MCP server. For a while that meant delegation existed on
//! one engine and not the other, which looked like a limitation of Claude Code
//! and was really a decision about where the tools lived.
//!
//! So the app grows a doorway. Claude Code starts this same program with
//! `--doorway`, that program carries the call over a socket, and the socket
//! ends at the same `team::Wants` queue the local engine posts to. Both engines
//! reach one implementation of what asking somebody else means, which is the
//! only arrangement in which "who is there" and "always allow" can mean the
//! same thing under both.
//!
//! The conversation is never sent. It is not in the socket's name, not in the
//! arguments, not on the wire: it is captured by the listener, and a doorway
//! can only reach the socket it was handed. A conversation id on the wire is a
//! conversation id that can be wrong, and the failure that buys is an agent
//! being handed somebody else's answer.
//!
//! Nothing here writes anything to stdout that is not a message. Claude Code
//! reads that pipe as protocol, and one stray line of chat on it is a server
//! that fails to start for reasons nobody can see.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::team;

/// What Claude Code is told to run this program with.
pub const IN_ARGV: &str = "--doorway";

/// The revisions of the protocol this knows about, newest first.
///
/// A tools-only server speaks all of these identically: `initialize`,
/// `tools/list` and `tools/call` did not change shape across any of them. What
/// matters is answering with one of them rather than inventing one, because a
/// version the client does not recognise makes it drop the server before it
/// ever asks what the server can do, and it says so nowhere a person will look.
const SPOKEN: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// How long the doorway waits for the app to answer one call.
///
/// Deliberately longer than the app's own ten minutes for a delegated errand,
/// so that a slow answer is reported by the side that knows why it was slow. If
/// this were the shorter of the two, every long delegation would come back as a
/// timeout from a process that has no idea what it was waiting for.
const TO_ANSWER: Duration = Duration::from_secs(660);

/// One request, one answer, one connection.
///
/// Not JSON-RPC, because there is nothing here to correlate: a connection
/// carries a single call and then closes. Ids exist to tell two answers apart
/// on one pipe, and a protocol that cannot have two answers does not need them.
#[derive(Debug, Serialize, Deserialize)]
struct Passed {
    tool: String,
    args: Value,
}

/// What came back, and whether it is an answer or a difficulty.
#[derive(Debug, Serialize, Deserialize)]
struct Came {
    said: String,
    went_wrong: bool,
}

// ------------------------------------------------------------ the server --

/// Speak MCP on this process's own pipes until the far end goes away.
///
/// Blocking and synchronous on purpose. This is a program whose whole job is to
/// copy a line from one place to another, and starting an async runtime to do
/// it would be more machinery than the thing being done.
pub fn serve_blocking(socket: &Path) -> ! {
    let input = std::io::stdin();
    let mut output = std::io::stdout();

    for line in BufReader::new(input.lock()).lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            // Not JSON at all. Skipped rather than answered, because there is
            // no id to answer to and a made-up one is worse than silence.
            eprintln!("doorway: ignoring a line that is not JSON");
            continue;
        };

        // No id means a notification, and a notification is never replied to.
        // `initialized` and `cancelled` both arrive this way, and answering
        // either of them is a protocol error that reads as a broken server.
        let Some(id) = message.get("id").filter(|id| !id.is_null()).cloned() else {
            continue;
        };
        let method = message
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or_default();
        // A message with no params at all is legal, and treating that as an
        // error would refuse a perfectly good `tools/list`.
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

        let answer = match method {
            "initialize" => Ok(hello(&params)),
            "tools/list" => Ok(json!({ "tools": offered() })),
            "ping" => Ok(json!({})),
            "tools/call" => Ok(called(socket, &params)),
            other => Err((-32601, format!("there is no {other} here"))),
        };

        // One or the other, never both, and never a null placeholder for the
        // one that did not happen: the client validates this envelope strictly
        // and a `"error": null` beside a good result fails the whole handshake.
        let reply = match answer {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
            }
        };
        if writeln!(output, "{reply}").is_err() || output.flush().is_err() {
            break;
        }
    }

    // Its stdin closed, which is how the client says it is finished. Leaving
    // quietly is what keeps a force-quit from leaving this behind.
    std::process::exit(0)
}

/// The answer to `initialize`.
fn hello(params: &Value) -> Value {
    let wanted = params
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    json!({
        "protocolVersion": match SPOKEN.contains(&wanted) {
            true => wanted,
            false => SPOKEN[0],
        },
        // Saying nothing here is the mistake that connects cleanly and is then
        // never asked for its tools, because a client will not call a method
        // for a capability that was not declared.
        "capabilities": { "tools": {} },
        "serverInfo": { "name": team::DOORWAY, "version": env!("CARGO_PKG_VERSION") },
    })
}

/// The two tools, in the shape MCP wants them.
///
/// Read from `team::declarations()` rather than written out again, so that the
/// wording an agent reads cannot drift between the engine that gets them as
/// function schemas and the engine that gets them through here.
fn offered() -> Vec<Value> {
    team::declarations()
        .iter()
        .filter_map(|declared| {
            let f = declared.get("function")?;
            Some(json!({
                "name": f.get("name")?,
                "description": f.get("description").cloned().unwrap_or_default(),
                // A tool with no schema, or one whose schema is not an object,
                // fails validation for the whole server rather than for itself.
                "inputSchema": f
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
            }))
        })
        .collect()
}

/// Carry one tool call to the app and bring back what it said.
///
/// Every way this can fail comes back as a result rather than as a JSON-RPC
/// error, and the difference matters: a result the model can read is something
/// it can act on or repeat to a person, where an error collapses into a code
/// and ends the step.
fn called(socket: &Path, params: &Value) -> Value {
    let tool = params
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or_default();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let came = carry(
        socket,
        &Passed {
            tool: tool.to_string(),
            args,
        },
    );
    let (said, went_wrong) = match came {
        Ok(came) => (came.said, came.went_wrong),
        Err(why) => (why, true),
    };
    json!({ "content": [{ "type": "text", "text": said }], "isError": went_wrong })
}

/// One connection, one call, one answer.
///
/// Connected here rather than at startup, so that a doorway whose app is not
/// running still starts, still lists its tools, and fails at the moment of use
/// with a sentence. A server that refuses to start is a capability that is
/// silently absent, which is indistinguishable from an agent that chose not to
/// use it.
fn carry(socket: &Path, passing: &Passed) -> Result<Came, String> {
    let mut link = UnixStream::connect(socket)
        .map_err(|_| "Errand is not running, so there is nobody to hand this to.".to_string())?;
    link.set_read_timeout(Some(TO_ANSWER)).ok();
    link.set_write_timeout(Some(TO_ANSWER)).ok();

    let line = serde_json::to_string(passing).map_err(|e| e.to_string())?;
    link.write_all(line.as_bytes())
        .and_then(|()| link.write_all(b"\n"))
        .and_then(|()| link.flush())
        .map_err(|_| "Errand stopped listening part way through.".to_string())?;
    // Said out loud, so the app knows nothing more is coming and can stop
    // waiting for a second line that will never arrive.
    link.shutdown(std::net::Shutdown::Write).ok();

    let mut back = String::new();
    link.read_to_string(&mut back).map_err(|_| {
        format!(
            "Errand did not answer within {} minutes.",
            TO_ANSWER.as_secs() / 60
        )
    })?;
    serde_json::from_str(back.trim())
        .map_err(|_| "Errand answered with something unreadable.".to_string())
}

// ------------------------------------------------------------- the house --

/// The socket end that belongs to one conversation.
///
/// Held by whoever opened the conversation. Dropping it stops answering and
/// takes the file away, so a conversation that is closed cannot be reached by a
/// process that outlived it.
pub struct Doorway {
    at: PathBuf,
    listening: tokio::task::JoinHandle<()>,
}

impl Drop for Doorway {
    fn drop(&mut self) {
        self.listening.abort();
        let _ = std::fs::remove_file(&self.at);
    }
}

impl Doorway {
    /// Where this doorway is, so it can be named in a config.
    pub fn at(&self) -> &Path {
        &self.at
    }
}

/// Answer for one conversation, on a socket of its own.
///
/// Must be called from inside a runtime, which the one caller is.
pub fn listen(
    at: PathBuf,
    from: String,
    wants: tokio::sync::mpsc::UnboundedSender<team::Wants>,
) -> Result<Doorway> {
    if let Some(parent) = at.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("making {}", parent.display()))?;
        // Nobody else's business. The socket below is the door into somebody's
        // agents, and the directory is the first of the two locks on it.
        permit(parent, 0o700);
    }
    // A socket file left by an app that crashed cannot be bound over, and
    // refusing to start because of a dead file is a failure nobody could act
    // on.
    let _ = std::fs::remove_file(&at);

    // Bound first, then narrowed. Setting the umask around the bind is the
    // usual trick and is wrong here: a umask belongs to the whole process, so
    // for as long as it is held every other thread that creates a file or a
    // directory gets it too. That is not theoretical. It made a directory
    // created elsewhere in this process mode 0600, which cannot be entered, and
    // it showed up as a store that could not be opened and a sandboxed command
    // that was refused, neither of them anywhere near this file.
    //
    // Nothing is lost by the order. The window in which the socket exists at
    // the default mode is real, but it sits inside a directory that is already
    // 0700, and a socket nobody can reach is not one anybody can connect to.
    // The directory is the lock; this is the second turn of it.
    let bound = tokio::net::UnixListener::bind(&at)
        .with_context(|| format!("listening at {}", at.display()))?;
    permit(&at, 0o600);

    let listening = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = bound.accept().await else {
                continue;
            };
            // One task each. Claude Code can put two tool calls in a single
            // message, and answering them one after the other would make the
            // second wait out the first, which for a delegated errand is ten
            // minutes of nothing.
            let from = from.clone();
            let wants = wants.clone();
            tokio::spawn(async move {
                answer_one(stream, from, wants).await;
            });
        }
    });

    Ok(Doorway { at, listening })
}

/// Read the one call on this connection, put it in the queue, write the answer.
async fn answer_one(
    stream: tokio::net::UnixStream,
    from: String,
    wants: tokio::sync::mpsc::UnboundedSender<team::Wants>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    let (reading, mut writing) = stream.into_split();
    let mut line = String::new();
    if tokio::io::BufReader::new(reading)
        .read_line(&mut line)
        .await
        .is_err()
    {
        return;
    }
    let Ok(passed) = serde_json::from_str::<Passed>(line.trim()) else {
        return;
    };

    let (tell_me, answer) = tokio::sync::oneshot::channel();
    let said = match wants.send(team::Wants {
        tool: passed.tool,
        args: passed.args,
        from,
        answer: tell_me,
    }) {
        Err(_) => Err(anyhow::anyhow!("Errand is no longer answering.")),
        Ok(()) => answer
            .await
            .unwrap_or_else(|_| Err(anyhow::anyhow!("Nobody answered."))),
    };

    let came = match said {
        Ok(said) => Came {
            said,
            went_wrong: false,
        },
        // The reason, as a sentence, because the model is going to read it and
        // decide what to do next. "there is nobody here called Scribe" is
        // something it can recover from; a code is not.
        Err(why) => Came {
            said: format!("{why:#}"),
            went_wrong: true,
        },
    };
    if let Ok(back) = serde_json::to_string(&came) {
        let _ = writing.write_all(back.as_bytes()).await;
        let _ = writing.flush().await;
    }
}

/// What Claude Code has to be told so it can find this.
///
/// Passed on the command line rather than written into `~/.claude.json`. That
/// file is also what this app reads to build the *local* engine's tools, so an
/// entry there would hand the local engine a second, socket-shaped route to
/// `ask` beside the one it already has in process: two implementations of the
/// thing this whole arrangement exists to have one of.
///
/// `alwaysLoad` is what keeps the two tools in front of the model instead of
/// behind a search. Without it every MCP tool is deferred, and an agent that
/// has to think to go looking for the ability to delegate is an agent that
/// mostly will not.
pub fn config(program: &Path, socket: &Path) -> String {
    let mut server = Map::new();
    server.insert("command".into(), json!(program.to_string_lossy()));
    server.insert(
        "args".into(),
        json!([IN_ARGV, socket.to_string_lossy().to_string()]),
    );
    server.insert("alwaysLoad".into(), json!(true));

    let mut servers = Map::new();
    servers.insert(team::DOORWAY.to_string(), Value::Object(server));
    json!({ "mcpServers": Value::Object(servers) }).to_string()
}

/// A file or directory, only for the person whose it is.
fn permit(what: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(what, std::fs::Permissions::from_mode(mode));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(line: &str) -> Value {
        serde_json::from_str(line).expect("a message")
    }

    #[test]
    fn a_server_that_forgets_to_say_it_has_tools_is_never_asked_for_them() {
        // The failure this guards is the quiet one: the handshake succeeds, the
        // server shows as connected, and `tools/list` is never called because
        // nothing said there were any.
        let said = hello(&ask(r#"{"protocolVersion":"2024-11-05"}"#));
        assert!(
            said.pointer("/capabilities/tools").is_some(),
            "it did not declare tools, so nothing will ever ask for them"
        );
        assert_eq!(said["protocolVersion"], "2024-11-05");
        assert_eq!(said["serverInfo"]["name"], team::DOORWAY);
    }

    #[test]
    fn a_protocol_version_we_do_not_know_is_answered_with_one_we_do() {
        // Echoing back something unrecognised makes the client drop the server
        // before asking it anything, and say so nowhere anybody looks.
        let said = hello(&ask(r#"{"protocolVersion":"1999-01-01"}"#));
        assert_eq!(said["protocolVersion"], SPOKEN[0]);
        let none = hello(&ask("{}"));
        assert_eq!(none["protocolVersion"], SPOKEN[0]);
    }

    #[test]
    fn both_tools_are_offered_with_a_schema_that_is_an_object() {
        // One tool with a missing or non-object schema fails validation for the
        // whole server, taking the other one with it.
        let tools = offered();
        let named: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(named, ["ask", "who_else"]);
        for tool in &tools {
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
            assert!(
                tool["description"].as_str().is_some_and(|d| !d.is_empty()),
                "a tool nobody explained is a tool nobody calls"
            );
        }
    }

    #[test]
    fn what_a_tool_is_called_here_is_what_it_is_called_to_the_other_engine() {
        // Two lists of the same two tools would drift, and the day they did,
        // an "always allow" would stop meaning the same thing on both engines.
        for tool in offered() {
            let name = tool["name"].as_str().expect("a name");
            assert_eq!(team::which_of_ours(name).map(team::Ours::name), Some(name));
        }
    }

    #[test]
    fn a_doorway_that_cannot_reach_the_app_says_so_instead_of_hanging() {
        let nowhere = std::path::Path::new("/tmp/errand-no-such-doorway.sock");
        let said = called(nowhere, &ask(r#"{"name":"who_else","arguments":{}}"#));
        assert_eq!(said["isError"], true);
        let text = said["content"][0]["text"].as_str().unwrap_or_default();
        assert!(
            text.contains("not running"),
            "a person has to be able to read why: {text}"
        );
    }

    #[test]
    fn what_claude_code_is_told_names_this_program_and_this_socket() {
        let said = config(
            std::path::Path::new("/Applications/Errand.app/Contents/MacOS/errand-app"),
            std::path::Path::new("/tmp/x/3f8a1c2b9d4e5607.sock"),
        );
        let v: Value = serde_json::from_str(&said).expect("valid json");
        let mine = &v["mcpServers"][team::DOORWAY];
        assert_eq!(
            mine["command"],
            "/Applications/Errand.app/Contents/MacOS/errand-app"
        );
        assert_eq!(mine["args"][0], IN_ARGV);
        assert_eq!(mine["args"][1], "/tmp/x/3f8a1c2b9d4e5607.sock");
        assert_eq!(
            mine["alwaysLoad"], true,
            "without this the two tools sit behind a search and mostly go unused"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_call_through_the_doorway_arrives_as_the_conversation_that_owns_the_socket() {
        // The one claim the whole arrangement rests on: nothing on the wire
        // says which conversation this is, so nothing on the wire can be wrong
        // about it.
        let at = std::env::temp_dir().join(format!("errand-test-{}.sock", std::process::id()));
        let (wants, mut asked) = tokio::sync::mpsc::unbounded_channel();
        let door = listen(at.clone(), "the-conversation".into(), wants).expect("a doorway");

        let answering = tokio::spawn(async move {
            let one = asked.recv().await.expect("something was asked");
            assert_eq!(one.from, "the-conversation");
            assert_eq!(one.tool, "who_else");
            let _ = one.answer.send(Ok("There is nobody else yet.".into()));
        });

        let socket = door.at().to_path_buf();
        let said = tokio::task::spawn_blocking(move || {
            called(&socket, &ask(r#"{"name":"who_else","arguments":{}}"#))
        })
        .await
        .expect("the call finished");

        answering.await.expect("the answer went back");
        assert_eq!(said["isError"], false);
        assert_eq!(said["content"][0]["text"], "There is nobody else yet.");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_closed_conversation_takes_its_doorway_with_it() {
        let at = std::env::temp_dir().join(format!("errand-gone-{}.sock", std::process::id()));
        let (wants, _asked) = tokio::sync::mpsc::unbounded_channel();
        let door = listen(at.clone(), "whoever".into(), wants).expect("a doorway");
        assert!(at.exists());
        drop(door);
        assert!(
            !at.exists(),
            "a socket left behind is a way in to a conversation that is over"
        );
    }
}
