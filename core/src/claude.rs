//! Claude Code, driven as a conversation rather than as a command.
//!
//! Claude Code is already the thing this app wants to be underneath: an agent
//! loop with tools, permissions, MCP, skills and subagents, signed in to the
//! person's own account. Rebuilding that would be a year of work and a second
//! thing to keep correct. So it is driven the way Grok Bot drives its own
//! agent, as a long-lived process on a pipe.
//!
//! The flags matter and were chosen by watching what they actually do:
//!
//! - `--input-format stream-json` is the one everything rests on. It accepts a
//!   new message while the process is alive, so a person can change their mind
//!   halfway through without starting again. The message is taken at the next
//!   turn boundary, not inside the tool in flight, which is worth saying out
//!   loud because it is what somebody watching will see.
//! - `--output-format stream-json` with `--verbose` is what makes the work
//!   visible: every tool call and result arrives as an event rather than as a
//!   paragraph at the end.
//! - `--include-partial-messages` streams the prose as it is written. A line
//!   that appears a word at a time reads as thinking; the same line arriving
//!   whole two seconds later reads as a hang.
//! - `--session-id` ties the process to the thread, so a thread that is closed
//!   and reopened is the same conversation to Claude as it is to the person.
//! - `--permission-mode acceptEdits` was arrived at by elimination, and the
//!   flag next to it is worth naming so nobody reaches for it again. The
//!   obvious-looking one is `dontAsk`, which is what Claude Code's own
//!   scheduled tasks use, and it is a trap here: it does not mean "do not
//!   interrupt anybody", it means "answer every prompt with no". Under it the
//!   morning-news errand tried the web five different ways, was refused five
//!   times, and reported a wall. The default mode already runs the ordinary
//!   safe things unasked. `acceptEdits` adds the one thing an errand cannot
//!   avoid, which is writing its own work down, and stops short of
//!   `bypassPermissions`, so what remains behind a prompt stays behind it.
//! - `--allowedTools` grants the looking-things-up tools up front, for the
//!   reason above: a prompt nobody can answer is a refusal. Everything on that
//!   list only reads. Bash is deliberately not on it, so an ordinary command
//!   still runs and a destructive one still stops, and `--disallowedTools` is
//!   not used at all, since this list adds permissions rather than being the
//!   whole of them.
//! - `--append-system-prompt` carries ERRAND MODE, below.
//!
//! What comes back is a stream of JSON objects, captured from a real run rather
//! than from the documentation: `system` with subtypes (`init` carries the
//! session and the model), `assistant` whose content blocks are `text` or
//! `tool_use`, `user` carrying `tool_result`, and one `result` per turn.
//! Anything else -- hooks firing, rate limit notices, post-turn summaries --
//! is deliberately dropped rather than shown, because a person watching their
//! own errand does not need to watch the machinery underneath it.

use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::engine::{Engine, Event, Step};

/// What turns a helpful assistant into somebody running an errand.
///
/// Without this the app fails in one particular and infuriating way: asked for
/// the morning's Bitcoin news it answers with three numbered options and asks
/// which one you want, having done nothing. That is the correct behaviour for
/// an assistant sitting in a terminal beside somebody, and the wrong behaviour
/// entirely for a thing you hand a job to and walk away from.
///
/// The text is appended rather than replacing the default, so everything Claude
/// Code already knows about its own tools and this machine survives. What it
/// adds is four things, in this order because they undo each other otherwise:
/// act before speaking, treat a dead route as the next route rather than the
/// end, keep the safeguards while doing it, and come back with the result or
/// with one answerable question, never with a menu.
const ERRAND_MODE: &str = include_str!("errand.md");

/// The tools an errand may reach for without anybody being asked first.
///
/// Every one of them only looks: it reads a file, a page, or a search result,
/// and changes nothing. The two that are not on the list are the point of the
/// list. Bash is left off because it is not one tool but every tool, and the
/// classifier underneath already lets an ordinary command through while
/// stopping a destructive one. Anything that spends money, sends a message or
/// signs in is not here and is not meant to be: those are the walls the agent
/// is told to stop at and name, and it does.
const GRANTED: &[&str] = &[
    "WebSearch",
    "WebFetch",
    "Read",
    "Glob",
    "Grep",
    "TodoWrite",
    "Task",
    "Skill",
    "ToolSearch",
];

