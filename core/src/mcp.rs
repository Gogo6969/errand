//! Tools that live outside this app, and both engines' way in to them.
//!
//! This is here rather than inside an engine, and that is the whole point of
//! the file. Claude Code brings its own MCP client, so for a while Errand had
//! MCP only when Claude was answering, and a local model got five built-in
//! tools and nothing else. That looked like a limitation of local models and
//! was really a decision about where the client lived, made by accident.
//!
//! Grok Bot does not make it: its documentation says an MCP server's tools
//! "become available to the model", whichever model that is. Tools belong to
//! the harness. So they belong here.
//!
//! The servers are read from `~/.claude.json`, which is where Claude Code keeps
//! them, rather than from a list of our own. One list means the two engines
//! genuinely see the same tools instead of two lists that drift, and it means
//! anything already set up works here without being set up again.
//!
//! Nothing in here ever prints a server's environment. Those are the variables
//! people put API keys in.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

/// How long a server gets to say hello before it is given up on.
///
/// Generous, because several of these are `npx` and the first run of one
/// downloads a package. Not unlimited, because a server that never answers
/// would otherwise hold up every thread that wanted a tool.
const TO_SAY_HELLO: Duration = Duration::from_secs(30);

/// How long one tool call gets.
const TO_ANSWER: Duration = Duration::from_secs(120);

/// A server somebody has configured, before anything has been started.
#[derive(Debug, Clone)]
pub struct Configured {
    pub name: String,
    pub how: How,
    /// Which file said so, for a window that has to explain where a tool came
    /// from.
    pub from: String,
}

/// The ways a server can be reached.
#[derive(Debug, Clone)]
pub enum How {
    /// A program on this machine, spoken to on its own pipes.
    Program {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    /// Somewhere on the network. Recognised so it can be listed and explained,
    /// rather than dropped silently and wondered about later.
    Remote { kind: String, url: String },
}

/// One tool a server offers.
#[derive(Debug, Clone)]
pub struct Tool {
    /// `mcp__server__tool`, which is what Claude Code calls it too. The same
    /// tool having the same name under both engines is worth the ugliness.
    pub called: String,
    /// What it is called on its own server.
    pub own_name: String,
    pub server: String,
    pub description: String,
    /// The JSON Schema for its arguments, as the server gave it.
    pub takes: Value,
}

/// Every server named in the files Claude Code reads.
///
/// User-level first, then anything the working directory adds, so a project can
/// bring tools of its own. A name defined twice takes the nearer one, which is
/// the rule everywhere else that layers configuration.
pub fn configured(cwd: &Path) -> Vec<Configured> {
    let mut found: Vec<Configured> = Vec::new();
    let mut add = |name: String, spec: &Value, from: &str| {
        if let Some(how) = read_how(spec) {
            found.retain(|c| c.name != name);
            found.push(Configured {
                name,
                how,
                from: from.to_string(),
            });
        }
    };

    let home = std::env::var("HOME").unwrap_or_default();
    let theirs = Path::new(&home).join(".claude.json");
    if let Some(all) = read_json(&theirs) {
        if let Some(servers) = all.get("mcpServers").and_then(|m| m.as_object()) {
            for (name, spec) in servers {
                add(name.clone(), spec, "~/.claude.json");
            }
        }
        // Claude Code also files servers under the project they belong to,
        // keyed by its absolute path.
        let here = cwd.to_string_lossy().to_string();
        if let Some(mine) = all.pointer(&format!("/projects/{}", escape(&here))) {
            if let Some(servers) = mine.get("mcpServers").and_then(|m| m.as_object()) {
                for (name, spec) in servers {
                    add(name.clone(), spec, "this folder");
                }
            }
        }
    }

    if let Some(all) = read_json(&cwd.join(".mcp.json")) {
        if let Some(servers) = all.get("mcpServers").and_then(|m| m.as_object()) {
            for (name, spec) in servers {
                add(name.clone(), spec, ".mcp.json");
            }
        }
    }

    found
}

/// A path as a JSON pointer segment.
fn escape(path: &str) -> String {
    path.replace('~', "~0").replace('/', "~1")
}

fn read_json(at: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(at).ok()?).ok()
}

