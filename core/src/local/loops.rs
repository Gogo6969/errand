//! The agent loop, for an engine that does not bring one.
//!
//! Claude Code is handed an errand and comes back having done it; everything
//! between those two moments is its business. A local model answers one
//! request at a time and stops, so the going-round-again is here: ask, read the
//! tool calls, do them, put the results back, ask again.
//!
//! The shape is deliberately the same as the other engine's from the outside.
//! One task owns the conversation and everything else posts to it, so `say` is
//! safe to call while a tool is running, and answering a question is just
//! another thing posted to the same queue. That last part matters more than it
//! looks: a question halts this loop, and the thing that unhalts it has to
//! arrive on a channel the loop is already listening to, or the loop would have
//! to poll something while it waits.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

use anyhow::Result;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use super::talk::LlmClient;
use super::tools;
use super::{ChatMessage, LlmSettings, ToolCall, ToolDef};
use crate::engine::{Answer, Engine, Event, NeedsYou, Step};
use crate::mcp;

/// How many times round the loop before something is wrong.
///
/// Not a budget, a tripwire. A model that has called the same tool twenty times
/// is not making progress and will not start; without this it does it for ever
/// and the only symptom is a thread that never ends.
const ENOUGH: usize = 24;

/// How many outside tools to put in front of the model without being asked.
///
/// Enough that an obvious job has its obvious tool to hand, few enough that the
/// schemas stay affordable. Five of peekaboo's is about twelve thousand bytes
/// against sixty-four for all of them.
const LIKELY: usize = 5;

/// A conversation with a model running on this machine.
pub struct Local {
    turns: UnboundedSender<Turn>,
}

/// Something to do to the conversation.
enum Turn {
    Say(String),
    Answer { call: String, said: Answer },
    Stop,
}

impl Local {
    /// Start talking to a model.
    ///
    /// `home` is the thread's own directory, which is where every path a tool
    /// is given is resolved from and the only place it has business writing.
    pub fn open(settings: LlmSettings, home: PathBuf) -> Result<(Self, Receiver<Event>)> {
        let (tx, rx) = channel();
        let (turns, asked) = tokio::sync::mpsc::unbounded_channel();

        let _ = tx.send(Event::Started {
            session: String::new(),
            model: settings.model.clone(),
        });

        tokio::runtime::Handle::current().spawn(conversation(
            LlmClient::new(settings),
            home,
            asked,
            tx,
        ));
        Ok((Self { turns }, rx))
    }
}

impl Engine for Local {
    fn say(&mut self, text: &str) -> Result<()> {
        self.turns
            .send(Turn::Say(text.to_string()))
            .map_err(|_| anyhow::anyhow!("this conversation has ended"))
    }

    fn answer(&mut self, call: &str, said: Answer) -> Result<()> {
        self.turns
            .send(Turn::Answer {
                call: call.to_string(),
                said,
            })
            .map_err(|_| anyhow::anyhow!("this conversation has ended"))
    }

    fn stop(&mut self) -> Result<()> {
        let _ = self.turns.send(Turn::Stop);
        Ok(())
    }
}