/// A thread's conversation with Claude Code.
///
/// Wants a runtime with more than one thread, or a caller that never blocks the
/// one it has. Reading the process and answering it are tasks, and a caller
/// that blocks a current-thread runtime waiting for an event stops the very
/// task that would produce it. The symptom is silence, which reads exactly like
/// an agent with nothing to say.
///
/// Nothing here holds the pipe. One task owns the process and its stdin, and
/// everything else posts to it, which is what makes `say` safe to call from a
/// window while the agent is mid-step: the caller is never waiting on a write
/// to a program that is busy thinking.
pub struct Claude {
    turns: tokio::sync::mpsc::UnboundedSender<Turn>,
}

/// Something to do to the conversation.
enum Turn {
    Say(String),
    Stop,
}

impl Claude {
    /// Start a conversation, or pick up the one this thread already had.
    ///
    /// `again` is the whole of the difference and it is not a detail. A session
    /// is started with `--session-id` exactly once; every reopening after that
    /// is `--resume`, and the two are not interchangeable:
    ///
    /// - `--session-id` on an id that already has a transcript fails outright,
    ///   and fails in the worst possible shape: exit 1, one line on stderr, and
    ///   *nothing at all on stdout*. A reader waiting for the usual opening
    ///   event waits for ever. That is why stderr and the exit code are watched
    ///   here rather than trusted to be quiet.
    /// - `--resume` on an id with no transcript fails politely, saying so on
    ///   stdout as well, so that one is visible without watching anything.
    /// - Both together is refused at launch unless the intent is to fork.
    ///
    /// All of it is scoped to the working directory. The same id resumed from
    /// somewhere else finds no conversation; started from somewhere else with
    /// `--session-id`, it quietly begins an empty one under the same name,
    /// which is a lie nobody would catch. So the directory is the thread's, is
    /// stored with it, and is passed back in unchanged.
    pub fn open(
        session: &str,
        cwd: &std::path::Path,
        again: bool,
    ) -> Result<(Self, Receiver<Event>)> {
        let pick_up = if again { "--resume" } else { "--session-id" };
        let mut child = tokio::process::Command::new("claude")
            .args([
                "--print",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--verbose",
                "--permission-mode",
                "acceptEdits",
                "--allowedTools",
            ])
            .args(GRANTED)
            .args(["--append-system-prompt", ERRAND_MODE, pick_up, session])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("starting claude; is Claude Code installed and on the PATH?")?;

        let mut stdin = child.stdin.take().context("claude stdin")?;
        let stdout = child.stdout.take().context("claude stdout")?;
        let stderr = child.stderr.take().context("claude stderr")?;
        let (tx, rx) = channel();

        let handle = tokio::runtime::Handle::current();

        // Whatever it complains about, kept. Most of the time it complains
        // about nothing; the one failure that matters says everything here and
        // nothing on stdout, so this is the only place it can be found.
        let complaints: Arc<Mutex<String>> = Arc::default();
        let heard = complaints.clone();
        let also_heard = complaints.clone();
        handle.spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut said = heard.lock().unwrap();
                said.push_str(line.trim());
                said.push(' ');
            }
        });

        let ended = tx.clone();
        handle.spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut said_anything = false;
            while let Ok(Some(line)) = lines.next_line().await {
                for event in read(&line) {
                    said_anything = true;
                    // A failure that says nothing is worse than no failure at
                    // all: the thread stops, and the person is told that it
                    // stopped, and nothing else. It happens for real -- asking
                    // to reopen a conversation that is not there ends the turn
                    // with an error carrying no words, and the reason is on the
                    // other pipe.
                    let event = match event {
                        Event::Failed { why } if why.trim().is_empty() => Event::Failed {
                            why: in_words(&complaints.lock().unwrap()),
                        },
                        other => other,
                    };
                    if ended.send(event).is_err() {
                        return; // Nobody is listening any more.
                    }
                }
            }
            // Stdout has closed. If it closed without a single word, the
            // process refused to start, and the reason is on the other pipe.
            // Left alone, this is a thread that sits there looking like it is
            // thinking, for ever.
            if !said_anything {
                let _ = ended.send(Event::Failed {
                    why: in_words(&also_heard.lock().unwrap()),
                });
            }
        });

        let (turns, mut asked) = tokio::sync::mpsc::unbounded_channel();
        handle.spawn(async move {
            while let Some(turn) = asked.recv().await {
                match turn {
                    Turn::Say(line) => {
                        if stdin.write_all(line.as_bytes()).await.is_err() {
                            break; // It has gone; the event stream will say so.
                        }
                    }
                    Turn::Stop => break,
                }
            }
            let _ = child.kill().await;
        });

        Ok((Self { turns }, rx))
    }
}

impl Engine for Claude {
    fn say(&mut self, text: &str) -> Result<()> {
        let line = format!(
            "{}\n",
            serde_json::json!({
                "type": "user",
                "message": { "role": "user", "content": [{ "type": "text", "text": text }] },
            })
        );
        self.turns
            .send(Turn::Say(line))
            .map_err(|_| anyhow::anyhow!("this conversation has ended"))
    }

