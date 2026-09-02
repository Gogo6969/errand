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
//! - `--permission-mode` is the agent's now rather than one decision for the
//!   whole app: `ask` is the default and asks about everything not granted
//!   below, `edits` adds file writing, `auto` is `bypassPermissions` and is
//!   the one somebody has to choose deliberately. What follows is why
//!   `acceptEdits` was the compiled-in default before there was a choice.
//!
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
//! - `--permission-prompt-tool stdio` is the one that makes a person part of
//!   this. It is not in the help text and the literal `stdio` is the whole
//!   trick: with it, and with the handshake below, a step that needs
//!   permission is asked about on the same pipe as everything else instead of
//!   being refused before anybody hears about it. Found by watching what the
//!   official SDK puts on its own command line, and then confirmed by running
//!   it: the same request that was silently denied ran, and came back with the
//!   page it fetched.
//!
//! What comes back is a stream of JSON objects, captured from a real run rather
//! than from the documentation: `system` with subtypes (`init` carries the
//! session and the model), `assistant` whose content blocks are `text` or
//! `tool_use`, `user` carrying `tool_result`, and one `result` per turn.
//! Anything else -- hooks firing, rate limit notices, post-turn summaries --
//! is deliberately dropped rather than shown, because a person watching their
//! own errand does not need to watch the machinery underneath it.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::engine::{Answer, Engine, Event, NeedsYou, Step};
use crate::team;

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

/// The first thing said on the pipe, before anything is asked of it.
///
/// Without this the agent treats the other end as a script rather than as
/// somebody who can answer, and the permission flag above does nothing. It
/// carries no settings of its own; it exists to say that there is a window
/// here and a person in front of it.
const HELLO: &str =
    r#"{"type":"control_request","request_id":"errand-hello","request":{"subtype":"initialize"}}"#;

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
    "Agent",
    "Skill",
    "ToolSearch",
    // Looking at who else there is changes nothing, which is the same reason
    // every other name on this list is here. Handing work to one of them is
    // deliberately not beside it: `team::asks_first` says that one is worth
    // stopping for, and leaving it off is how that gets honoured without a
    // second rule saying the same thing.
    "mcp__errand__who_else",
    // The three that write, and the only three here that do. What this list
    // actually holds is not "tools that read" but "tools that cannot reach
    // outside this app": these reach one agent's own notebook and nothing else.
    // They spend nothing, send nothing and sign in to nothing, and every call
    // leaves a line in the conversation saying what was noted.
    //
    // A card on any of them would be worse than useless, for the same reason
    // the permission mode above is never `dontAsk`. A note is written
    // mid-errand, and half of these errands run at seven in the morning with
    // nobody at the window: the card goes unanswered, and an unanswered card is
    // a refusal. The symptom is an agent that quietly learns nothing, which is
    // exactly what a broken notebook looks like from outside.
    "mcp__errand__remember",
    "mcp__errand__recall",
    "mcp__errand__forget",
    // The two standing jobs, for the same reason and one more. Setting one
    // happens in the middle of the conversation that asked for it, so somebody
    // is there; and unlike a note, what it sets is visible afterwards in a
    // panel of its own, says when it will next run, and stops with one press.
    // A card asking permission to write down the thing somebody just asked for
    // out loud is a question about their own sentence.
    "mcp__errand__every_day",
    "mcp__errand__keep_an_eye_on",
    // Asking somebody to come and do something is the one tool here that is
    // entirely a question. Putting a permission card in front of it would be
    // asking whether it may ask.
    "mcp__errand__over_to_you",
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
    /// Questions asked and not yet answered, by the id they came with.
    ///
    /// Kept because answering needs more than the answer: the agent wants its
    /// own input handed back, and a remembered yes needs the rule it suggested.
    /// Both arrive with the question and neither is worth making the window
    /// carry back and forth.
    waiting: Waiting,
    /// What it said it had, when it started.
    ///
    /// Filled by the task reading its output, because that is the only place
    /// the line goes past. Read by the window when somebody asks what this
    /// agent can reach.
    brought: Arc<Mutex<crate::Brought>>,
}

/// The questions in flight, shared between the task reading them and the
/// caller answering them.
type Waiting = Arc<Mutex<HashMap<String, serde_json::Value>>>;

/// Something to do to the conversation.
enum Turn {
    /// A line to write, whether that is something said or something answered.
    /// They are the same to the pipe and the order between them matters, which
    /// is the reason they are not two channels.
    Say(String),
    Stop,
}

/// How the next process picks a conversation up.
///
/// A bool could say two of these, and the third is the one that goes badly
/// when it is guessed wrong: `--session-id` on an id that already has a
/// transcript fails with exit 1 and nothing at all on stdout, so a reader
/// waiting for the usual opening event waits for ever.
#[derive(Debug, Clone, Copy)]
pub enum PickUp<'a> {
    /// Nothing has ever run here.
    New,
    /// It has run here before.
    Again,
    /// It carries on from another conversation, and has not run yet.
    ///
    /// Claude Code is asked to resume that one and fork it under the id we
    /// chose. Verified that it honours our id rather than inventing one, which
    /// is what lets a conversation's id stay the engine's session id.
    From(&'a str),
}

/// The models Claude Code will answer as, by alias and by the name to show.
///
/// Aliases rather than dated ids, and that is the whole reason this is a list
/// here instead of whatever somebody types. A dated id retires, and the way
/// that is discovered is a routine at seven in the morning failing on a name
/// that worked yesterday. An alias is the CLI's standing promise to mean the
/// latest of its kind.
///
/// Fixed rather than asked for, because there is nothing to ask: the CLI takes
/// any string and accepts a misspelling without complaint, failing later and
/// somewhere less obvious. A short list somebody chooses from cannot be
/// misspelled.
pub const MODELS: &[(&str, &str)] = &[
    ("opus", "Opus"),
    ("sonnet", "Sonnet"),
    ("haiku", "Haiku"),
    ("fable", "Fable"),
];