/// The whole conversation, for as long as anybody is having it.
async fn conversation(
    client: LlmClient,
    home: PathBuf,
    mut asked: UnboundedReceiver<Turn>,
    out: std::sync::mpsc::Sender<Event>,
) {
    // Started once for the conversation rather than once per turn. Several of
    // these are `npx` and take seconds to come up; paying that on every message
    // would make the thread feel broken.
    //
    // A server that did not start is not announced here. It was, briefly, and
    // it was wrong twice over: the same sentence went into the transcript again
    // every time the thread was reopened, and it is not something the agent
    // said. It belongs where somebody goes to look, which is the panel that
    // lists what this thread can reach, and `trouble` is what that panel reads.
    let outside = mcp::Servers::open(&home).await;

    let mut history = vec![ChatMessage::System {
        content: opening_instructions(&home, &outside),
    }];
    // Tools somebody has said yes to for good, this conversation. Deliberately
    // not saved anywhere: a permission that outlives the thread it was granted
    // in is a permission nobody remembers granting.
    let mut allowed: HashSet<String> = HashSet::new();
    // Tools fetched by name so far. Once a schema has been paid for it stays
    // for the rest of the conversation: an errand that needed to take a
    // screenshot once will very likely need to again, and paying twice for the
    // same discovery is the thing this whole mechanism exists to avoid.
    let mut loaded: HashSet<String> = HashSet::new();

    while let Some(turn) = asked.recv().await {
        let said = match turn {
            Turn::Say(text) => text,
            // An answer with no question behind it. It happens when a thread is
            // reopened while a card is still on screen from last time.
            Turn::Answer { .. } => continue,
            Turn::Stop => break,
        };

        // Look the request up before handing it over.
        //
        // `find_tools` exists and works, and a small model does not reliably
        // think to use it: asked to check macOS permissions it guessed at shell
        // commands three times rather than searching, while the right tool sat
        // one lookup away. So the search runs here, on the words the person
        // just used, and the likely tools are already in front of the model on
        // the first round. It is the same search either way, done by the thing
        // that already knows what was asked.
        //
        // Deliberately a few, not all: the whole point is not to be back at
        // sixty-four thousand bytes of schemas per request.
        for tool in outside.matching(&said, LIKELY) {
            loaded.insert(tool.called.clone());
        }

        history.push(ChatMessage::User {
            content: said,
            name: None,
            image_data_urls: vec![],
        });

        let ran = errand(
            &client,
            &home,
            &outside,
            &mut history,
            &mut allowed,
            &mut loaded,
            &mut asked,
            &out,
        )
        .await;
        match ran {
            Ok(Done::Finished(said)) => {
                let _ = out.send(Event::Done { said });
            }
            Ok(Done::Abandoned) => break,
            Err(why) => {
                let _ = out.send(Event::Failed {
                    why: why.to_string(),
                });
            }
        }
    }
}

/// How a turn ended.
enum Done {
    Finished(String),
    /// The person closed the thread while it was working.
    Abandoned,
}