    fn stop(&mut self) -> Result<()> {
        // It may already be gone, which is not a failure to stop it.
        let _ = self.turns.send(Turn::Stop);
        Ok(())
    }
}

/// Turn one line of Claude Code's output into what the window understands.
///
/// A line can be nothing at all, and most are: the hooks, the rate limit
/// notices, the post-turn summaries. Returning a list rather than an option is
/// because one assistant message can carry several things worth showing --
/// a sentence and then the tool it decided on.
pub fn read(line: &str) -> Vec<Event> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![];
    };
    let at = |k: &str| v.get(k).and_then(|s| s.as_str()).unwrap_or("").to_string();

    match at("type").as_str() {
        "system" if at("subtype") == "init" => vec![Event::Started {
            session: at("session_id"),
            model: at("model"),
        }],

        "assistant" => v
            .pointer("/message/content")
            .and_then(|c| c.as_array())
            .map(|blocks| blocks.iter().filter_map(block).collect())
            .unwrap_or_default(),

        // A tool has answered. What it said is the agent's business; that it
        // answered at all is the person's, because it is how a step stops
        // looking like a hang.
        "user" => v
            .pointer("/message/content")
            .and_then(|c| c.as_array())
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
                    .map(|b| Event::Did {
                        call: b
                            .get("tool_use_id")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string(),
                        outcome: one_line(&b.get("content").map(said).unwrap_or_default()),
                    })
                    .collect()
            })
            .unwrap_or_default(),

        "result" => {
            let said = v
                .get("result")
                .map(said)
                .unwrap_or_default()
                .trim()
                .to_string();
            if v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false) {
                vec![Event::Failed { why: said }]
            } else {
                vec![Event::Done { said }]
            }
        }

        _ => vec![],
    }
}

/// What the agent complained about, said the way a person would say it.
///
/// The two failures worth knowing by name are the ones that follow from getting
/// the reopening flags wrong, and both are said here in words rather than
/// passed through as they arrive. Everything else is passed through: it is
/// still better than silence, and inventing a sentence for a failure nobody has
/// seen yet is how a program ends up explaining the wrong thing confidently.
fn in_words(complained: &str) -> String {
    let said = complained.trim();
    if said.contains("No conversation found") {
        return "This conversation could not be found where it was left. Its history is gone, \
                though anything it did is not."
            .to_string();
    }
    if said.contains("is already in use") {
        return "This conversation was opened as though it were new when it already existed. \
                That is a fault in Errand rather than anything you did."
            .to_string();
    }
    if said.is_empty() {
        return "The agent stopped without saying why.".to_string();
    }
    said.to_string()
}

/// One content block, if it is something a person should see.
fn block(b: &serde_json::Value) -> Option<Event> {
    match b.get("type").and_then(|t| t.as_str())? {
        "text" => {
            let text = b.get("text")?.as_str()?.trim().to_string();
            (!text.is_empty()).then_some(Event::Said {
                text,
                settled: true,
            })
        }
        "tool_use" => {
            let tool = b.get("name")?.as_str()?.to_string();
            Some(Event::Doing(Step {
                what: in_plain_words(&tool, b.get("input")),
                tool,
                // Every tool_use block carries one, and the tool_result that
                // answers it carries the same string back as `tool_use_id`.
                call: b.get("id")?.as_str()?.to_string(),
            }))
        }
        _ => None,
    }
}

/// A tool call, said the way the person whose errand it is would say it.
///
/// The alternative is showing the tool's own name and arguments, which is
/// honest and tells nobody anything: "Bash: osascript -e 'tell application
/// \"Mail\"...'" is not what somebody wants to read about their own post. Where
/// a tool carries its own description, that is used, because whatever wrote it
/// knew what it was for. Otherwise the name is turned into something plain.
fn in_plain_words(tool: &str, input: Option<&serde_json::Value>) -> String {
    if let Some(said) = input
        .and_then(|i| i.get("description"))
        .and_then(|d| d.as_str())
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        return said.to_string();
    }
    let target = |k: &str| {
        input
            .and_then(|i| i.get(k))
            .and_then(|v| v.as_str())
            .map(one_line)
    };
    match tool {
        "Bash" => match target("command") {
            Some(c) => format!("Running {c}"),
            None => "Running a command".into(),
        },
        "Read" => match target("file_path") {
            Some(p) => format!("Reading {p}"),
            None => "Reading a file".into(),
        },
        "Write" | "Edit" => match target("file_path") {
            Some(p) => format!("Writing {p}"),
            None => "Writing a file".into(),
        },
        "WebFetch" | "WebSearch" => "Looking something up on the web".into(),
        "Task" => "Handing part of this to a helper".into(),
        other => format!("Using {other}"),
    }
}