/// One server's entry, as something we know how to reach.
fn read_how(spec: &Value) -> Option<How> {
    let kind = spec
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or(match spec.get("command") {
            Some(_) => "stdio",
            None => "http",
        });

    match kind {
        "stdio" => Some(How::Program {
            command: spec.get("command")?.as_str()?.to_string(),
            args: spec
                .get("args")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            env: spec
                .get("env")
                .and_then(|e| e.as_object())
                .map(|e| {
                    e.iter()
                        .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        other => Some(How::Remote {
            kind: other.to_string(),
            url: spec.get("url")?.as_str()?.to_string(),
        }),
    }
}

// ------------------------------------------------------------ talking to one --

/// A server that is running and answering.
struct Link {
    asks: tokio::sync::mpsc::UnboundedSender<(Value, oneshot::Sender<Result<Value>>)>,
}

impl Link {
    /// Start the program and shake hands with it.
    async fn start(
        name: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut child = tokio::process::Command::new(command)
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Servers talk to themselves on stderr constantly. It is not ours
            // to show and it must not fill a pipe nobody is reading.
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("starting the {name} server"))?;

        let mut stdin = child.stdin.take().context("its stdin")?;
        let stdout = child.stdout.take().context("its stdout")?;

        let waiting: Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value>>>>> = Arc::default();
        let answered = waiting.clone();

        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue; // Servers do print the odd thing that is not JSON.
                };
                let Some(id) = v.get("id").and_then(|i| i.as_i64()) else {
                    continue; // A notification. Nothing is waiting for it.
                };
                if let Some(who) = answered.lock().unwrap().remove(&id) {
                    let _ = who.send(match v.get("error") {
                        Some(bad) => Err(anyhow!("{}", said_why(bad))),
                        None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                    });
                }
            }
            // The server has gone. Anything still waiting will wait for ever
            // unless it is told, and a tool call that never returns is a thread
            // that never finishes.
            for (_, who) in answered.lock().unwrap().drain() {
                let _ = who.send(Err(anyhow!("the server stopped")));
            }
        });

        let (asks, mut asked) =
            tokio::sync::mpsc::unbounded_channel::<(Value, oneshot::Sender<Result<Value>>)>();
        tokio::spawn(async move {
            while let Some((message, who)) = asked.recv().await {
                if let Some(id) = message.get("id").and_then(|i| i.as_i64()) {
                    waiting.lock().unwrap().insert(id, who);
                }
                let line = format!("{message}\n");
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
            }
            let _ = child.kill().await;
        });

        let link = Self { asks };

        // The handshake. Version taken from the specification rather than
        // negotiated, because every server in the wild accepts this one and a
        // server that does not is a server we cannot use anyway.
        link.ask(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "errand", "version": env!("CARGO_PKG_VERSION") },
            }),
            TO_SAY_HELLO,
        )
        .await?;
        link.tell("notifications/initialized", json!({}));
        Ok(link)
    }

    /// Ask something and wait for the answer.
    async fn ask(&self, method: &str, params: Value, within: Duration) -> Result<Value> {
        static NEXT: AtomicI64 = AtomicI64::new(1);
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        let (tell_me, answer) = oneshot::channel();
        self.asks
            .send((
                json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
                tell_me,
            ))
            .map_err(|_| anyhow!("the server is not running"))?;

        match tokio::time::timeout(within, answer).await {
            Err(_) => Err(anyhow!("it did not answer within {}s", within.as_secs())),
            Ok(Err(_)) => Err(anyhow!("the server stopped before answering")),
            Ok(Ok(answer)) => answer,
        }
    }

    /// Say something that wants no answer.
    fn tell(&self, method: &str, params: Value) {
        let (nobody, _) = oneshot::channel();
        let _ = self.asks.send((
            json!({ "jsonrpc": "2.0", "method": method, "params": params }),
            nobody,
        ));
    }
}

/// What a server said went wrong, in whatever shape it said it.
fn said_why(bad: &Value) -> String {
    bad.get("message")
        .and_then(|m| m.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| bad.to_string())
}

// --------------------------------------------------------------- all of them --

/// Every server that is running, and everything they can do.
#[derive(Default)]
pub struct Servers {
    links: HashMap<String, Link>,
    tools: Vec<Tool>,
    /// The ones that did not start, and why, in words. Kept rather than
    /// discarded: a tool that is quietly absent looks to everybody like a tool
    /// the agent chose not to use.
    pub trouble: Vec<(String, String)>,
}

