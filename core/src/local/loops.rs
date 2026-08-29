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
use super::tokens;
use super::tools;
use super::{ChatMessage, LlmSettings, ToolCall, ToolDef};
use crate::engine::{Answer, Engine, Event, NeedsYou, Step};
use crate::mcp;
use crate::team;

/// How many times round the loop before something is wrong.
///
/// Not a budget, a tripwire. A model that has called the same tool twenty times
/// is not making progress and will not start; without this it does it for ever
/// and the only symptom is a thread that never ends.
const ENOUGH: usize = 24;

/// The share of the context window that tool schemas may never exceed.
///
/// A ceiling, not a target. The rest of the window belongs to the conversation,
/// which is what somebody is actually having, and tools crowding that out is
/// the failure this exists to prevent.
const TOOLS_MAY_TAKE: usize = 4;

/// Roughly how many tokens of window buy one speculative tool.
///
/// The second bound, and the one that turned out to matter. Filling the whole
/// allowance is affordable and slow: on a 32k model the share alone took
/// twenty-one tools and nine thousand tokens of schema, and the answer went
/// from nineteen seconds to fifty-three for tools that were never called.
///
/// Cost is linear in tokens and benefit is not: the search puts the likeliest
/// tool first, so the second is worth less than the first and the twentieth is
/// worth almost nothing. This buys a few more on a model with room for them
/// without paying for a long tail nobody uses.
const PER_SPECULATION: usize = 4_000;

/// Never fewer than this, or a small model gets nothing to work with, and never
/// more, because past a handful the search has already been right or wrong.
const SPECULATION: std::ops::RangeInclusive<usize> = 3..=12;

/// A conversation with a model running on this machine.
pub struct Local {
    turns: UnboundedSender<Turn>,
}

/// Something to do to the conversation.
enum Turn {
    /// What was said, and anything attached to it as a data URL.
    Say(String, Vec<String>),
    Answer {
        call: String,
        said: Answer,
    },
    Stop,
}

impl Local {
    /// Start talking to a model.
    ///
    /// `home` is the thread's own directory, which is where every path a tool
    /// is given is resolved from and the only place it has business writing.
    pub fn open(
        settings: LlmSettings,
        home: PathBuf,
        asks: &str,
        // What this agent has already been told about its job, if anything.
        remembers: &str,
        // Where to send the things only the app can do, and which conversation
        // is asking. Nothing here means an engine on its own, which is what the
        // terminal harness is.
        host: Option<(String, tokio::sync::mpsc::UnboundedSender<team::Wants>)>,
    ) -> Result<(Self, Receiver<Event>)> {
        let remembers = remembers.to_string();
        let (tx, rx) = channel();
        let (turns, asked) = tokio::sync::mpsc::unbounded_channel();

        let _ = tx.send(Event::Started {
            session: String::new(),
            model: settings.model.clone(),
        });

        tokio::runtime::Handle::current().spawn(conversation(
            LlmClient::new(settings),
            home,
            asks.to_string(),
            remembers,
            host,
            asked,
            tx,
        ));
        Ok((Self { turns }, rx))
    }
}