/// Content that may be a string or a list of blocks, as one string.
fn said(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

/// One line of it, short enough to sit in a timeline.
fn one_line(s: &str) -> String {
    let line = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.chars().count() > 80 {
        let short: String = line.chars().take(79).collect();
        format!("{short}…")
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines captured from a real run rather than written from memory, which is
    /// the only reason to trust any of this.
    const INIT: &str = r#"{"type":"system","subtype":"init","session_id":"7ee4de55-1111","model":"claude-opus-5"}"#;
    const TEXT: &str = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"I'll run that command."}]}}"#;
    const TOOL: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_01","name":"Bash","input":{"command":"echo hello","description":"Echo a greeting"}}]}}"#;
    const RESULT: &str = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"hello-from-a-tool"}]}}"#;
    const DONE: &str = r#"{"type":"result","is_error":false,"result":"Output: hello","total_cost_usd":0.18,"num_turns":2}"#;
    const HOOK: &str = r#"{"type":"system","subtype":"hook_started","session_id":"7ee4de55-1111"}"#;

    #[test]
    fn the_start_of_a_thread_says_what_is_answering_it() {
        assert_eq!(
            read(INIT),
            vec![Event::Started {
                session: "7ee4de55-1111".into(),
                model: "claude-opus-5".into(),
            }]
        );
    }

    #[test]
    fn prose_arrives_as_something_to_read_and_a_tool_as_something_being_done() {
        assert_eq!(
            read(TEXT),
            vec![Event::Said {
                text: "I'll run that command.".into(),
                settled: true
            }]
        );
        // The tool's own description wins, because whatever wrote it knew what
        // the call was for better than a rule here ever will.
        assert_eq!(
            read(TOOL),
            vec![Event::Doing(Step {
                what: "Echo a greeting".into(),
                tool: "Bash".into(),
                call: "toolu_01".into(),
            })]
        );
    }

    #[test]
    fn a_tool_with_nothing_to_say_for_itself_is_still_said_in_plain_words() {
        let bare = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_02","name":"Bash","input":{"command":"ls -la /tmp"}}]}}"#;
        let Event::Doing(step) = &read(bare)[0] else {
            panic!("a tool call is a step");
        };
        assert_eq!(step.what, "Running ls -la /tmp");
        assert_eq!(step.tool, "Bash", "the real name is kept for the timeline");

        // And one nobody has taught it about is named rather than hidden.
        let odd = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_03","name":"NotebookEdit","input":{}}]}}"#;
        let Event::Doing(step) = &read(odd)[0] else {
            panic!("still a step");
        };
        assert_eq!(step.what, "Using NotebookEdit");
    }

    #[test]
    fn a_tool_answering_is_shown_because_that_is_how_a_step_stops_looking_like_a_hang() {
        assert_eq!(
            read(RESULT),
            vec![Event::Did {
                call: "t1".into(),
                outcome: "hello-from-a-tool".into()
            }]
        );
    }

    #[test]
    fn the_end_of_a_turn_is_an_ending_or_a_failure_and_never_both() {
        assert_eq!(
            read(DONE),
            vec![Event::Done {
                said: "Output: hello".into()
            }]
        );
        let bad = r#"{"type":"result","is_error":true,"result":"it could not be done"}"#;
        assert_eq!(
            read(bad),
            vec![Event::Failed {
                why: "it could not be done".into()
            }]
        );
        assert!(read(DONE)[0].ends_the_turn());
    }

    #[test]
    fn the_machinery_underneath_is_not_shown_to_anybody() {
        // Hooks firing, rate limit notices, post-turn summaries. All true, none
        // of it anybody's errand.
        for line in [
            HOOK,
            r#"{"type":"rate_limit_event"}"#,
            r#"{"type":"system","subtype":"post_turn_summary"}"#,
            "not json at all",
            "",
        ] {
            assert!(read(line).is_empty(), "showed the machinery: {line}");
        }
    }

    #[test]
    fn a_long_tool_result_is_cut_to_something_a_timeline_can_hold() {
        let long = "x".repeat(500);
        let line = format!(
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"t1","content":"{long}"}}]}}}}"#
        );
        let Event::Did { outcome, .. } = &read(&line)[0] else {
            panic!("a result");
        };
        assert_eq!(outcome.chars().count(), 80, "79 and the mark that says so");
        assert!(outcome.ends_with('…'));
    }
}