/// Has this conversation already got a session on disk?
///
/// Asked rather than remembered, because the flag in the store and the file on
/// disk can disagree and only one of them decides whether `--session-id` or
/// `--resume` is the right thing to say. Changing an agent's engine sets the
/// flag back to nothing, quite correctly -- the new engine has not had this
/// conversation -- but it cannot delete a transcript Claude Code wrote, so an
/// agent moved to a local model and back was then started as new against a
/// session that already existed. That fails with exit 1, one line on stderr and
/// nothing on stdout, every time, for ever. The agent is simply dead, and
/// nothing in the window can say why.
///
/// Claude Code files a transcript under a flattening of the working directory,
/// which is why the directory has to be the same one it was started in.
/// The engine's own word for a posture.
///
/// Named here rather than written inline where it is used, because it goes on
/// the command line every time and a command-line argument outranks the same
/// setting in a settings file. Anything describing what the engine will
/// actually do has to know this, or it is describing a file that is overruled.
pub fn the_mode_for(asks: &str) -> &'static str {
    match asks {
        "auto" => "bypassPermissions",
        "edits" => "acceptEdits",
        // Work the job out and come back with the plan, having changed nothing.
        // The posture somebody wants for an errand whose shape they are not
        // sure of yet: it reads, it looks things up, and then it says what it
        // would do, which is a thing you can argue with before it happens
        // rather than after.
        "plan" => "plan",
        _ => "default",
    }
}

pub fn already_going(session: &str, cwd: &std::path::Path) -> bool {
    let Ok(home) = std::env::var("HOME") else {
        return false;
    };
    let flattened: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| match c {
            '/' | '.' | ' ' => '-',
            other => other,
        })
        .collect();
    std::path::Path::new(&home)
        .join(".claude/projects")
        .join(flattened)
        .join(format!("{session}.jsonl"))
        .exists()
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
        pick_up: PickUp<'_>,
        asks: &str,
        doorway: Option<&std::path::Path>,
        model: Option<&str>,
        // What this agent has already been told about its job, if anything.
        remembers: &str,
    ) -> Result<(Self, Receiver<Event>)> {
        // Three shapes, and the third is why this is not a bool. Forking asks
        // Claude Code to read one conversation and continue it under a
        // different name, which needs both ids on the command line at once.
        let opening: Vec<String> = match pick_up {
            PickUp::New => vec!["--session-id".into(), session.to_string()],
            PickUp::Again => vec!["--resume".into(), session.to_string()],
            PickUp::From(parent) => vec![
                "--resume".into(),
                parent.to_string(),
                "--fork-session".into(),
                "--session-id".into(),
                session.to_string(),
            ],
        };

        // In front of the model from the first word, rather than waiting to be
        // searched for. An agent that has to think to go looking for what it
        // was told is an agent that mostly will not, and the whole point of a
        // standing job is that it does not have to be told twice.
        //
        // Owned, because it has to outlive the command builder that borrows it.
        let steering = match remembers.trim().is_empty() {
            true => format!("{ERRAND_MODE}\n\n{}", crate::memory::HOW_TO_USE_IT),
            false => format!(
                "{ERRAND_MODE}\n\n{}\n\n{remembers}",
                crate::memory::HOW_TO_USE_IT
            ),
        };

        // Where to reach this app's own two tools, if this conversation has a
        // doorway open. Passed on the command line rather than written into a
        // file, so there is nothing to leave behind and nothing for the local
        // engine to find and take a second route through.
        let reach_us = doorway.and_then(|socket| {
            let program = std::env::current_exe().ok()?;
            Some(crate::doorway::config(&program, socket))
        });

        // Walled in exactly when nobody is going to be asked.
        //
        // Asking is the better mechanism while it is switched on: it explains
        // itself, it happens per action, and it lets somebody say yes to the
        // thing they actually wanted. A wall explains nothing and refuses in
        // the same words whatever the reason. So on an agent set to ask, the
        // asking is the wall; on one set to get on with it without asking,
        // there is nothing else left, and this is what stands in its place.
        //
        // It costs something even here: everything Claude Code starts is inside
        // it too, MCP servers and hooks included, so the profile has to make
        // room for the directories those write to. That list is in one place
        // rather than two, because a second copy of it went stale.
        let walled = asks == "auto" && crate::wall::possible();
        let mut child = match walled {
            true => crate::wall::around("claude", cwd),
            false => tokio::process::Command::new("claude"),
        }
        .args([
            "--print",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--verbose",
            // What a helper says, so that handing part of an errand to one
            // is something you can watch rather than a step that sits
            // there. It arrives tagged with the step that started it, and
            // is shown underneath that step.
            "--forward-subagent-text",
            "--permission-mode",
            // The agent's, not one decision for the whole app. A research
            // agent and one that edits your files do not deserve the same
            // posture, and it was compiled in until now.
            the_mode_for(asks),
            "--allowedTools",
        ])
        .args(GRANTED)
        .args([
            "--permission-prompt-tool",
            "stdio",
            "--append-system-prompt",
            &steering,
        ])
        .args(&opening)
        // Not `--strict-mcp-config`, which would silently switch off every
        // server the person has set up for Claude Code. Ours is added to
        // theirs, the way anybody would expect.
        .args(match &reach_us {
            Some(config) => vec!["--mcp-config", config.as_str()],
            None => vec![],
        })
        // Nothing chosen means whatever this person's Claude Code is set
        // to, which is the right default: it is their CLI and their
        // account, and overruling it from here would be a surprise.
        .args(match model {
            Some(named) => vec!["--model", named],
            None => vec![],
        })
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("starting claude; is Claude Code installed and on the PATH?")?;
        let waiting: Waiting = Arc::default();
        let brought: Arc<Mutex<crate::Brought>> = Arc::default();
        let turning_up = brought.clone();

        let mut stdin = child.stdin.take().context("claude stdin")?;
        let stdout = child.stdout.take().context("claude stdout")?;
        let stderr = child.stderr.take().context("claude stderr")?;
        let (tx, rx) = channel();

        let handle = tokio::runtime::Handle::current();

        // Whatever it complains about, kept. Most of the time it complains
        // about nothing; the one failure that matters says everything here and
        // nothing on stdout, so this is the only place it can be found.
        // Still wanted, until somebody says otherwise. Read by the stdout
        // watcher to tell a process that refused to start from one that was
        // asked to leave, which are indistinguishable from the pipe's end.
        let still_wanted = Arc::new(std::sync::atomic::AtomicBool::new(true));

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
        let asked = waiting.clone();
        let watching_for_the_end = still_wanted.clone();
        handle.spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut said_anything = false;
            while let Ok(Some(line)) = lines.next_line().await {
                // What it turned up with, said once at the start. Kept rather
                // than passed on, because it is a fact about what is answering
                // and not something that happened: the window asks for it when
                // somebody opens the panel, which is long after this line went
                // by.
                if let Some(kit) = what_it_brought(&line) {
                    *turning_up.lock().unwrap() = kit;
                }
                // A question is the one thing that has to be kept rather than
                // only passed on: answering it needs what it arrived with.
                if let Some((id, request)) = a_question(&line) {
                    asked.lock().unwrap().insert(id, request);
                }
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
            //
            // Unless it was stopped on purpose, which looks identical from
            // here and is not a failure at all. Switching a thread to another
            // engine used to leave a red line in it saying the agent had
            // stopped without saying why, which was true and completely
            // misleading.
            if !said_anything && watching_for_the_end.load(std::sync::atomic::Ordering::SeqCst) {
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
            still_wanted.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = child.kill().await;
        });

        // Said before anything else, so the agent knows there is somebody here.
        let _ = turns.send(Turn::Say(format!("{HELLO}\n")));
        Ok((
            Self {
                turns,
                waiting,
                brought,
            },
            rx,
        ))
    }
}