impl Engine for Local {
    fn say(&mut self, text: &str, pictures: &[crate::Picture]) -> Result<()> {
        self.turns
            .send(Turn::Say(
                text.to_string(),
                pictures.iter().map(crate::Picture::as_data_url).collect(),
            ))
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
    asks: String,
    remembers: String,
    host: Option<(String, tokio::sync::mpsc::UnboundedSender<team::Wants>)>,
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
        content: opening_instructions(&home, &outside, &remembers),
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
        let (said, pictures) = match turn {
            Turn::Say(text, pictures) => (text, pictures),
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
        // As many as fit rather than a fixed few: what fits depends on the
        // model, and a number chosen for one is wrong for every other.
        for called in as_many_as_fit(&client.settings, &outside, &said, &history) {
            loaded.insert(called);
        }

        history.push(ChatMessage::User {
            content: said,
            name: None,
            image_data_urls: pictures,
        });

        let ran = errand(
            &client,
            &home,
            &outside,
            &asks,
            host.as_ref(),
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
    asks: &str,
    host: Option<&(String, tokio::sync::mpsc::UnboundedSender<team::Wants>)>,
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
        // What only the app can do, offered when there is an app to do it.
        if host.is_some() {
            defs.extend(team::declarations().into_iter().map(|schema| {
                ToolDef {
                    name: schema
                        .pointer("/function/name")
                        .and_then(|n| n.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    description: String::new(),
                    schema,
                }
            }));
        }

        // A turn's inputs are the one thing nothing else prints, and an answer
        // that comes back empty is almost always one of them. Off unless asked
        // for, and never the contents of a message, only their shape.
        if std::env::var("ERRAND_TRACE").is_ok() {
            eprintln!(
                "round {round}: {} tools ({}), {} messages, {} tokens of schema",
                defs.len(),
                defs.iter()
                    .map(|d| d.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                history.len(),
                tokens_in(&defs),
            );
        }

        // Trimmed to fit, on a copy, every round.
        //
        // Without this a long errand simply stops working: tool results are the
        // biggest things in a conversation and there is no warning before the
        // window is full, only a request that fails or an answer that ignores
        // the beginning. The guard drops the oldest turns and never the
        // instructions or the thing just asked.
        //
        // A copy because `history` is the conversation, and dropping a turn to
        // make one request fit is not a reason to forget it happened.
        let mut asking = history.clone();
        tokens::trim_to_fit(
            &mut asking,
            client
                .settings
                .context_window
                .saturating_sub(client.settings.max_tokens)
                .saturating_sub(tokens_in(&defs)),
        );

        let cancel = CancellationToken::new();
        let mut stream = client.stream(&asking, &defs, None, cancel).await?;

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
            // The agent's posture, the same three words the other engine uses.
            // `auto` asks about nothing, `edits` lets a file be written, and
            // `ask` -- the default -- asks about everything that changes
            // anything.
            // Resolved once. Everything below asks the same question of the
            // same name, and doing it by string in three places is how a
            // fourth place gets it slightly different.
            let mine = team::ours(&name);
            let must_ask = match asks {
                // `auto` first, or it would not mean never: handing work to
                // another agent had its own default and quietly outranked the
                // posture somebody had chosen for this agent.
                "auto" => false,
                _ if mine.is_some() => mine.is_some_and(team::asks_first),
                "edits" => tools::asks_first(&name) && name != "write_file",
                _ => tools::asks_first(&name),
            };
            if must_ask && !allowed.contains(&name) {
                let _ = out.send(Event::NeedsYou(NeedsYou {
                    asking: say_plainly(outside, &name, &args),
                    detail: match mine {
                        Some(mine) => team::the_thing_itself(mine, &args),
                        None => tools::the_thing_itself(&name, &args),
                    },
                    tool: name.clone(),
                    // Both ids, and here they happen to be the same one: this
                    // engine's questions are about the tool call directly,
                    // with no request of their own in between.
                    call: call.id.clone(),
                    step: call.id.clone(),
                    can_remember: true,
                    // The whole tool. A local model's tools are coarse enough
                    // that anything finer would be guesswork: allowing one
                    // exact shell command is not a permission anybody wants to
                    // grant twice, and allowing a prefix of one is a rule this
                    // side has no basis for inventing.
                    rule: String::new(),
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
                // Only the thing holding every agent can reach another one, so
                // this goes up rather than being answered here.
                _ if mine.is_some() => match host {
                    None => Ok(team::without_the_app(mine.expect("just matched")).to_string()),
                    Some((from, to)) => {
                        let (tell_me, answer) = tokio::sync::oneshot::channel();
                        let sent = to.send(team::Wants {
                            tool: name.clone(),
                            args: args.clone(),
                            from: from.clone(),
                            answer: tell_me,
                        });
                        match sent {
                            Err(_) => Ok("Nobody answered.".to_string()),
                            Ok(()) => answer
                                .await
                                .unwrap_or_else(|_| Ok("Nobody answered.".to_string())),
                        }
                    }
                },
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
            Some(Turn::Say(text, _)) => meanwhile.push(text),
            // An answer to some other question, which by now has no question
            // behind it. Nothing to do with it but let it go.
            Some(Turn::Answer { .. }) => {}
        }
    }
}

/// What the tool declarations cost, in the model's own units.
fn tokens_in(defs: &[ToolDef]) -> usize {
    defs.iter()
        .map(|d| tokens::count_tokens(&d.schema.to_string()))
        .sum()
}

/// The tools worth putting in front of the model for this request.
///
/// Best matches first, taken while they fit. What fits is worked out from the
/// model's own context window rather than guessed: whatever is left after the
/// reply is reserved and the conversation so far is accounted for, capped at a
/// share of the window so that tools can never crowd out the conversation.
///
/// Counted with a real tokenizer rather than by dividing bytes by four. The
/// difference is not academic for JSON Schema, which is mostly punctuation and
/// short keys and tokenizes far worse than prose.
fn as_many_as_fit(
    settings: &LlmSettings,
    outside: &mcp::Servers,
    said: &str,
    history: &[ChatMessage],
) -> Vec<String> {
    let reserved_for_the_reply = settings.max_tokens;
    let already_used = tokens::estimate_messages(history) + tokens::count_tokens(said);
    let left = settings
        .context_window
        .saturating_sub(reserved_for_the_reply)
        .saturating_sub(already_used);

    let mut room = left.min(settings.context_window / TOOLS_MAY_TAKE);
    let most =
        (settings.context_window / PER_SPECULATION).clamp(*SPECULATION.start(), *SPECULATION.end());

    let mut taking = Vec::new();
    // Every tool there is, in match order, so a long conversation still gets
    // the best one it can afford rather than the first alphabetically.
    for tool in outside.matching(said, usize::MAX) {
        if taking.len() >= most {
            break;
        }
        let costs =
            tokens::count_tokens(&tool.description) + tokens::count_tokens(&tool.takes.to_string());
        if costs > room {
            // Not `break`: a small tool after a large one is still worth
            // having, and the large one is what did not fit.
            continue;
        }
        room -= costs;
        taking.push(tool.called.clone());
    }
    taking
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
    // A handful, because this is a list for a model to read and choose from
    // rather than everything it could afford. The budget is what bounds how
    // much is actually sent; this bounds how much is worth reading.
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
    if let Some(mine) = team::ours(name) {
        return team::in_plain_words(mine, args);
    }
    match outside.knows(name) {
        None => tools::in_plain_words(name, args),
        Some(tool) => match tool.description.lines().next().map(str::trim) {
            Some(said) if !said.is_empty() => said.to_string(),
            _ => format!("Using {} from {}", tool.own_name, tool.server),
        },
    }
}

/// What the model is told before anything else.
pub(crate) fn opening_instructions(
    home: &std::path::Path,
    outside: &mcp::Servers,
    remembers: &str,
) -> String {
    // How much is out there, and never what any of it is called. See
    // `Servers::what_else`: listing the names made a 7B model answer with
    // nothing at all, and taking them out made the same request work.
    // What it already knows, and how to keep knowing things. Before the rest,
    // because it is about the job rather than about the machinery.
    let notes = match remembers.trim().is_empty() {
        true => format!("\n\n{}", crate::memory::HOW_TO_USE_IT),
        false => format!("\n\n{}\n\n{remembers}", crate::memory::HOW_TO_USE_IT),
    };
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
         Finish on the result. Do not append an offer of further work.{more}{notes}",
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

    /// A server offering tools whose schemas cost roughly `each` tokens.
    fn offering(count: usize, each: usize) -> mcp::Servers {
        let mut servers = mcp::Servers::default();
        for i in 0..count {
            servers.add_for_testing(mcp::Tool {
                called: format!("mcp__pretend__tool_{i}"),
                own_name: format!("tool_{i}"),
                server: "pretend".into(),
                description: format!("Take a screenshot. {}", "padding ".repeat(each / 2)),
                takes: serde_json::json!({ "type": "object" }),
            });
        }
        servers
    }

    #[test]
    fn a_bigger_window_takes_more_tools_and_a_smaller_one_takes_fewer() {
        let servers = offering(40, 200);
        let asking = "take a screenshot";

        let small = LlmSettings {
            context_window: 8_192,
            max_tokens: 2_048,
            ..Default::default()
        };
        let large = LlmSettings {
            context_window: 128_000,
            max_tokens: 4_096,
            ..Default::default()
        };

        let few = as_many_as_fit(&small, &servers, asking, &[]);
        let many = as_many_as_fit(&large, &servers, asking, &[]);
        assert!(!few.is_empty(), "even a small window affords something");
        assert!(
            many.len() > few.len(),
            "a model with sixteen times the room took {} and {}",
            many.len(),
            few.len()
        );
        assert!(
            many.len() <= *SPECULATION.end(),
            "past a handful the search has already been right or wrong"
        );
        assert!(
            few.len() >= *SPECULATION.start(),
            "a small model still gets something"
        );
    }

    #[test]
    fn a_conversation_too_long_for_the_window_is_trimmed_from_the_oldest_end() {
        // The instructions and the thing just asked are the two that cannot go.
        let mut talk = vec![ChatMessage::System {
            content: "the instructions".into(),
        }];
        for i in 0..60 {
            talk.push(ChatMessage::User {
                content: format!("turn {i} {}", "words ".repeat(200)),
                name: None,
                image_data_urls: vec![],
            });
        }
        talk.push(ChatMessage::User {
            content: "the thing just asked".into(),
            name: None,
            image_data_urls: vec![],
        });

        let before = talk.len();
        tokens::trim_to_fit(&mut talk, 2_000);
        assert!(talk.len() < before, "nothing was dropped");
        assert!(
            matches!(talk.first(), Some(ChatMessage::System { .. })),
            "it lost its instructions"
        );
        assert_eq!(
            talk.last().map(|m| m.content()),
            Some("the thing just asked"),
            "it forgot what it was asked"
        );
    }

    #[test]
    fn tools_never_take_more_than_their_share_however_big_the_window() {
        // The conversation is what somebody is actually having. A thousand
        // tools fitting is not a reason to send a thousand.
        let servers = offering(1_000, 400);
        let huge = LlmSettings {
            context_window: 200_000,
            max_tokens: 4_096,
            ..Default::default()
        };
        let taken = as_many_as_fit(&huge, &servers, "take a screenshot", &[]);

        // Asserted as the cost of what was taken rather than as a count, since
        // the share is the rule and the count is only what falls out of it.
        let spent: usize = taken
            .iter()
            .filter_map(|called| servers.knows(called))
            .map(|t| {
                tokens::count_tokens(&t.description) + tokens::count_tokens(&t.takes.to_string())
            })
            .sum();
        assert!(
            spent <= huge.context_window / TOOLS_MAY_TAKE,
            "spent {spent} of a {} allowance",
            huge.context_window / TOOLS_MAY_TAKE
        );
        assert!(taken.len() < 1_000, "took all of them anyway");
    }

    #[test]
    fn a_conversation_that_has_filled_the_window_leaves_no_room_for_tools() {
        // And says so by taking none, rather than by sending a request that
        // cannot be answered.
        let servers = offering(10, 200);
        let small = LlmSettings {
            context_window: 4_096,
            max_tokens: 2_048,
            ..Default::default()
        };
        let long: Vec<ChatMessage> = (0..40)
            .map(|_| ChatMessage::User {
                content: "words ".repeat(200),
                name: None,
                image_data_urls: vec![],
            })
            .collect();
        assert!(as_many_as_fit(&small, &servers, "take a screenshot", &long).is_empty());
    }

    #[test]
    fn the_model_is_told_the_names_of_everything_it_could_reach_but_not_the_schemas() {
        let nothing = mcp::Servers::default();
        let bare = opening_instructions(std::path::Path::new("/tmp/x"), &nothing, "");
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
