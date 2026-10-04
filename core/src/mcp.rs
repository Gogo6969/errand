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
//!
//! Only the person's own servers are started, and only in the form they
//! allowed. The servers started here run as the person, outside every wall,
//! and they were read from files a teammate could write: its own folder's
//! `.mcp.json` is inside its wall, and walled Claude Code may write
//! `~/.claude.json`. So a server named in a teammate's folder is listed and
//! never started, and one in the person's own list starts only while its
//! command, arguments and environment are exactly what the person allowed.

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
    /// Whose list it is in, which decides whether this app may start it.
    pub found: Found,
}

/// Where a server was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// The person's own list, at the top of `~/.claude.json`.
    TheirOwn,
    /// Filed under this folder in `~/.claude.json`.
    ThisFolder,
    /// The folder's own `.mcp.json`, which a teammate can write.
    McpJson,
}

/// Whether this app starts a server, and if not why not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// The person allowed it as it is now.
    Allowed,
    /// The person has not allowed it yet.
    NotYet,
    /// Allowed once, and changed since.
    Changed,
    /// Set up in a teammate's own folder, which this app never starts.
    InItsFolder,
}

/// Why a server was not started, in the words the Tools panel shows.
pub const NOT_ALLOWED_YET: &str = "not started: you have not allowed it in Errand yet";
pub const CHANGED_SINCE_ALLOWED: &str = "not started: it changed since you allowed it";
pub const IN_ITS_OWN_FOLDER: &str =
    "not started: it is set up for this agent's folder rather than in your own list";

impl Configured {
    /// What exactly this app would run, as a fingerprint: the command, its
    /// arguments, and its environment with values, in a fixed order. Values
    /// count, because an added interpreter or loader variable turns an allowed
    /// command into any code at all. Nothing for a server on the network.
    pub fn fingerprint(&self) -> Option<String> {
        use sha2::Digest;
        let How::Program { command, args, env } = &self.how else {
            return None;
        };
        let mut env: Vec<(&String, &String)> = env.iter().collect();
        env.sort();
        let canonical = serde_json::to_string(&(command, args, env)).ok()?;
        let hash = sha2::Sha256::digest(canonical.as_bytes());
        Some(hash.iter().map(|b| format!("{b:02x}")).collect())
    }