impl Engine for Claude {
    fn say(&mut self, text: &str, pictures: &[crate::Picture]) -> Result<()> {
        // Pictures first, then the words. Both engines and every model behind
        // them read a question about an image better when the image is already
        // above it, and putting the text first makes "what is in this" a
        // question about nothing yet.
        let mut content: Vec<serde_json::Value> = pictures
            .iter()
            .map(|one| {
                serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": one.kind,
                        "data": one.base64,
                    },
                })
            })
            .collect();
        content.push(serde_json::json!({ "type": "text", "text": text }));

        let line = format!(
            "{}\n",
            serde_json::json!({
                "type": "user",
                "message": { "role": "user", "content": content },
            })
        );
        self.turns
            .send(Turn::Say(line))
            .map_err(|_| anyhow::anyhow!("this conversation has ended"))
    }

    /// Answer a question, and let the halted step go ahead or not.
    ///
    /// The agent is given back its own input rather than anything of ours: it
    /// asked about a specific command and it must run that command, not one
    /// this end reconstructed. A remembered yes carries the rule the agent
    /// itself suggested, for the same reason.
    fn answer(&mut self, call: &str, said: Answer) -> Result<()> {
        let asked = self.waiting.lock().unwrap().remove(call);
        let asked = asked.unwrap_or_default();
        let reply = match said {
            Answer::No => serde_json::json!({
                "behavior": "deny",
                // Said to the agent, not to the person. It reads this and
                // decides what to do next, so it is worth saying that the
                // route is closed rather than that something went wrong.
                "message": "Not this one. Find another way or say what you need.",
            }),
            // Always is a plain yes on the wire. The remembering is the app's,
            // because a rule filed in Claude Code's own settings is one this
            // app can neither show you nor take back -- and an allowlist you
            // cannot read is not a boundary.
            Answer::Yes | Answer::Always => serde_json::json!({
                "behavior": "allow",
                "updatedInput": asked.get("input").cloned().unwrap_or_default(),
            }),
        };
        let line = format!(
            "{}\n",
            serde_json::json!({
                "type": "control_response",
                "response": { "subtype": "success", "request_id": call, "response": reply },
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

    fn brought(&self) -> crate::Brought {
        self.brought.lock().unwrap().clone()
    }
}

/// A question, if this line is one, as its id and everything it came with.
///
/// What Claude Code says it has, from the line where it says it.
///
/// Read off the same stream everything else comes from rather than asked for
/// separately, because asking means starting a second process and waiting for
/// it to say the one thing this one already said.
fn what_it_brought(line: &str) -> Option<crate::Brought> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "system" || v.get("subtype")?.as_str()? != "init" {
        return None;
    }
    let names = |which: &str| -> Vec<String> {
        v.get(which)
            .and_then(|list| list.as_array())
            .map(|list| {
                list.iter()
                    // Plugins arrive as objects with a name and a path; the
                    // rest arrive as plain strings.
                    .filter_map(|one| {
                        one.as_str()
                            .or_else(|| one.get("name")?.as_str())
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(crate::Brought {
        skills: names("skills"),
        helpers: names("agents"),
        plugins: names("plugins"),
        commands: names("slash_commands"),
    })
}

/// Separate from `read` because the two want different things from the same
/// line: the window wants a question it can show, and answering wants the
/// request exactly as it arrived. Parsing it twice is cheaper than threading
/// the raw line through everything that handles events.
fn a_question(line: &str) -> Option<(String, serde_json::Value)> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "control_request" {
        return None;
    }
    let request = v.get("request")?;
    if request.get("subtype")?.as_str()? != "can_use_tool" {
        return None;
    }
    Some((v.get("request_id")?.as_str()?.to_string(), request.clone()))
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
        // A hook of somebody's own, but only when it did something.
        //
        // Hooks run constantly and most of them run silently, so showing every
        // one would bury the conversation in machinery. What is worth a line
        // is a hook that refused something or had something to say: those are
        // the ones that change what the agent does, and before this they
        // changed it invisibly, leaving an agent that would not do a thing and
        // could not say why.
        "system" if at("subtype") == "hook_response" => {
            let failed = v
                .get("exit_code")
                .and_then(serde_json::Value::as_i64)
                .is_some_and(|code| code != 0);
            // Both fields are always there and either may be empty, so this
            // is the first non-empty one rather than the first present one.
            let said = ["output", "stderr"]
                .iter()
                .filter_map(|which| v.get(*which).and_then(|o| o.as_str()))
                .map(str::trim)
                .find(|o| !o.is_empty());
            match (failed, said) {
                (false, None) => vec![],
                (failed, said) => {
                    let name = at("hook_name");
                    let call = format!("hook-{}", at("hook_id"));
                    vec![
                        Event::Doing(Step {
                            what: match failed {
                                true => format!("A hook stopped this: {name}"),
                                false => format!("A hook of yours ran: {name}"),
                            },
                            tool: "hook".into(),
                            call: call.clone(),
                        }),
                        Event::Did {
                            call,
                            outcome: said
                                .unwrap_or("It refused, and said nothing about why.")
                                .to_string(),
                        },
                    ]
                }
            }
        }

        // Claude Code has made room for itself, which it does on its own and
        // said nothing about until now.
        //
        // Worth showing for the same reason the local engine's version is: an
        // agent that has quietly summarised away the first half of a
        // conversation is indistinguishable from one that read it and ignored
        // it, and the person is left re-explaining something they are certain
        // they said. `trigger` tells apart the one it decided on from one
        // somebody asked for, and only the first is a surprise.
        "system" if at("subtype") == "compact_boundary" => {
            let how = v
                .pointer("/compact_metadata/trigger")
                .and_then(|t| t.as_str())
                .unwrap_or("auto");
            let before = v
                .pointer("/compact_metadata/pre_tokens")
                .and_then(serde_json::Value::as_u64);
            let call = format!("room-{}", before.unwrap_or_default());
            vec![
                Event::Doing(Step {
                    what: match how {
                        "manual" => "Making room, as asked".to_string(),
                        _ => "Making room: this conversation had filled up what it can hold"
                            .to_string(),
                    },
                    tool: "context".into(),
                    call: call.clone(),
                }),
                Event::Did {
                    call,
                    outcome: match before {
                        Some(n) => format!(
                            "It kept a summary of the first {n} tokens instead of the words. \
                             All of it is still written down here, and still in the export."
                        ),
                        None => "It kept a summary instead of the words. All of it is still \
                                 written down here, and still in the export."
                            .to_string(),
                    },
                },
            ]
        }
        // A step that has stopped and is waiting to be allowed.
        "control_request" => match a_question(line) {
            Some((call, request)) => {
                let tool = request
                    .get("tool_name")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = request.get("input");
                // A tool of ours arrives under the name of the server it came
                // through, and that name is Claude Code's business rather than
                // this app's. Converted here, at the edge, so that everything
                // past this point -- the card, the allowlist, the line in the
                // conversation -- sees the one plain name the local engine
                // already uses. Checked before anything else, so a card about
                // delegation cannot be reworded by an argument that happens to
                // be called "description".
                let plain = team::which_of_ours(&tool);
                let ours = input.cloned().unwrap_or(serde_json::Value::Null);
                // What "always" would allow, worked out once, so the rule that
                // gets stored and the words on the button cannot disagree.
                // The plain name, worked out once. The card, the allowlist and
                // the words on the button all have to be about the same tool,
                // and this is the one place that decides which name that is.
                let named = plain.map_or_else(|| tool.clone(), |mine| mine.name().to_string());
                let narrowed = crate::allowing::what_always_means(
                    &named,
                    // Nothing when the engine offered nothing to remember, and
                    // an empty string when it offered the whole tool. Two
                    // different answers: collapsing them takes the button off
                    // every question about a tool with no finer rule than
                    // itself, delegation among them.
                    request
                        .get("permission_suggestions")
                        .and_then(|s| s.as_array())
                        .filter(|s| !s.is_empty())
                        .map(|_| {
                            request
                                .pointer("/permission_suggestions/0/rules/0/ruleContent")
                                .and_then(|r| r.as_str())
                                .unwrap_or("")
                        }),
                );
                vec![Event::NeedsYou(NeedsYou {
                    asking: match plain {
                        Some(name) => team::in_plain_words(name, &ours),
                        // A connector reads somebody's mail or somebody's
                        // diary, and a card about it should say which rather
                        // than naming a tool.
                        None => match crate::connectors::which(&tool) {
                            Some(job) => crate::connectors::in_plain_words(job, &ours),
                            None => in_plain_words(&tool, input),
                        },
                    },
                    detail: match plain {
                        Some(name) => team::the_thing_itself(name, &ours),
                        None => the_thing_itself(&tool, input),
                    },
                    // Offered only when the agent named a rule that would
                    // cover it. Without one there is nothing to remember, and
                    // a button that quietly does nothing is worse than no
                    // button.
                    can_remember: narrowed.is_some(),
                    // Narrowed to something reusable where it can be, and
                    // said in words either way. What the engine suggests for a
                    // shell command is the whole command line, so "always" used
                    // to mean "always, for this exact line" -- which is almost
                    // never the same line twice, and is why somebody could
                    // press it six times and be asked a seventh.
                    rule: narrowed.as_ref().map_or(String::new(), |a| a.rule.clone()),
                    allows: narrowed
                        .as_ref()
                        .map_or(String::new(), |a| a.in_words.clone()),
                    step: request
                        .get("tool_use_id")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    tool: named,
                    call,
                })]
            }
            None => vec![],
        },

        "system" if at("subtype") == "init" => vec![Event::Started {
            session: at("session_id"),
            model: at("model"),
        }],

        // A helper talking, rather than the agent itself.
        //
        // Shown underneath the step that started it rather than in the
        // conversation, which is the whole point of separating them: a
        // subagent is one step of somebody's errand, and its working out
        // dropped into the middle of the conversation reads as the agent
        // changing the subject. Before this the window said "Handing part of
        // this to a helper" and then nothing at all until the helper finished,
        // which for a long piece of work is indistinguishable from a hang.
        "assistant" if v.get("parent_tool_use_id").is_some_and(|p| !p.is_null()) => {
            let call = at("parent_tool_use_id");
            v.pointer("/message/content")
                .and_then(|c| c.as_array())
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| match b.get("type")?.as_str()? {
                            "text" => Some(b.get("text")?.as_str()?.trim().to_string())
                                .filter(|t| !t.is_empty()),
                            // What the helper is doing, in the same words the
                            // agent's own steps use.
                            "tool_use" => {
                                Some(in_plain_words(b.get("name")?.as_str()?, b.get("input")))
                            }
                            _ => None,
                        })
                        .map(|outcome| Event::Did {
                            call: call.clone(),
                            outcome,
                        })
                        .collect()
                })
                .unwrap_or_default()
        }

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

        // The prose as it is written. `--include-partial-messages` has been
        // passed since the beginning and every one of these was dropped on the
        // floor, so with this engine an answer arrived whole after several
        // seconds of nothing -- which is the exact thing the note at the top of
        // this file says reads as a hang rather than as thinking.
        //
        // Not written down: these are prefixes of a sentence that arrives whole
        // a moment later as an `assistant` message, and keeping them would keep
        // every prefix of every sentence.
        "stream_event" => {
            let event = v.get("event");
            match event.and_then(|e| e.get("type")).and_then(|t| t.as_str()) {
                Some("content_block_delta") => {
                    let delta = event.and_then(|e| e.get("delta"));
                    let kind = delta.and_then(|d| d.get("type")).and_then(|t| t.as_str());
                    // What it says, and only that. A thinking model's working
                    // arrives on this same stream as `thinking_delta`, and
                    // there is nowhere here to put it that is not the answer:
                    // the other engine has a channel of its own for it and a
                    // test insisting the two never run together, because
                    // joined, a model's private working ends up in what
                    // somebody reads and in everything summarised afterwards.
                    let text = match kind {
                        Some("text_delta") => delta.and_then(|d| d.get("text")),
                        _ => None,
                    };
                    match text.and_then(|t| t.as_str()).unwrap_or_default() {
                        "" => vec![],
                        said => vec![Event::Said {
                            text: said.to_string(),
                            settled: false,
                        }],
                    }
                }
                _ => vec![],
            }
        }

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
                // What it cost, where the engine says. Per turn: checked
                // against a real one, two messages into a single process, and
                // the second reported its own cost rather than a running total.
                let cost = v
                    .get("total_cost_usd")
                    .and_then(|c| c.as_f64())
                    .filter(|d| *d > 0.0)
                    .map(|dollars| crate::engine::Cost {
                        dollars,
                        turns: v.get("num_turns").and_then(|t| t.as_i64()).unwrap_or(1),
                    });
                vec![Event::Done { said, cost }]
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

/// Whether a failure means the session it was told to pick up is not there.
///
/// Beside the sentence it matches, so that changing the words changes both.
/// Worth knowing by name because it is the one failure that repeats itself for
/// ever: the session is gone, so every reopening asks for it again and fails
/// again, and a routine that worked yesterday never works again.
pub fn the_session_is_gone(said: &str) -> bool {
    said.contains("could not be found where it was left")
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
        // A plan is an answer, not a step. It is the thing the errand was for,
        // and Claude Code otherwise files it under ~/.claude/plans where the
        // person who asked for it will never look.
        "tool_use" if b.get("name")?.as_str()? == "ExitPlanMode" => {
            let plan = b
                .pointer("/input/plan")
                .and_then(|p| p.as_str())
                .unwrap_or_default();
            match plan.trim().is_empty() {
                true => None,
                false => Some(Event::Said {
                    text: plan.to_string(),
                    settled: true,
                }),
            }
        }
        "tool_use" => {
            let tool = b.get("name")?.as_str()?.to_string();
            // Converted here for the same reason it is converted for a
            // question. A line in the timeline reading "Using
            // mcp__errand__ask" tells somebody watching that a server they
            // never configured is doing something they cannot name, when what
            // is happening is one of their own agents asking another.
            let plain = team::which_of_ours(&tool);
            Some(Event::Doing(Step {
                what: match plain {
                    Some(name) => team::in_plain_words(
                        name,
                        b.get("input").unwrap_or(&serde_json::Value::Null),
                    ),
                    None => match crate::connectors::which(&tool) {
                        Some(job) => crate::connectors::in_plain_words(
                            job,
                            b.get("input").unwrap_or(&serde_json::Value::Null),
                        ),
                        None => in_plain_words(&tool, b.get("input")),
                    },
                },
                tool: plain.map_or(tool, |mine| mine.name().to_string()),
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
        // Both names, because it has had both. It was `Task` when this was
        // written and is `Agent` now, and a name that quietly stops matching
        // leaves a wording nobody notices is dead.
        "Task" | "Agent" => "Handing part of this to a helper".into(),
        // The end of a plan, and the moment somebody is asked whether to
        // start. Named for what it is rather than for the function, because
        // "Using ExitPlanMode" is the machinery and the plan is the point.
        "ExitPlanMode" => "Here is the plan".into(),
        other => format!("Using {other}"),
    }
}

/// The thing itself, whole, for a question that has to be judged.
///
/// The opposite of `in_plain_words`, and both are needed for the same call.
/// "Fetch the page and show the first lines" is what somebody wants to read
/// about a step that is happening; it is not enough to decide whether it
/// should. So the question shows the plain sentence and the actual command
/// underneath, uncut, because the dangerous part of a long command is usually
/// at the end of it.
fn the_thing_itself(tool: &str, input: Option<&serde_json::Value>) -> String {
    let field = |k: &str| {
        input
            .and_then(|i| i.get(k))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    match tool {
        "Bash" => field("command"),
        "Read" | "Write" | "Edit" | "NotebookEdit" => field("file_path"),
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        // The plan itself, which is the whole of what is being agreed to.
        // Without this the card shows the raw arguments, and somebody is asked
        // to approve a plan by reading it as JSON.
        "ExitPlanMode" => field("plan"),
        _ => None,
    }
    .unwrap_or_else(|| {
        // Anything unrecognised is shown as it arrived rather than summarised,
        // since the whole point here is that nothing is hidden.
        input
            .map(|i| i.to_string())
            .filter(|s| s != "null")
            .unwrap_or_default()
    })
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
    fn handing_work_to_somebody_is_shown_as_that_and_not_as_a_server_nobody_configured() {
        // What this looked like before: a timeline that said "Using
        // mcp__errand__ask" while one of somebody's own agents asked another.
        let asking = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_03","name":"mcp__errand__ask","input":{"agent":"Day Check","request":"What is today's date?"}}]}}"#;
        let Event::Doing(step) = &read(asking)[0] else {
            panic!("it was not a step");
        };
        assert_eq!(step.what, "Asking Day Check");
        assert_eq!(step.tool, "ask");

        let looking = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_04","name":"mcp__errand__who_else","input":{}}]}}"#;
        let Event::Doing(step) = &read(looking)[0] else {
            panic!("it was not a step");
        };
        assert_eq!(step.what, "Looking for somebody to hand this to");
    }

    #[test]
    fn a_missing_session_is_recognised_from_the_words_it_is_reported_in() {
        // The two are a pair: one turns the engine's complaint into a sentence,
        // the other reads that sentence back to decide the conversation may
        // start over. Kept honest here so that rewording one cannot quietly
        // strand every conversation whose session has gone.
        let said = in_words("Error: No conversation found with session ID: abc");
        assert!(the_session_is_gone(&said), "got: {said}");
        assert!(!the_session_is_gone(&in_words("something else entirely")));
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
    fn the_prose_arrives_as_it_is_written_rather_than_all_at_once() {
        // The flag that asks for this has been passed since the beginning and
        // every one of these was dropped, so an answer landed whole after
        // several seconds of nothing. Captured from a real run.
        const DELTA: &str = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The"}},"session_id":"408c"}"#;
        assert_eq!(
            read(DELTA),
            vec![Event::Said {
                text: "The".into(),
                // Not kept: this is a prefix of a sentence that arrives whole a
                // moment later, and keeping them would keep every prefix of
                // every sentence.
                settled: false
            }]
        );

        // What a thinking model shows of its working arrives on this same
        // stream, and must not join what it says. There is nowhere here to put
        // it that is not the answer, and joined, a model's private working ends
        // up in what somebody reads and in everything summarised afterwards.
        const THINKING: &str = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"weighing"}}}"#;
        assert_eq!(read(THINKING), vec![]);

        // Everything else in that stream is bookkeeping and says nothing to
        // anybody.
        for quiet in [
            r#"{"type":"stream_event","event":{"type":"message_start","message":{}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ] {
            assert_eq!(read(quiet), vec![], "{quiet}");
        }
    }

    #[test]
    fn the_end_of_a_turn_is_an_ending_or_a_failure_and_never_both() {
        assert_eq!(
            read(DONE),
            vec![Event::Done {
                // Read from the same line the answer is, because the engine
                // says what the turn cost there and this app used to throw it
                // away -- leaving no answer at all to the one question anybody
                // running errands has.
                cost: Some(crate::engine::Cost {
                    dollars: 0.18,
                    turns: 2
                }),
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

    /// Captured from a real run of the flags in the module docs. None of this
    /// shape is documented anywhere, so the line itself is the specification.
    const ASKED: &str = r#"{"type":"control_request","request_id":"f719d6a2","request":{"subtype":"can_use_tool","tool_name":"Bash","display_name":"Bash","input":{"command":"curl -s https://example.com | head -c 40","description":"Fetch example.com"},"permission_suggestions":[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"curl -s https://example.com"}],"behavior":"allow","destination":"localSettings"}],"tool_use_id":"toolu_015"}}"#;

    #[test]
    fn a_step_that_needs_permission_becomes_a_question_with_the_command_in_it() {
        let Event::NeedsYou(ask) = &read(ASKED)[0] else {
            panic!("a halted step is a question");
        };
        assert_eq!(
            ask.asking, "Fetch example.com",
            "its own words, where it has them"
        );
        assert_eq!(
            ask.detail, "curl -s https://example.com | head -c 40",
            "whole and uncut: the end of a command is the part worth reading"
        );
        assert_eq!(ask.tool, "Bash");
        assert_eq!(
            ask.call, "f719d6a2",
            "the request's own id, which is what an answer is addressed to"
        );
        assert!(
            ask.can_remember,
            "it suggested a rule, so yes can be remembered"
        );
    }

    #[test]
    fn every_model_offered_is_an_alias_rather_than_a_dated_name() {
        // A dated id retires, and the way that is found out is a routine at
        // seven in the morning dying on a name that worked yesterday. The
        // aliases are the CLI's standing promise to mean the latest of a kind,
        // so a list of them does not go stale.
        assert!(!MODELS.is_empty());
        for (alias, shown) in MODELS {
            assert!(
                !alias.contains(|c: char| c.is_ascii_digit()),
                "`{alias}` looks like a dated name rather than an alias"
            );
            assert!(!alias.contains('-'), "`{alias}` is not a bare alias");
            assert!(!shown.is_empty(), "`{alias}` has nothing to show a person");
        }
        // The one everybody will reach for first.
        assert!(MODELS.iter().any(|(alias, _)| *alias == "opus"));
    }

    #[test]
    fn a_conversation_with_a_transcript_is_known_to_be_going_whatever_was_written_down() {
        // The bug this exists to stop: an agent moved to a local model and back
        // had its "already started" flag cleared, quite correctly, but the
        // transcript stayed. Every start after that said `--session-id` about a
        // session that existed, which fails with nothing on stdout, and the
        // agent was dead with nothing in the window able to say why.
        let home = std::env::var("HOME").expect("a home");
        let cwd = std::path::Path::new("/tmp/errand test/a.b");
        let flattened = "-tmp-errand-test-a-b";
        let id = format!("probe-{}", std::process::id());

        let holds = std::path::Path::new(&home)
            .join(".claude/projects")
            .join(flattened);
        assert!(
            !already_going(&id, cwd),
            "it claimed a session nobody has ever started"
        );

        std::fs::create_dir_all(&holds).expect("somewhere for it to live");
        let transcript = holds.join(format!("{id}.jsonl"));
        std::fs::write(&transcript, "{}\n").expect("a transcript");
        let found = already_going(&id, cwd);
        let _ = std::fs::remove_file(&transcript);
        let _ = std::fs::remove_dir(&holds);

        assert!(
            found,
            "the transcript was on disk and it still wanted to start as new"
        );
    }

    #[test]
    fn what_it_turned_up_with_is_read_from_the_line_where_it_says_so() {
        // Plugins arrive as objects with a name and a path; everything else
        // arrives as a plain string. Reading only one shape drops the other
        // silently, and a panel that lists skills and no plugins looks like a
        // machine with no plugins on it.
        const OPENING: &str = r#"{"type":"system","subtype":"init","skills":["pdf","docx"],"agents":["Explore","general-purpose"],"plugins":[{"name":"marketing","path":"/x"},{"name":"productivity","path":"/y"}],"slash_commands":["init","review"]}"#;
        let kit = what_it_brought(OPENING).expect("it said what it had");
        assert_eq!(kit.skills, ["pdf", "docx"]);
        assert_eq!(kit.helpers, ["Explore", "general-purpose"]);
        assert_eq!(
            kit.plugins,
            ["marketing", "productivity"],
            "plugins were dropped"
        );
        assert_eq!(kit.commands, ["init", "review"]);
        assert!(!kit.is_empty());
    }

    #[test]
    fn any_other_line_is_not_mistaken_for_it() {
        assert!(what_it_brought(r#"{"type":"assistant","message":{"content":[]}}"#).is_none());
        assert!(what_it_brought(r#"{"type":"system","subtype":"hook_started"}"#).is_none());
        assert!(what_it_brought("not json at all").is_none());
    }

    #[test]
    fn an_engine_that_brought_nothing_says_so_rather_than_looking_broken() {
        // The honest answer for a local model: it has what this app hands it
        // and not a thing more.
        assert!(crate::Brought::default().is_empty());
    }

    #[test]
    fn a_hook_that_did_nothing_is_not_shown_and_one_that_refused_is() {
        // Hooks run constantly and mostly silently. Showing every one would
        // bury the conversation in machinery; showing none leaves an agent
        // that will not do a thing and cannot say why.
        const QUIET: &str = r#"{"type":"system","subtype":"hook_response","hook_id":"h1","hook_name":"PreToolUse:Bash","exit_code":0,"output":"","stderr":""}"#;
        assert!(read(QUIET).is_empty(), "a silent hook was shown");

        const REFUSED: &str = r#"{"type":"system","subtype":"hook_response","hook_id":"h2","hook_name":"PreToolUse:Bash","exit_code":2,"output":"","stderr":"not on this branch"}"#;
        let said = read(REFUSED);
        assert_eq!(said.len(), 2, "{said:?}");
        let Event::Doing(step) = &said[0] else {
            panic!("it was not shown as something that happened");
        };
        assert!(step.what.contains("stopped this"), "{}", step.what);
        assert!(
            step.what.contains("PreToolUse:Bash"),
            "it did not say which"
        );
        let Event::Did { outcome, .. } = &said[1] else {
            panic!("it did not say why");
        };
        assert_eq!(outcome, "not on this branch");
    }

    #[test]
    fn a_hook_that_succeeded_but_had_something_to_say_is_still_shown() {
        // The startup hook that says a plugin is ready is worth a line: it is
        // something a person put there to be told.
        const SAID: &str = r#"{"type":"system","subtype":"hook_response","hook_id":"h3","hook_name":"SessionStart:startup","exit_code":0,"output":"the tunnel is up"}"#;
        let said = read(SAID);
        assert_eq!(said.len(), 2);
        let Event::Doing(step) = &said[0] else {
            panic!()
        };
        assert!(step.what.contains("ran"), "{}", step.what);
    }

    #[test]
    fn a_helper_is_shown_under_the_step_that_started_it_and_not_in_the_conversation() {
        // A subagent is one step of somebody's errand. Its working out dropped
        // into the middle of the conversation reads as the agent changing the
        // subject, and before this the window said "Handing part of this to a
        // helper" and then nothing until it finished.
        const HELPER: &str = r#"{"type":"assistant","parent_tool_use_id":"toolu_parent","message":{"content":[{"type":"text","text":"I'll count the files."},{"type":"tool_use","id":"toolu_kid","name":"Bash","input":{"command":"ls | wc -l"}}]}}"#;
        let said = read(HELPER);
        assert_eq!(said.len(), 2, "{said:?}");
        for one in &said {
            let Event::Did { call, .. } = one else {
                panic!("a helper spoke into the conversation: {one:?}");
            };
            assert_eq!(call, "toolu_parent", "it did not land under its own step");
        }
        let Event::Did { outcome, .. } = &said[1] else {
            unreachable!()
        };
        assert_eq!(outcome, "Running ls | wc -l", "{outcome}");
    }

    #[test]
    fn the_agent_itself_still_speaks_into_the_conversation() {
        // The guard is on the presence of a parent, and an ordinary message
        // has the field set to null rather than absent.
        const ITS_OWN: &str = r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Here is the answer."}]}}"#;
        assert!(
            matches!(read(ITS_OWN).first(), Some(Event::Said { .. })),
            "the agent's own words stopped reaching the conversation"
        );
    }

    #[test]
    fn a_plan_arrives_in_the_conversation_rather_than_in_a_file_nobody_opens() {
        // Claude Code files a plan under ~/.claude/plans and ends the turn.
        // Watched once, that reads as an errand that did nothing at all: no
        // files changed, which is right, and nothing said, which is not.
        const PLANNED: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_9","name":"ExitPlanMode","input":{"plan":"1. Read the folder\n2. Write the three files"}}]}}"#;
        let said = read(PLANNED);
        let Some(Event::Said { text, settled }) = said.first() else {
            panic!("the plan was not said: {said:?}");
        };
        assert!(
            *settled,
            "a plan arriving a word at a time is not an answer"
        );
        assert!(text.contains("1. Read the folder"), "{text}");
    }

    #[test]
    fn being_asked_to_start_shows_the_plan_and_not_its_arguments() {
        // Somebody agreeing to a plan has to be able to read it. The default
        // arm renders the raw arguments, which is JSON.
        const ASKED: &str = r#"{"type":"control_request","request_id":"p1","request":{"subtype":"can_use_tool","tool_name":"ExitPlanMode","input":{"plan":"Write three files, then check them."},"tool_use_id":"toolu_9"}}"#;
        let Event::NeedsYou(ask) = &read(ASKED)[0] else {
            panic!("it was not a question");
        };
        assert_eq!(ask.asking, "Here is the plan");
        assert_eq!(ask.detail, "Write three files, then check them.");
    }

    #[test]
    fn making_room_is_shown_rather_than_done_in_silence() {
        // The failure this prevents: an agent that has summarised away the
        // first half of a conversation looks exactly like one that read it and
        // ignored it, and the person re-explains something they know they said.
        const MADE_ROOM: &str = r#"{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"auto","pre_tokens":142000,"post_tokens":18000}}"#;
        let said = read(MADE_ROOM);
        assert_eq!(
            said.len(),
            2,
            "a step with no outcome reads as a step that hung"
        );

        let Event::Doing(step) = &said[0] else {
            panic!("it was not shown as something being done");
        };
        assert!(step.what.contains("Making room"), "{}", step.what);
        assert_eq!(step.tool, "context");

        let Event::Did { outcome, call } = &said[1] else {
            panic!("it did not say what came of it");
        };
        assert_eq!(*call, step.call, "the outcome does not belong to the step");
        assert!(outcome.contains("142000"));
        assert!(
            outcome.contains("still written down"),
            "it did not say the conversation itself is intact: {outcome}"
        );
    }

    #[test]
    fn room_made_because_somebody_asked_does_not_read_as_a_surprise() {
        const ASKED: &str = r#"{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"manual","pre_tokens":9000}}"#;
        let Event::Doing(step) = &read(ASKED)[0] else {
            panic!("it was not shown");
        };
        assert!(step.what.contains("as asked"), "{}", step.what);
    }

    #[test]
    fn making_room_does_not_get_in_the_way_of_the_other_system_line() {
        // `system` carries both the opening of a thread and the making of
        // room, and the new arm sits in front of the old one. A guard that
        // matched too widely would swallow the event that says what is
        // answering, and the window would never learn which model it has.
        let opening = r#"{"type":"system","subtype":"init","session_id":"x","model":"claude-opus-5","tools":[]}"#;
        assert!(
            matches!(read(opening).first(), Some(Event::Started { .. })),
            "the opening of a thread stopped arriving"
        );
    }

    #[test]
    fn a_tool_is_granted_up_front_exactly_when_it_does_not_stop_to_ask() {
        // The two lists that have to agree and had nothing making them.
        // GRANTED is Claude Code's; `team::asks_first` is the local engine's,
        // which has never heard of GRANTED. Break the correspondence and one
        // engine acts unasked while the other shows a card every time,
        // silently, for ever. That is the exact asymmetry this app exists to
        // prevent, arriving without a single test failing.
        for declared in team::declarations() {
            let name = declared
                .pointer("/function/name")
                .and_then(|n| n.as_str())
                .expect("a declared tool has a name");
            let tool = team::ours(name).expect("a declared tool is one of ours");
            let prefixed = format!("mcp__{}__{name}", team::DOORWAY);
            assert_eq!(
                GRANTED.contains(&prefixed.as_str()),
                !team::asks_first(tool),
                "`{name}` is granted up front on one engine and asked about on the other"
            );
        }
    }

    #[test]
    fn reaching_outside_this_app_is_never_granted_up_front() {
        // The app's own tools reach one agent's own records and nothing else,
        // which is what makes granting them safe. A connector reaches somebody's
        // mail or somebody's diary, and it is run by the app rather than by the
        // walled engine, so the wall does not decide it either. Left off this
        // list, an agent that is set to ask has to ask.
        for declared in crate::connectors::declarations() {
            let name = declared
                .pointer("/function/name")
                .and_then(|n| n.as_str())
                .expect("a declared tool has a name");
            let prefixed = format!("mcp__{}__{name}", team::DOORWAY);
            assert!(
                !GRANTED.contains(&prefixed.as_str()),
                "`{name}` reaches outside this app and is granted without asking"
            );
        }
    }

    #[test]
    fn a_question_about_delegation_reads_the_same_whichever_engine_raised_it() {
        // Captured from a real run against the doorway. Claude Code names the
        // tool after the server it came through; the card, the allowlist and
        // the line in the conversation must all see the plain name, or an
        // "always" given here would not hold when a local model is answering
        // and the card would read "Using mcp__errand__ask" over raw JSON.
        const ASKED: &str = r#"{"type":"control_request","request_id":"c5f5","request":{"subtype":"can_use_tool","tool_name":"mcp__errand__ask","display_name":"Ask","input":{"agent":"Scribe","request":"Draft a reply to Sarah."},"permission_suggestions":[{"type":"addRules","rules":[{"toolName":"mcp__errand__ask"}],"behavior":"allow","destination":"localSettings"}],"tool_use_id":"toolu_01X"}}"#;

        let Event::NeedsYou(ask) = &read(ASKED)[0] else {
            panic!("it was not a question");
        };
        assert_eq!(ask.tool, "ask", "the prefix reached the allowlist");
        assert_eq!(ask.asking, "Asking Scribe");
        assert_eq!(ask.detail, "Scribe: Draft a reply to Sarah.");
        // Claude Code suggests a rule naming only the tool, with nothing to
        // narrow it. An empty rule is what the local engine writes down too,
        // which is what makes one row serve both.
        assert_eq!(ask.rule, "");
        assert!(ask.can_remember);
        // And the button says how wide that is, because an empty rule allows
        // every use of the tool and the word "always" hides that entirely.
        assert_eq!(ask.allows, "anything this agent does with ask");
    }

    #[test]
    fn saying_always_to_a_shell_command_allows_that_command_and_not_that_line() {
        // What actually happened: six "always" answers in a row and a seventh
        // question, because the rule stored was the whole command line and no
        // two lines were the same.
        const ASKED: &str = r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"top -l 1 -n 15"},"permission_suggestions":[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"top -l 1 -n 15"}],"behavior":"allow"}],"tool_use_id":"toolu_1"}}"#;
        let Event::NeedsYou(ask) = &read(ASKED)[0] else {
            panic!("it was not a question");
        };
        assert_eq!(ask.rule, "top");
        assert_eq!(ask.allows, "any top command");
    }

    #[test]
    fn a_command_that_does_more_than_one_thing_is_still_allowed_only_as_itself() {
        // Narrowing this to `printf` would allow the half after the semicolon
        // too, for ever.
        const ASKED: &str = r#"{"type":"control_request","request_id":"r2","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"printf a > f; rm -rf x"},"permission_suggestions":[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"printf a > f; rm -rf x"}],"behavior":"allow"}],"tool_use_id":"toolu_2"}}"#;
        let Event::NeedsYou(ask) = &read(ASKED)[0] else {
            panic!("it was not a question");
        };
        assert_eq!(ask.rule, "printf a > f; rm -rf x");
        assert_eq!(ask.allows, "only this exact command");
    }

    #[test]
    fn a_tool_from_somebody_elses_server_is_still_described_the_ordinary_way() {
        // Only our own two names are converted. Anything else keeps the name
        // its server gave it, because that is the name that identifies it.
        const ASKED: &str = r#"{"type":"control_request","request_id":"r9","request":{"subtype":"can_use_tool","tool_name":"mcp__peekaboo__click","input":{"x":10}}}"#;
        let Event::NeedsYou(ask) = &read(ASKED)[0] else {
            panic!("it was not a question");
        };
        assert_eq!(ask.tool, "mcp__peekaboo__click");
        assert_eq!(ask.asking, "Using mcp__peekaboo__click");
    }

    #[test]
    fn a_question_with_no_rule_behind_it_does_not_offer_to_remember_the_answer() {
        // A button that quietly does nothing is worse than no button.
        let bare = r#"{"type":"control_request","request_id":"r2","request":{"subtype":"can_use_tool","tool_name":"WebFetch","input":{"url":"https://example.com"}}}"#;
        let Event::NeedsYou(ask) = &read(bare)[0] else {
            panic!("still a question");
        };
        assert!(!ask.can_remember);
        assert_eq!(ask.detail, "https://example.com");
    }

    #[test]
    fn a_control_message_that_is_not_a_question_is_not_shown_as_one() {
        // The agent answers our opening hello on the same channel, and an
        // acknowledgement is not something to interrupt anybody with.
        for line in [
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"errand-hello"}}"#,
            r#"{"type":"control_request","request_id":"x","request":{"subtype":"interrupt"}}"#,
        ] {
            assert!(read(line).is_empty(), "showed control traffic: {line}");
        }
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