/// One errand: round the loop until the model stops asking for tools.
#[allow(clippy::too_many_arguments)]
async fn errand(
    client: &LlmClient,
    home: &std::path::Path,
    outside: &mcp::Servers,
    history: &mut Vec<ChatMessage>,
    allowed: &mut HashSet<String>,
    loaded: &mut HashSet<String>,
    asked: &mut UnboundedReceiver<Turn>,
    out: &std::sync::mpsc::Sender<Event>,
) -> Result<Done> {
    // Ours always, and theirs only once somebody has asked for them.
    //
    // Sending every tool every time was correct and unaffordable: twenty-six
    // servers' worth of schemas is sixty-four thousand bytes in front of every
    // single request, which on a small model is most of the minute it takes to
    // answer. So the model is told the names up front, which is a few hundred
    // bytes, and fetches the schemas it actually wants.
    let with_schemas = |t: &crate::mcp::Tool| ToolDef {
        name: t.called.clone(),
        description: t.description.clone(),
        schema: serde_json::json!({
            "type": "function",
            "function": {
                "name": t.called,
                "description": t.description,
                "parameters": t.takes,
            },
        }),
    };

    for round in 0..ENOUGH {
        // Rebuilt each time round, because the last step may have fetched more.
        let mut defs: Vec<ToolDef> = tools::all()
            .into_iter()
            .filter(|t| t.def.name != "find_tools" || !outside.tools().is_empty())
            .map(|t| t.def)
            .collect();
        defs.extend(
            outside
                .tools()
                .iter()
                .filter(|t| loaded.contains(&t.called))
                .map(&with_schemas),
        );

        // A turn's inputs are the one thing nothing else prints, and an answer
        // that comes back empty is almost always one of them. Off unless asked
        // for, and never the contents of a message, only their shape.
        if std::env::var("ERRAND_TRACE").is_ok() {
            eprintln!(
                "round {round}: {} tools ({}), {} messages, {} bytes of instruction",
                defs.len(),
                defs.iter()
                    .map(|d| d.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                history.len(),
                history
                    .iter()
                    .map(|m| match m {
                        ChatMessage::System { content } => content.len(),
                        _ => 0,
                    })
                    .sum::<usize>(),
            );
        }

        let cancel = CancellationToken::new();
        let mut stream = client.stream(history, &defs, None, cancel).await?;

        let mut wrote = String::new();
        let mut wants: Vec<super::stream::ToolCallAccum> = Vec::new();
        let mut broke: Option<String> = None;

        while let Some(delta) = stream.rx.recv().await {
            match delta {
                super::stream::ChatDelta::Token(t) => {
                    wrote.push_str(&t);
                    // Unsettled, so a sentence being written looks like one.
                    let _ = out.send(Event::Said {
                        text: t,
                        settled: false,
                    });
                }
                // The thinking is the machinery underneath, and nobody watching
                // their own errand needs to watch it.
                super::stream::ChatDelta::Reasoning(_) => {}
                super::stream::ChatDelta::ToolCall(call) => wants.push(call),
                super::stream::ChatDelta::Done { .. } => break,
                super::stream::ChatDelta::Error(why) => {
                    broke = Some(why);
                    break;
                }
            }
        }

        if std::env::var("ERRAND_TRACE").is_ok() {
            eprintln!(
                "  -> {} characters, {} tool calls{}",
                wrote.len(),
                wants.len(),
                match &broke {
                    Some(why) => format!(", broke: {why}"),
                    None => String::new(),
                }
            );
        }

        // A stream that broke after writing something has still written it.
        // Throwing away half an answer because the connection hiccuped is
        // worse than showing half an answer and saying it was cut off.
        if let Some(why) = broke {
            if wrote.trim().is_empty() {
                anyhow::bail!("{why}");
            }
            let said = format!("{}\n\n(cut off: {why})", wrote.trim());
            let _ = out.send(Event::Said {
                text: said.clone(),
                settled: true,
            });
            return Ok(Done::Finished(said));
        }

        // Nothing more to do: this is the answer.
        if wants.is_empty() {
            let said = wrote.trim().to_string();
            if !said.is_empty() {
                let _ = out.send(Event::Said {
                    text: said.clone(),
                    settled: true,
                });
            }
            return Ok(Done::Finished(said));
        }

        // Anything it said on the way to deciding is still worth keeping.
        if !wrote.trim().is_empty() {
            let _ = out.send(Event::Said {
                text: wrote.trim().to_string(),
                settled: true,
            });
        }

        let calls: Vec<ToolCall> = wants
            .iter()
            .enumerate()
            .map(|(i, w)| {
                ToolCall::new(
                    w.id.clone().unwrap_or_else(|| format!("call-{round}-{i}")),
                    w.name.clone().unwrap_or_default(),
                    match w.arguments.is_empty() {
                        true => "{}".to_string(),
                        false => w.arguments.clone(),
                    },
                )
            })
            .collect();

        history.push(ChatMessage::Assistant {
            content: wrote.trim().to_string(),
            tool_calls: calls.clone(),
        });

        for call in calls {
            let name = call.function.name.clone();
            let args: serde_json::Value =
                serde_json::from_str(&call.function.arguments).unwrap_or(serde_json::json!({}));

            let _ = out.send(Event::Doing(Step {
                what: say_plainly(outside, &name, &args),
                tool: name.clone(),
                call: call.id.clone(),
            }));

            // The card, for exactly the same reasons as the other engine. A
            // local model running a shell command is no safer for being local.
            if tools::asks_first(&name) && !allowed.contains(&name) {
                let _ = out.send(Event::NeedsYou(NeedsYou {
                    asking: say_plainly(outside, &name, &args),
                    detail: tools::the_thing_itself(&name, &args),
                    tool: name.clone(),
                    // Both ids, and here they happen to be the same one: this
                    // engine's questions are about the tool call directly,
                    // with no request of their own in between.
                    call: call.id.clone(),
                    step: call.id.clone(),
                    // Remembering is per tool, for this conversation only.
                    can_remember: true,
                }));

                let (said, meanwhile) = match wait_for_an_answer(asked, &call.id).await {
                    None => return Ok(Done::Abandoned),
                    Some(both) => both,
                };
                // Whatever they typed while deciding is part of the
                // conversation and goes in before the tool result, so the model
                // reads it as context for the step rather than as a new errand.
                for text in meanwhile {
                    history.push(ChatMessage::User {
                        content: text,
                        name: None,
                        image_data_urls: vec![],
                    });
                }
                match said {
                    Answer::No => {
                        let refused = "You said no. Try another way, or say what you need.";
                        let _ = out.send(Event::Did {
                            call: call.id.clone(),
                            outcome: "Not allowed".to_string(),
                        });
                        history.push(ChatMessage::Tool {
                            content: refused.to_string(),
                            tool_call_id: call.id.clone(),
                        });
                        continue;
                    }
                    Answer::Always => {
                        allowed.insert(name.clone());
                    }
                    Answer::Yes => {}
                }
            }

            let outcome = match match name.as_str() {
                "find_tools" => Ok(look_up(outside, loaded, &args)),
                _ if outside.knows(&name).is_some() => {
                    // A model can call something it has only seen the name of,
                    // and refusing on a technicality would be pedantry: it
                    // knows what it wants. Keep the schema for next time.
                    loaded.insert(name.clone());
                    outside.call(&name, &args).await
                }
                _ => tools::run(&name, &args, home).await,
            } {
                Ok(said) => said,
                // Told to the model as a result, not raised as an error: a
                // failed step is something to try differently, and an error is
                // something to give up on.
                Err(why) => format!("That did not work: {why}"),
            };
            let _ = out.send(Event::Did {
                call: call.id.clone(),
                outcome: first_line(&outcome),
            });
            history.push(ChatMessage::Tool {
                content: outcome,
                tool_call_id: call.id,
            });
        }
    }

    let stuck = format!(
        "It went round {ENOUGH} times without finishing, so it was stopped. \
         Whatever it is trying is not working."
    );
    let _ = out.send(Event::Said {
        text: stuck.clone(),
        settled: true,
    });
    Ok(Done::Finished(stuck))
}

/// Wait for somebody to answer this particular question.
///
/// A person looking at a card does not always press a button. Sometimes they
/// type, because what they want to say is "yes, but only the first one" and no
/// button says that. So anything said while waiting comes back with the answer
/// and is put into the conversation, rather than being swallowed because it
/// arrived at an inconvenient moment.
///
/// Returns nothing at all only when the thread is being closed.
async fn wait_for_an_answer(
    asked: &mut UnboundedReceiver<Turn>,
    call: &str,
) -> Option<(Answer, Vec<String>)> {
    let mut meanwhile: Vec<String> = Vec::new();
    loop {
        match asked.recv().await {
            Some(Turn::Answer { call: which, said }) if which == call => {
                return Some((said, meanwhile))
            }
            Some(Turn::Stop) | None => return None,
            Some(Turn::Say(text)) => meanwhile.push(text),
            // An answer to some other question, which by now has no question
            // behind it. Nothing to do with it but let it go.
            Some(Turn::Answer { .. }) => {}
        }
    }
}

/// Answer a lookup, and make what it found available.
///
/// The reply is what the model needs to decide, and no more: names and the one
/// line each tool leads with. The full schemas arrive with the next request,
/// which is the whole point -- they are paid for once, when something is going
/// to be used, rather than every time in case it is.
fn look_up(
    outside: &mcp::Servers,
    loaded: &mut HashSet<String>,
    args: &serde_json::Value,
) -> String {
    const AT_A_TIME: usize = 5;
    let needing = args
        .get("needing")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();

    let found = outside.matching(&needing, AT_A_TIME);
    if found.is_empty() {
        // A fact, and a nudge towards a better search rather than a list to
        // pick from, for the same reason the catalogue has no names in it.
        return format!(
            "Nothing matched \"{needing}\". There are {} to search. Try naming the action \
             rather than the tool: what you want to happen, in a few plain words.",
            outside.what_else()
        );
    }

    let mut said = String::from("These are available to you now:\n");
    for tool in &found {
        loaded.insert(tool.called.clone());
        said.push_str(&format!(
            "  {} -- {}\n",
            tool.called,
            one_line(&tool.description)
        ));
    }
    said
}

/// The first line of something, short enough to sit in a list.
fn one_line(s: &str) -> String {
    let line = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    match line.chars().count() > 140 {
        true => format!("{}…", line.chars().take(139).collect::<String>()),
        false => line.to_string(),
    }
}

/// What a step is doing, in words, wherever the tool came from.
///
/// A tool from outside has no entry in our list and never will: the whole point
/// is that anybody can add one. So its own description is used, which is what
/// the server wrote for the model to read, and it is the best sentence anybody
/// has about it.
fn say_plainly(outside: &mcp::Servers, name: &str, args: &serde_json::Value) -> String {
    match outside.knows(name) {
        None => tools::in_plain_words(name, args),
        Some(tool) => match tool.description.lines().next().map(str::trim) {
            Some(said) if !said.is_empty() => said.to_string(),
            _ => format!("Using {} from {}", tool.own_name, tool.server),
        },
    }
}

/// What the model is told before anything else.
pub(crate) fn opening_instructions(home: &std::path::Path, outside: &mcp::Servers) -> String {
    // How much is out there, and never what any of it is called. See
    // `Servers::what_else`: listing the names made a 7B model answer with
    // nothing at all, and taking them out made the same request work.
    let more = match outside.tools().is_empty() {
        true => String::new(),
        false => format!(
            "\n\nThere are {} tools available beyond the ones you can see, fetched with \
             find_tools. Do that before concluding something cannot be done here.",
            outside.what_else()
        ),
    };

    format!(
        "You are Errand. You have been handed a job, not a design question, and you \
         come back having done it.\n\n\
         Do the work before you write a word. Where the request is under-specified, \
         pick the obvious sensible default, act on it, and say what you assumed. A \
         question you ask instead of acting is worse than a default you state.\n\n\
         A failed route is information, not a stopping point. Note it and try the \
         next one. \"I got nothing\" is an answer only after at least three genuinely \
         different attempts you can name.\n\n\
         You have tools. Use them rather than describing what you would do. If your \
         message says you will read, fetch or run something, the tool call is in the \
         same turn.\n\n\
         Some tools stop and ask the person first. That is normal and not a failure: \
         wait for the answer. If the answer is no, find another way rather than \
         asking again.\n\n\
         Your working directory is {}. Paths are relative to it and it is the only \
         place you write.\n\n\
         Finish on the result. Do not append an offer of further work.{more}",
        home.display()
    )
}

/// One line of it, short enough to sit in a timeline.
fn first_line(s: &str) -> String {
    let line = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    match line.chars().count() > 80 {
        true => format!("{}…", line.chars().take(79).collect::<String>()),
        false => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_is_told_the_names_of_everything_it_could_reach_but_not_the_schemas() {
        let nothing = mcp::Servers::default();
        let bare = opening_instructions(std::path::Path::new("/tmp/x"), &nothing);
        assert!(
            !bare.contains("find_tools"),
            "with no servers there is nothing to look up, and saying so invites a wild goose chase"
        );
        assert!(
            bare.contains("/tmp/x"),
            "it still needs to know where it is working"
        );
    }
}