impl Servers {
    /// Start everything configured for this directory and ask what it offers.
    ///
    /// One that fails to start does not stop the others. Most people have a
    /// server configured that they have not used for months.
    pub async fn open(cwd: &Path) -> Self {
        let mut servers = Servers::default();
        for want in configured(cwd) {
            let (command, args, env) = match &want.how {
                How::Program { command, args, env } => (command, args, env),
                How::Remote { kind, .. } => {
                    servers.trouble.push((
                        want.name.clone(),
                        format!("{kind} servers are not reached from here yet"),
                    ));
                    continue;
                }
            };

            match Link::start(&want.name, command, args, env).await {
                // `{:#}` rather than `{}`: the plain form gives only the
                // outermost context, so a server that could not be found at all
                // reported "starting the mempalace server" and stopped there,
                // which names the thing that failed and not the reason.
                Err(why) => servers
                    .trouble
                    .push((want.name.clone(), format!("{why:#}"))),
                Ok(link) => match link.ask("tools/list", json!({}), TO_SAY_HELLO).await {
                    Err(why) => servers
                        .trouble
                        .push((want.name.clone(), format!("it started but {why:#}"))),
                    Ok(listed) => {
                        for tool in listed
                            .get("tools")
                            .and_then(|t| t.as_array())
                            .unwrap_or(&vec![])
                        {
                            let Some(own_name) = tool.get("name").and_then(|n| n.as_str()) else {
                                continue;
                            };
                            servers.tools.push(Tool {
                                called: format!("mcp__{}__{}", want.name, own_name),
                                own_name: own_name.to_string(),
                                server: want.name.clone(),
                                description: tool
                                    .get("description")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                takes: tool
                                    .get("inputSchema")
                                    .cloned()
                                    .unwrap_or(json!({ "type": "object" })),
                            });
                        }
                        servers.links.insert(want.name.clone(), link);
                    }
                },
            }
        }
        servers
    }

    /// Everything on offer.
    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// Is this one of ours?
    pub fn knows(&self, called: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.called == called)
    }

    /// Use one, and say what came back.
    pub async fn call(&self, called: &str, args: &Value) -> Result<String> {
        let tool = self
            .knows(called)
            .ok_or_else(|| anyhow!("there is no tool called {called} here"))?;
        let link = self
            .links
            .get(&tool.server)
            .ok_or_else(|| anyhow!("the {} server is not running", tool.server))?;

        let answer = link
            .ask(
                "tools/call",
                json!({ "name": tool.own_name, "arguments": args }),
                TO_ANSWER,
            )
            .await?;

        // A server can say a call failed without the call itself failing, and
        // that has to reach the model as a result rather than as an error, or
        // it treats the whole route as impossible instead of this attempt.
        let went_wrong = answer
            .get("isError")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);
        let said = in_words(answer.get("content"));
        Ok(match went_wrong {
            true => format!("That did not work: {said}"),
            false => said,
        })
    }
}

/// What a server sent back, as text.
///
/// Content arrives as a list of parts which may be text, images or references
/// to other resources. Only text can go into a conversation here; the rest is
/// named rather than dropped, so a model is told a picture came back instead of
/// being handed nothing and drawing its own conclusion.
fn in_words(content: Option<&Value>) -> String {
    let Some(parts) = content.and_then(|c| c.as_array()) else {
        return content.map(|c| c.to_string()).unwrap_or_default();
    };
    parts
        .iter()
        .map(|part| match part.get("type").and_then(|t| t.as_str()) {
            Some("text") => part
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            Some(other) => format!("[{other}]"),
            None => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_is_read_the_way_claude_code_writes_it_down() {
        let spec = json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@steipete/peekaboo", "mcp"],
            "env": { "SOME_KEY": "value" }
        });
        let How::Program { command, args, env } = read_how(&spec).expect("a program") else {
            panic!("that is a program");
        };
        assert_eq!(command, "npx");
        assert_eq!(args, ["-y", "@steipete/peekaboo", "mcp"]);
        assert_eq!(env.get("SOME_KEY").map(String::as_str), Some("value"));
    }

    #[test]
    fn an_entry_with_no_type_is_a_program_when_it_names_a_command() {
        // Older entries were written before the field existed.
        let spec = json!({ "command": "/usr/local/bin/thing" });
        assert!(matches!(read_how(&spec), Some(How::Program { .. })));
    }

    #[test]
    fn a_server_somewhere_else_is_recognised_rather_than_quietly_skipped() {
        // It cannot be reached yet. Being able to say so is the point: a tool
        // that is silently absent looks like a tool the agent declined to use.
        let spec = json!({ "type": "http", "url": "https://docs.x.com/mcp" });
        let Some(How::Remote { kind, url }) = read_how(&spec) else {
            panic!("that is somewhere else");
        };
        assert_eq!(kind, "http");
        assert_eq!(url, "https://docs.x.com/mcp");
    }

    #[test]
    fn what_a_server_sends_back_becomes_something_a_model_can_read() {
        let content = json!([
            { "type": "text", "text": "first" },
            { "type": "image", "data": "…" },
            { "type": "text", "text": "second" }
        ]);
        assert_eq!(
            in_words(Some(&content)),
            "first\n[image]\nsecond",
            "a picture is named rather than dropped"
        );
    }

    #[test]
    fn a_failure_says_what_the_server_called_it() {
        assert_eq!(
            said_why(&json!({ "message": "no such window" })),
            "no such window"
        );
        // And falls back to the whole thing rather than to nothing.
        assert_eq!(said_why(&json!({ "code": -32601 })), r#"{"code":-32601}"#);
    }
}