    /// What it runs, for the person to judge: the command line, and the names
    /// of the variables it sets, never their values.
    pub fn shown(&self) -> String {
        match &self.how {
            How::Remote { kind, url } => format!("{kind} at {url}"),
            How::Program { command, args, env } => {
                let mut said = std::iter::once(command.as_str())
                    .chain(args.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" ");
                if !env.is_empty() {
                    let mut keys: Vec<&String> = env.keys().collect();
                    keys.sort();
                    said.push_str(&format!(
                        " (with {} set)",
                        keys.iter()
                            .map(|k| k.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                said
            }
        }
    }
}

/// Whether a server is one that drives the screen: clicking, typing, looking.
///
/// Never allowed without the person choosing it, because such a server can
/// press a teammate's own cards and type into a terminal, which runs outside
/// every wall.
pub fn drives_the_screen(server: &Configured) -> bool {
    let named = |s: &str| {
        let s = s.to_ascii_lowercase();
        s.contains("peekaboo") || s.contains("computer-use") || s.contains("computer_use")
    };
    named(&server.name)
        || match &server.how {
            How::Program { command, args, .. } => named(command) || args.iter().any(|a| named(a)),
            How::Remote { url, .. } => named(url),
        }
}

/// Whether a reason a server was not started is one of the app's own choices
/// rather than the server failing: a server and a copy of it in a teammate's
/// folder share a name, and only the reason tells their two lines apart.
pub fn not_started_by_choice(why: &str) -> bool {
    why == NOT_ALLOWED_YET || why == CHANGED_SINCE_ALLOWED || why == IN_ITS_OWN_FOLDER
}

/// Whether this app may start a server, against what the person allowed.
pub fn standing(server: &Configured, allowed: &HashMap<String, String>) -> Standing {
    if server.found != Found::TheirOwn {
        return Standing::InItsFolder;
    }
    match (allowed.get(&server.name), server.fingerprint()) {
        (Some(was), Some(now)) if *was == now => Standing::Allowed,
        (Some(_), _) => Standing::Changed,
        (None, _) => Standing::NotYet,
    }
}

/// What the person has allowed, by server name, kept in this process.
///
/// In memory rather than read from a file each time, the way the wall keeps
/// the folders allowed: the app fills it from its own store, which no wall
/// lets a teammate touch, and a file this side of the wall would be one more
/// thing to keep out of reach. Empty until the app says otherwise, so a test,
/// or the command-line harness, starts nothing.
fn allowed_registry() -> &'static Mutex<HashMap<String, String>> {
    static ALLOWED: std::sync::OnceLock<Mutex<HashMap<String, String>>> =
        std::sync::OnceLock::new();
    ALLOWED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Replace what is allowed with this, by name and fingerprint.
pub fn allow_these(allowed: HashMap<String, String>) {
    *allowed_registry().lock().unwrap_or_else(|e| e.into_inner()) = allowed;
}

/// What is allowed now.
pub fn allowed_now() -> HashMap<String, String> {
    allowed_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// The person's own server by this name, as long as it is still exactly what
/// they were shown: allowing what the panel showed, and not whatever the entry
/// was changed to while they looked.
pub fn the_one_shown(name: &str, fingerprint: &str) -> Result<Configured> {
    let home = std::env::var("HOME").unwrap_or_default();
    the_one_shown_in(Path::new(&home), name, fingerprint)
}

fn the_one_shown_in(theirs: &Path, name: &str, fingerprint: &str) -> Result<Configured> {
    let found = configured_in(theirs, Path::new("/nowhere/at/all"))
        .into_iter()
        .find(|c| c.name == name && c.found == Found::TheirOwn)
        .ok_or_else(|| anyhow!("there is no server called {name} in your own list"))?;
    if found.fingerprint().as_deref() != Some(fingerprint) {
        anyhow::bail!("{name} changed since it was shown, so it was not allowed: look again");
    }
    Ok(found)
}

/// Where a server's program is looked for when the person's own PATH cannot
/// be read. Their own is used first, as their terminal has it: the folders on
/// it that package managers keep programs in are no longer writable from
/// inside any wall, so what is found there is what they put there.
const WHERE_PROGRAMS_ARE: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

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
/// User-level first, then anything the working directory adds. Each is kept
/// with where it was found, and none replaces another by name: a teammate's
/// folder defining `mempalace` would otherwise quietly take the place of the
/// person's own.
pub fn configured(cwd: &Path) -> Vec<Configured> {
    let home = std::env::var("HOME").unwrap_or_default();
    configured_in(Path::new(&home), cwd)
}

/// The same, with the home folder named, so a test can lay one out.
fn configured_in(home: &Path, cwd: &Path) -> Vec<Configured> {
    let mut found: Vec<Configured> = Vec::new();
    let mut add = |name: String, spec: &Value, from: &str, whose: Found| {
        if let Some(how) = read_how(spec) {
            found.push(Configured {
                name,
                how,
                from: from.to_string(),
                found: whose,
            });
        }
    };

    let theirs = home.join(".claude.json");
    if let Some(all) = read_json(&theirs) {
        if let Some(servers) = all.get("mcpServers").and_then(|m| m.as_object()) {
            for (name, spec) in servers {
                add(name.clone(), spec, "~/.claude.json", Found::TheirOwn);
            }
        }
        // Claude Code also files servers under the project they belong to,
        // keyed by its absolute path.
        let here = cwd.to_string_lossy().to_string();
        if let Some(mine) = all.pointer(&format!("/projects/{}", escape(&here))) {
            if let Some(servers) = mine.get("mcpServers").and_then(|m| m.as_object()) {
                for (name, spec) in servers {
                    add(name.clone(), spec, "this folder", Found::ThisFolder);
                }
            }
        }
    }

    // Only a plain file, and not a large one: it is listed, never started, so
    // nothing here needs more than its names.
    let mcp_json = cwd.join(".mcp.json");
    let plain = std::fs::symlink_metadata(&mcp_json)
        .is_ok_and(|m| m.file_type().is_file() && m.len() < 1_000_000);
    if plain {
        if let Some(all) = read_json(&mcp_json) {
            if let Some(servers) = all.get("mcpServers").and_then(|m| m.as_object()) {
                for (name, spec) in servers {
                    add(name.clone(), spec, ".mcp.json", Found::McpJson);
                }
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
        // As little of this app's own environment as a program needs, and the
        // server's own variables on top: whatever else the app was started
        // with is not part of what the person allowed.
        let mut child = tokio::process::Command::new(command);
        child.env_clear();
        for keep in [
            "HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "LC_ALL", "SHELL",
        ] {
            if let Ok(value) = std::env::var(keep) {
                child.env(keep, value);
            }
        }
        child.env(
            "PATH",
            crate::claude::the_persons_path().unwrap_or_else(|| WHERE_PROGRAMS_ARE.to_string()),
        );
        let mut child = child
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

/// What something claims to do, before it starts qualifying it.
fn first_sentence(about: &str) -> &str {
    let end = about
        .find(". ")
        .map(|at| at + 1)
        .unwrap_or(about.len())
        .min(about.find('\n').unwrap_or(about.len()));
    &about[..end]
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

/// What somebody can do about a server that did not start, in words.
///
/// The Tools panel said why in red and nothing else: "starting the peekaboo
/// server: No such file or directory", with nothing to say that the file is
/// Claude Code's settings, which entry, or that deleting it is a fine answer.
pub fn what_to_do(name: &str, trouble: &str) -> String {
    if trouble == NOT_ALLOWED_YET {
        return format!(
            "Errand starts a server for agents on other models only once you allow it, \
             because it runs as you, outside every wall. If \"{name}\" is yours and you want \
             these agents to use it, press Allow."
        );
    }
    if trouble == CHANGED_SINCE_ALLOWED {
        return format!(
            "What \"{name}\" runs changed since you allowed it: its command, arguments or \
             settings. If you changed it yourself, press Allow again. If you did not, look at \
             it in ~/.claude.json before you do."
        );
    }
    if trouble == IN_ITS_OWN_FOLDER {
        return "Errand starts only servers in your own list, at the top of ~/.claude.json, \
                and never one set up for an agent's folder, in its .mcp.json or under its \
                folder in ~/.claude.json: an agent could have put it there. If you want it, \
                add it to your own list and allow it here."
            .to_string();
    }
    let lower = trouble.to_lowercase();
    if lower.contains("not reached from here yet") {
        return "Agents on Claude Code reach this one themselves. Agents on any other model \
                cannot yet, because Errand only starts servers that run on this Mac. Nothing \
                needs doing unless one of those agents needs its tools."
            .to_string();
    }
    if lower.contains("no such file or directory") {
        return format!(
            "The program it starts is not on this Mac any more. It is set up under \"{name}\" \
             in ~/.claude.json, which is Claude Code's own settings file: reinstall that \
             program, or delete that entry if you no longer use it. Until then its tools are \
             missing for every agent."
        );
    }
    if lower.contains("permission denied") {
        return format!(
            "The program it starts is there but may not be run. Check the command under \
             \"{name}\" in ~/.claude.json, and that the file it names is executable."
        );
    }
    if lower.starts_with("it started but") {
        return "It started and then did not answer. It may still be installing itself, or be \
                waiting to be signed in: open this panel again in a minute, and if it keeps \
                saying this, run its command from ~/.claude.json in a terminal to see what it \
                says."
            .to_string();
    }
    format!(
        "Its tools are missing for every agent, which looks exactly like an agent choosing \
         not to use them. Check the command under \"{name}\" in ~/.claude.json still exists: \
         an interpreter inside a virtual environment stops existing when the one it was built \
         from is upgraded away."
    )
}

impl Servers {
    /// Start everything configured for this directory that the person allowed,
    /// and ask what it offers.
    ///
    /// One that fails to start does not stop the others. Most people have a
    /// server configured that they have not used for months.
    pub async fn open(cwd: &Path) -> Self {
        Self::open_these(configured(cwd), &allowed_now()).await
    }

    /// The same, for a given list against a given allowance.
    pub async fn open_these(wanted: Vec<Configured>, allowed: &HashMap<String, String>) -> Self {
        let mut servers = Servers::default();
        for want in wanted {
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
            let why_not = match standing(&want, allowed) {
                Standing::Allowed => None,
                Standing::NotYet => Some(NOT_ALLOWED_YET),
                Standing::Changed => Some(CHANGED_SINCE_ALLOWED),
                Standing::InItsFolder => Some(IN_ITS_OWN_FOLDER),
            };
            if let Some(why) = why_not {
                servers.trouble.push((want.name.clone(), why.to_string()));
                continue;
            }

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

    /// Add a tool without a server behind it, for tests that are about what is
    /// offered rather than about talking to anything.
    #[doc(hidden)]
    pub fn add_for_testing(&mut self, tool: Tool) {
        self.tools.push(tool);
    }

    /// Everything on offer.
    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// The tools that best match some words, most likely first.
    ///
    /// Deliberately crude: word overlap, weighted by where the word was found.
    /// A model asking for "take a screenshot" is not trying to defeat a search
    /// engine, it is naming the thing it wants, and the names were written by
    /// somebody hoping to be found.
    ///
    /// The weighting is the part that had to be learnt. A flat count put
    /// `click` above `see` for "take a screenshot", because `click`'s notes say
    /// "take a screenshot first" and `see`'s only say what it does. So what a
    /// tool claims in its first sentence counts for much more than what it
    /// mentions afterwards: the opening line is what a tool is for, and the
    /// rest is caveats.
    pub fn matching(&self, words: &str, most: usize) -> Vec<&Tool> {
        let words: Vec<String> = words
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(str::to_string)
            .collect();

        let mut scored: Vec<(usize, &Tool)> = self
            .tools
            .iter()
            .map(|tool| {
                let name = tool.own_name.to_lowercase();
                let about = tool.description.to_lowercase();
                let summary = first_sentence(&about);
                let score = words
                    .iter()
                    .map(
                        |w| match (name.contains(w), summary.contains(w), about.contains(w)) {
                            (true, _, _) => 6,
                            (_, true, _) => 3,
                            (_, _, true) => 1,
                            _ => 0,
                        },
                    )
                    .sum();
                (score, tool)
            })
            .filter(|(score, _)| *score > 0)
            .collect();

        // Ties broken by name, so the same words always bring back the same
        // tools in the same order. A search that shuffles is a search nobody
        // can debug.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.called.cmp(&b.1.called)));
        scored.into_iter().take(most).map(|(_, t)| t).collect()
    }

    /// Every tool there is, by name only, grouped by the server offering it.
    ///
    /// This is what a model is told up front instead of the schemas. Names and
    /// nothing else: for the servers on this machine that is a few hundred
    /// bytes against sixty-four thousand, and a name is enough to know that
    /// something exists and to go looking for it.
    /// What else is out there, as counts and the servers offering them.
    ///
    /// Counts and server names, and deliberately not tool names. That is not a
    /// space saving, it is the thing that makes this work at all.
    ///
    /// The first version listed all twenty-six names, on the reasoning that a
    /// name is enough to know something exists and to go looking for it.
    /// Against a 7B model it was worse than saying nothing: with the names in
    /// the prompt the model returned an empty response, no text and no tool
    /// call, every time; take them out and the same request produced a tool
    /// call immediately. Reproduced three times against controls, and the list
    /// was the only difference.
    ///
    /// The guess is that naming functions it cannot call is a worse position
    /// than not knowing they exist, and it stalls. The finding stands without
    /// the guess: say how much is out there and how to ask for it, never what
    /// any of it is called.
    pub fn what_else(&self) -> String {
        let mut by_server: Vec<(&str, usize)> = Vec::new();
        for tool in &self.tools {
            match by_server.iter_mut().find(|(name, _)| *name == tool.server) {
                Some((_, count)) => *count += 1,
                None => by_server.push((&tool.server, 1)),
            }
        }
        let where_from = by_server
            .iter()
            .map(|(server, count)| format!("{server} ({count})"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{} more from {where_from}", self.tools.len())
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
    #[test]
    fn a_server_that_did_not_start_says_what_to_do_about_it() {
        // The two the Tools panel showed in red on a real Mac, with nothing
        // to say whether or how to mend either.
        let gone = what_to_do(
            "peekaboo",
            "starting the peekaboo server: No such file or directory (os error 2)",
        );
        assert!(gone.contains("\"peekaboo\" in ~/.claude.json"), "{gone}");
        assert!(
            gone.contains("reinstall") && gone.contains("delete that entry"),
            "{gone}"
        );
        let remote = what_to_do("replit", "http servers are not reached from here yet");
        assert!(
            remote.contains("Agents on Claude Code reach this one"),
            "{remote}"
        );
        assert!(remote.contains("Nothing needs doing"), "{remote}");
        // And something nobody has seen yet still gets somewhere to look.
        let other = what_to_do("odd", "something else entirely");
        assert!(other.contains("\"odd\" in ~/.claude.json"), "{other}");
    }

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

    fn pretend(tools: &[(&str, &str, &str)]) -> Servers {
        Servers {
            tools: tools
                .iter()
                .map(|(server, name, about)| Tool {
                    called: format!("mcp__{server}__{name}"),
                    own_name: name.to_string(),
                    server: server.to_string(),
                    description: about.to_string(),
                    takes: json!({ "type": "object" }),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn asking_for_a_thing_by_name_finds_it_ahead_of_one_that_merely_mentions_it() {
        let servers = pretend(&[
            (
                "peekaboo",
                "see",
                "Captures a screenshot and maps the elements on it.",
            ),
            (
                "peekaboo",
                "click",
                "Clicks an element. Take a screenshot first.",
            ),
            ("notes", "add_note", "Write a note."),
        ]);
        let found = servers.matching("take a screenshot", 5);
        assert_eq!(found.len(), 2, "the note has nothing to do with it");
        assert_eq!(
            found[0].own_name, "see",
            "the one whose description is about screenshots, not the one that mentions them"
        );
    }

    #[test]
    fn the_same_words_always_bring_back_the_same_tools_in_the_same_order() {
        let servers = pretend(&[
            ("a", "window", "Manage windows."),
            ("b", "window", "Manage windows."),
        ]);
        let once: Vec<&str> = servers
            .matching("window", 5)
            .iter()
            .map(|t| t.called.as_str())
            .collect();
        let twice: Vec<&str> = servers
            .matching("window", 5)
            .iter()
            .map(|t| t.called.as_str())
            .collect();
        assert_eq!(
            once, twice,
            "a search that shuffles is one nobody can debug"
        );
    }

    #[test]
    fn words_too_short_to_mean_anything_do_not_match_everything() {
        let servers = pretend(&[("peekaboo", "see", "Captures a screenshot.")]);
        assert!(servers.matching("do it to me", 5).is_empty());
    }

    #[test]
    fn what_else_is_out_there_says_how_much_and_never_what_it_is_called() {
        // Not a style choice. With tool names in the prompt a 7B model returned
        // nothing at all, and without them the same request produced a tool
        // call straight away.
        let servers = pretend(&[
            ("peekaboo", "see", "Captures a screenshot."),
            ("peekaboo", "click", "Clicks something."),
            ("notes", "add_note", "Writes a note."),
        ]);
        let said = servers.what_else();
        assert_eq!(said, "3 more from peekaboo (2), notes (1)");
        assert!(!said.contains("see"), "no tool names, ever");
        assert!(!said.contains("add_note"));
    }

    /// A home folder of its own with a `.claude.json`, and an agent folder.
    fn laid_out(tag: &str, claude_json: Value, mcp_json: Option<Value>) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("errand-mcp-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let home = root.join("home");
        let cwd = root.join("agent");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::write(home.join(".claude.json"), claude_json.to_string()).unwrap();
        if let Some(planted) = mcp_json {
            std::fs::write(cwd.join(".mcp.json"), planted.to_string()).unwrap();
        }
        (home, cwd)
    }

    use std::path::PathBuf;

    #[test]
    fn only_the_persons_own_list_can_ever_be_started_and_a_folder_cannot_shadow_it() {
        let (home, cwd) = laid_out("whose", json!({}), None);
        let cwd_key = cwd.to_string_lossy().to_string();
        std::fs::write(
            home.join(".claude.json"),
            json!({
                "mcpServers": { "mine": { "command": "/usr/bin/true" } },
                "projects": { cwd_key: { "mcpServers": { "proj": { "command": "/usr/bin/true" } } } }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            cwd.join(".mcp.json"),
            json!({ "mcpServers": { "mine": { "command": "/usr/bin/false" }, "planted": { "command": "/usr/bin/true" } } })
                .to_string(),
        )
        .unwrap();
        let all = configured_in(&home, &cwd);
        let whose: Vec<(String, Found)> = all.iter().map(|c| (c.name.clone(), c.found)).collect();
        assert!(
            whose.contains(&("mine".into(), Found::TheirOwn)),
            "{whose:?}"
        );
        assert!(
            whose.contains(&("proj".into(), Found::ThisFolder)),
            "{whose:?}"
        );
        assert!(
            whose.contains(&("planted".into(), Found::McpJson)),
            "{whose:?}"
        );
        // The person's own `mine` is still there beside the folder's.
        assert_eq!(all.iter().filter(|c| c.name == "mine").count(), 2);
        // Allowing every fingerprint there is still starts only the person's
        // own: the folder's entries, its own `mine` included, never.
        let theirs = all
            .iter()
            .find(|c| c.name == "mine" && c.found == Found::TheirOwn)
            .unwrap();
        let mut allowed: HashMap<String, String> =
            [("mine".to_string(), theirs.fingerprint().unwrap())].into();
        for c in all
            .iter()
            .filter(|c| c.found != Found::TheirOwn && c.name != "mine")
        {
            allowed.insert(c.name.clone(), c.fingerprint().unwrap());
        }
        for c in &all {
            let expected = match c.found {
                Found::TheirOwn => Standing::Allowed,
                _ => Standing::InItsFolder,
            };
            assert_eq!(
                standing(c, &allowed),
                expected,
                "{} from {:?}",
                c.name,
                c.found
            );
        }
        std::fs::remove_dir_all(home.parent().unwrap()).ok();
    }

    #[test]
    fn allowing_a_server_covers_exactly_what_was_shown() {
        let one = |command: &str, args: &[&str], env: &[(&str, &str)]| Configured {
            name: "x".into(),
            how: How::Program {
                command: command.into(),
                args: args.iter().map(|a| a.to_string()).collect(),
                env: env
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
            from: "~/.claude.json".into(),
            found: Found::TheirOwn,
        };
        let base = one("/bin/srv", &["--a"], &[("K", "1"), ("J", "2")]);
        // The same entry, its variables in another order, is the same.
        assert_eq!(
            base.fingerprint(),
            one("/bin/srv", &["--a"], &[("J", "2"), ("K", "1")]).fingerprint()
        );
        // Any change to what runs is a different one.
        for changed in [
            one("/bin/other", &["--a"], &[("K", "1"), ("J", "2")]),
            one("/bin/srv", &["--b"], &[("K", "1"), ("J", "2")]),
            one("/bin/srv", &["--a"], &[("K", "9"), ("J", "2")]),
            one(
                "/bin/srv",
                &["--a"],
                &[("K", "1"), ("J", "2"), ("DYLD_INSERT_LIBRARIES", "/x")],
            ),
        ] {
            assert_ne!(base.fingerprint(), changed.fingerprint());
        }
        let allowed: HashMap<String, String> =
            [("x".to_string(), base.fingerprint().unwrap())].into();
        assert_eq!(standing(&base, &allowed), Standing::Allowed);
        assert_eq!(
            standing(
                &one("/bin/srv", &["--a"], &[("K", "9"), ("J", "2")]),
                &allowed
            ),
            Standing::Changed
        );
        assert_eq!(standing(&base, &HashMap::new()), Standing::NotYet);
        // A server on the network has nothing to fingerprint and is never allowed.
        let remote = Configured {
            name: "r".into(),
            how: How::Remote {
                kind: "http".into(),
                url: "https://example.com/mcp".into(),
            },
            from: "~/.claude.json".into(),
            found: Found::TheirOwn,
        };
        assert_eq!(remote.fingerprint(), None);
        // What the person is shown names variables and never their values.
        let shown = one("/bin/srv", &["--a"], &[("API_KEY", "sekrit-value")]).shown();
        assert!(
            shown.contains("/bin/srv --a") && shown.contains("API_KEY"),
            "{shown}"
        );
        assert!(!shown.contains("sekrit-value"), "{shown}");
    }

    #[test]
    fn allowing_refuses_a_server_that_changed_after_it_was_shown() {
        let (home, _cwd) = laid_out(
            "shown",
            json!({ "mcpServers": { "mine": { "command": "/bin/srv", "args": ["--a"] } } }),
            None,
        );
        let shown = configured_in(&home, Path::new("/x"))[0]
            .fingerprint()
            .unwrap();
        assert!(the_one_shown_in(&home, "mine", &shown).is_ok());
        std::fs::write(
            home.join(".claude.json"),
            json!({ "mcpServers": { "mine": { "command": "/bin/srv", "args": ["--evil"] } } })
                .to_string(),
        )
        .unwrap();
        assert!(the_one_shown_in(&home, "mine", &shown).is_err());
        assert!(the_one_shown_in(&home, "nobody", &shown).is_err());
        std::fs::remove_dir_all(home.parent().unwrap()).ok();
    }

    #[tokio::test]
    async fn a_server_a_teammate_wrote_into_its_folder_never_runs() {
        // Run rather than read: each command only leaves a mark, and which
        // marks exist afterwards is which servers this app started.
        let (home, cwd) = laid_out("run", json!({}), None);
        let mark = |n: &str| home.parent().unwrap().join(n);
        let touching =
            |n: &str| json!({ "command": "/usr/bin/touch", "args": [mark(n).to_string_lossy()] });
        let cwd_key = cwd.to_string_lossy().to_string();
        std::fs::write(
            home.join(".claude.json"),
            json!({
                "mcpServers": { "allowed": touching("m-allowed"), "unallowed": touching("m-unallowed") },
                "projects": { cwd_key: { "mcpServers": { "project": touching("m-project") } } }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            cwd.join(".mcp.json"),
            json!({ "mcpServers": { "planted": touching("m-planted"), "allowed": touching("m-shadow") } }).to_string(),
        )
        .unwrap();
        let all = configured_in(&home, &cwd);
        let theirs = all
            .iter()
            .find(|c| c.name == "allowed" && c.found == Found::TheirOwn)
            .unwrap()
            .fingerprint()
            .unwrap();
        let allowed: HashMap<String, String> = [("allowed".to_string(), theirs)].into();
        let servers = Servers::open_these(all, &allowed).await;
        assert!(mark("m-allowed").exists(), "the person's allowed one runs");
        for never in ["m-unallowed", "m-project", "m-planted", "m-shadow"] {
            assert!(!mark(never).exists(), "{never} must never run");
        }
        assert!(servers
            .trouble
            .iter()
            .any(|(n, why)| n == "planted" && why == IN_ITS_OWN_FOLDER));
        assert!(servers
            .trouble
            .iter()
            .any(|(n, why)| n == "unallowed" && why == NOT_ALLOWED_YET));
        std::fs::remove_dir_all(home.parent().unwrap()).ok();
    }

    #[test]
    fn a_server_that_was_not_started_says_how_to_allow_it() {
        assert!(what_to_do("m", NOT_ALLOWED_YET).contains("press Allow"));
        assert!(what_to_do("m", CHANGED_SINCE_ALLOWED).contains("changed since you allowed it"));
        assert!(
            what_to_do("m", IN_ITS_OWN_FOLDER).contains("never one set up for an agent's folder")
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
