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

/// Ask the model, and try exactly once more if the answer was "not now".
///
/// The failure this exists for belongs to standing jobs. A briefing that meets
/// a busy provider for ten seconds at seven in the morning does not run late,
/// it does not run: the conversation gets a red line nobody is awake to read.
/// At the keyboard the same failure costs a retyped sentence.
///
/// Once, not a loop. The clock marks a routine as having run before its turn
/// is claimed, so a failure does not immediately become another attempt, and
/// that ordering is what stops a provider outage becoming a hot loop. This
/// lives inside the one attempt rather than around it, so that guard is
/// untouched: two requests where there was one, and then the failure is
/// reported exactly as it always was.
async fn ask_it(
    client: &LlmClient,
    asking: &[ChatMessage],
    defs: &[ToolDef],
    cancel: &CancellationToken,
    out: &std::sync::mpsc::Sender<Event>,
) -> anyhow::Result<crate::local::stream::StreamHandle> {
    let why = match client.stream(asking, defs, None, cancel.clone()).await {
        Ok(stream) => return Ok(stream),
        Err(why) => why,
    };
    let Some(waiting) = crate::local::again::worth_another_go(&why.to_string()) else {
        return Err(why);
    };

    // Said out loud rather than waited out quietly. Twenty seconds of nothing
    // on screen cannot be told apart from a hang, and somebody watching one
    // presses Stop.
    let call = format!("again-{}", waiting.as_millis());
    let _ = out.send(Event::Doing(crate::engine::Step {
        what: crate::local::again::in_plain_words(&why.to_string(), waiting),
        tool: "waiting".into(),
        call: call.clone(),
    }));
    tokio::select! {
        _ = cancel.cancelled() => return Err(why),
        _ = tokio::time::sleep(waiting) => {}
    }

    match client.stream(asking, defs, None, cancel.clone()).await {
        Ok(stream) => {
            let _ = out.send(Event::Did {
                call,
                outcome: "It answered this time.".into(),
            });
            Ok(stream)
        }
        // The second failure is the one reported, because it is the current
        // state of the world rather than the one from three seconds ago.
        Err(again) => {
            let _ = out.send(Event::Did {
                call,
                outcome: "Still not answering.".into(),
            });
            Err(again)
        }
    }
}

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
    /// A question of the app's own, taken back out when it is answered.
    Aside(String),
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
        // Who this agent is and what it has already been told about its job.
        knows: &crate::memory::Knowing,
        // What was said in this conversation before now.
        //
        // A local model keeps no session of its own, so this is the only way it
        // knows anything about a conversation it is being reopened into. Empty
        // for a brand new one.
        so_far: Vec<ChatMessage>,
        // Where to send the things only the app can do, and which conversation
        // is asking. Nothing here means an engine on its own, which is what the
        // terminal harness is.
        host: Option<(String, tokio::sync::mpsc::UnboundedSender<team::Wants>)>,
    ) -> Result<(Self, Receiver<Event>)> {
        let knows = knows.clone();
        let (tx, rx) = channel();
        let (turns, asked) = tokio::sync::mpsc::unbounded_channel();

        let _ = tx.send(Event::Started {
            session: String::new(),
            model: settings.model.clone(),
        });

        tokio::runtime::Handle::current().spawn(conversation(
            LlmClient::new(settings),
            Opening {
                home,
                asks: asks.to_string(),
                knows,
                so_far,
            },
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

    fn aside(&mut self, text: &str) -> Result<()> {
        self.turns
            .send(Turn::Aside(text.to_string()))
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

/// Everything a conversation needs to know before its first turn.
///
/// Grouped rather than passed one by one, because they are one thing: this is
/// the state a conversation opens in, and three of the four exist only because
/// a local model keeps no session of its own and has to be told.
struct Opening {
    /// The thread's own directory, which is where every path a tool is given is
    /// resolved from and the only place it has business writing.
    home: PathBuf,
    /// How much it asks before acting.
    asks: String,
    /// Who this agent is, and what it has already been told about its job.
    knows: crate::memory::Knowing,
    /// What was said in this conversation before now. Empty for a new one.
    so_far: Vec<ChatMessage>,
}

/// The whole conversation, for as long as anybody is having it.
async fn conversation(
    client: LlmClient,
    opening: Opening,
    host: Option<(String, tokio::sync::mpsc::UnboundedSender<team::Wants>)>,
    mut asked: UnboundedReceiver<Turn>,
    out: std::sync::mpsc::Sender<Event>,
) {
    let Opening {
        home,
        asks,
        knows,
        so_far,
    } = opening;
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
        content: opening_instructions(&home, &outside, &knows, &asks),
    }];
    // And what was already said here, if this conversation has been had before.
    // Trimmed by the same rule as everything else the moment it does not fit,
    // so a very long thread reopens as its most recent part rather than
    // refusing to open at all.
    history.extend(so_far);
    // Tools somebody has said yes to for good, this conversation. Deliberately
    // not saved anywhere: a permission that outlives the thread it was granted
    // in is a permission nobody remembers granting.
    let mut allowed: HashSet<String> = HashSet::new();
    // Tools fetched by name so far. Once a schema has been paid for it stays
    // for the rest of the conversation: an errand that needed to take a
    // screenshot once will very likely need to again, and paying twice for the
    // same discovery is the thing this whole mechanism exists to avoid.
    let mut loaded: HashSet<String> = HashSet::new();
    // How much of this conversation the model can no longer see. Kept so that
    // it is mentioned when it changes rather than on every turn after.
    let mut forgotten: usize = 0;

    while let Some(turn) = asked.recv().await {
        let (said, pictures, an_aside) = match turn {
            Turn::Say(text, pictures) => (text, pictures, false),
            Turn::Aside(text) => (text, Vec::new(), true),
            // An answer with no question behind it. It happens when a thread is
            // reopened while a card is still on screen from last time.
            Turn::Answer { .. } => continue,
            Turn::Stop => break,
        };
        // Where the conversation stood before this. An aside is put back to
        // here when it is done, so nothing the app asked on its own account is
        // left in front of the model when somebody says the next thing.
        let stood_at = history.len();

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
        // Not for an aside: it is a question about the errand just finished and
        // has no use for a tool, and loading tools for it would leave them in
        // front of the model afterwards with nothing that asked for them.
        if !an_aside {
            for called in as_many_as_fit(&client.settings, &outside, &said, &history) {
                loaded.insert(called);
            }
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
            &mut forgotten,
            &mut asked,
            &out,
        )
        .await;
        // Taken back out, however it went. A failed aside that stayed would be
        // the same problem with none of the benefit.
        if an_aside {
            history.truncate(stood_at);
        }
        match ran {
            Ok(Done::Finished(said)) => {
                // Nothing: a model on this machine costs no dollars, and saying
                // $0.00 beside it would answer a question nobody asked about it.
                let _ = out.send(Event::Done { said, cost: None });
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
    // How much of this conversation the model can no longer see. Belongs to
    // the conversation and not to one turn, so that falling out of the window
    // is mentioned when it changes rather than on every turn from then on.
    forgotten: &mut usize,
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
        // What only the app can do, and what this Mac can be let at, offered
        // when there is an app to do either.
        if host.is_some() {
            let both = team::declarations()
                .into_iter()
                .chain(crate::connectors::declarations());
            defs.extend(both.map(|schema| {
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
        let held_before = asking.len();
        tokens::trim_to_fit(
            &mut asking,
            client
                .settings
                .context_window
                .saturating_sub(client.settings.max_tokens)
                .saturating_sub(tokens_in(&defs)),
        );

        // Said out loud when it happens, and only when the amount changes.
        //
        // Dropping the oldest turns is the right thing to do and was already
        // being done; doing it in silence is what made it a problem. An agent
        // that has quietly forgotten the first half of a conversation is
        // indistinguishable from one that read it and ignored it, and the
        // person is left re-explaining something they are sure they said.
        //
        // Once per change rather than once per request, because it happens on
        // every turn from then on and a line about it every time would bury
        // the conversation it is about.
        let dropped = held_before - asking.len();
        if dropped > *forgotten {
            *forgotten = dropped;
            let _ = out.send(Event::Doing(Step {
                what: format!(
                    "Making room: the earliest {} of this conversation {} out of what it can hold",
                    match dropped {
                        1 => "turn".to_string(),
                        n => format!("{n} turns"),
                    },
                    match dropped {
                        1 => "has fallen",
                        _ => "have fallen",
                    }
                ),
                tool: "context".into(),
                call: format!("room-{dropped}"),
            }));
            let _ = out.send(Event::Did {
                call: format!("room-{dropped}"),
                outcome: "It is still written down here, and still in the export. \
                          The model just cannot see that far back any more."
                    .into(),
            });
        }

        let cancel = CancellationToken::new();
        let mut stream = ask_it(client, &asking, &defs, &cancel, out).await?;

        let mut wrote = String::new();
        // What a thinking model showed of its working. Kept rather than
        // dropped, because it has to go back in the next request: DeepSeek's
        // reasoning models answer a request with tools in it and an earlier
        // assistant turn missing this with a 400, and every turn here has
        // tools in it.
        let mut thought = String::new();
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
                super::stream::ChatDelta::Reasoning(t) => thought.push_str(&t),
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
            // Kept apart from the answer, never joined onto it: joining puts
            // the model's private working into what somebody reads, into the
            // notes it keeps, and into anything it is later asked to summarise.
            reasoning: match thought.trim().is_empty() {
                true => None,
                false => Some(thought.clone()),
            },
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
            // Mail and the diary are the app's too, and they were declared to
            // the model without being routed anywhere: the arm below sent only
            // the app's own team tools up, so every connector call fell past it
            // into the local tool table, which has never heard of them. A model
            // offered `unread_mail` and told "There is no tool called
            // unread_mail here" reports that as "Mail is not connected", and
            // somebody goes looking at their permissions for a fault that is in
            // this line.
            let job = crate::connectors::which(&name);
            let this_mac = job.is_some();
            // What the person themselves typed in this conversation, and
            // nothing else. A tool result goes in as a `Tool` message and never
            // as a `User` one, so a web page's own words can never end up in
            // here pretending to be somebody asking for something.
            let they_said: Vec<String> = history
                .iter()
                .filter_map(|m| match m {
                    ChatMessage::User { content, .. } => Some(content.clone()),
                    _ => None,
                })
                .collect();
            let must_ask = match asks {
                // Ahead of `auto`, and the only thing that is. Reading a page
                // in somebody's own browser sends a request out from this Mac
                // signed in as them, so it is the one thing here that acts
                // rather than looks, and an address that came from somewhere
                // other than them is worth stopping for whatever the posture.
                // `connectors::asks_first` says why the address decides it
                // rather than the tool.
                _ if job
                    .is_some_and(|job| crate::connectors::asks_first(job, &args, &they_said)) =>
                {
                    true
                }
                // `auto` next, or it would not mean never: handing work to
                // another agent had its own default and quietly outranked the
                // posture somebody had chosen for this agent.
                "auto" => false,
                // Nothing that changes anything, because nothing is meant to
                // be changed yet. Claude Code has a posture for this; a local
                // model has only what it is told, so the wall is put here
                // rather than trusted to the instructions, which a small model
                // will talk itself past.
                "plan" => tools::asks_first(&name) || name == "run_command",
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
                    // The whole tool, and said so: an empty rule allows every
                    // use of it, and that is a bigger thing to agree to than
                    // the button used to admit.
                    allows: crate::allowing::the_whole_tool(&name).in_words,
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
                _ if mine.is_some() || this_mac => match host {
                    None => Ok(match mine {
                        Some(ours) => team::without_the_app(ours).to_string(),
                        // A connector reaches this Mac's own apps through the
                        // app itself, so without one there is nothing to reach
                        // them with.
                        None => "That reads something on this Mac, which needs Errand \
                                 itself to be running."
                            .to_string(),
                    }),
                    Some((from, to)) => {
                        let (tell_me, answer) = tokio::sync::oneshot::channel();
                        let sent = to.send(team::Wants {
                            // A model does not want a commentary on somebody
                            // else's work: none of it is the answer and all of
                            // it would be in its context.
                            along_the_way: None,
                            tool: name.clone(),
                            args: args.clone(),
                            from: from.clone(),
                            // A model asking is never the owner, whatever it
                            // says in the request.
                            as_owner: false,
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
                _ => {
                    tools::run(
                        &name,
                        &args,
                        home,
                        host.map_or("", |(from, _)| from.as_str()),
                    )
                    .await
                }
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
            // The app asking something of its own while a card is on screen.
            // Not what the card is waiting for and not a person's words either,
            // so it is neither answered nor carried into the conversation.
            Some(Turn::Aside(_)) => {}
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
    knows: &crate::memory::Knowing,
    asks: &str,
) -> String {
    // Who it is, before anything else. The first "You are" in the prompt is
    // the one a small model takes for its name, and while the identity rode
    // in with the notes that sentence was "You are Errand", with "You are
    // Inbox Watch" two thousand characters later: an agent with a name of its
    // own introduced itself as Errand. Until it has settled on a name it is
    // Errand, which is the one thing it can truthfully be called.
    let who = match knows.identity.trim().is_empty() {
        true => "You are Errand.".to_string(),
        false => format!("{}\n\nYou work inside Errand.", knows.identity.trim()),
    };
    // How much is out there, and never what any of it is called. See
    // `Servers::what_else`: listing the names made a 7B model answer with
    // nothing at all, and taking them out made the same request work.
    // What it already knows, and how to keep knowing things. Before the rest,
    // because it is about the job rather than about the machinery.
    let notes = match knows.notes.trim().is_empty() {
        true => format!("\n\n{}", crate::memory::HOW_TO_USE_IT),
        false => format!(
            "\n\n{}\n\n{}",
            crate::memory::HOW_TO_USE_IT,
            knows.notes.trim()
        ),
    };
    // Told as well as enforced. The wall in `must_ask` is what actually stops
    // it, but a model that does not know why its tools are refusing it will
    // spend the turn trying them again in different words.
    let plan = match asks == "plan" {
        false => String::new(),
        true => "\n\nTHIS ERRAND IS A PLAN, NOT THE WORK.\n\n\
                 Find out what you need to. Read, search, look things up. Then \
                 come back with what you would do, in order, and what you would \
                 need. Change nothing: no files written, no commands run, nothing \
                 sent. Somebody wants to read the plan before it happens rather \
                 than after. If you cannot work it out without changing something, \
                 say which step needs it and why, and stop there."
            .to_string(),
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
        "{who} You have been handed a job, not a design question, and you \
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
         {wall}\n\n\
         Finish on the result. Do not append an offer of further work.{more}{notes}{plan}",
        home.display(),
        // Told about the wall before it runs into it. A local model is always
        // walled in, and one that only finds out from a bare "Operation not
        // permitted" decides it is macOS and sends the person to grant access
        // the app already has.
        wall = crate::wall::what_the_wall_means(home),
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
    fn every_tool_a_local_model_is_offered_has_somewhere_to_go() {
        // What actually happened: the connector tools were declared to the
        // model and routed nowhere, so calling one answered "There is no tool
        // called unread_mail here". The model reported that as Mail not being
        // connected, and the mail feature was dead on the only engine that
        // could run, with nothing anywhere saying so.
        //
        // The declarations and the routing are two lists that have to agree.
        // This is the check that they do.
        for schema in crate::connectors::declarations() {
            let name = schema
                .pointer("/function/name")
                .and_then(|n| n.as_str())
                .expect("every declaration names its tool");
            assert!(
                crate::connectors::which(name).is_some(),
                "{name} is offered to a local model and routed nowhere"
            );
        }
        for schema in team::declarations() {
            let name = schema
                .pointer("/function/name")
                .and_then(|n| n.as_str())
                .expect("every declaration names its tool");
            assert!(
                team::ours(name).is_some(),
                "{name} is offered to a local model and routed nowhere"
            );
        }
    }

    #[test]
    fn a_plan_says_it_is_a_plan_and_an_ordinary_errand_says_nothing_about_one() {
        // Told as well as enforced. The wall in `must_ask` is what actually
        // stops it, but a model that does not know why its tools are refusing
        // will spend the turn trying them again in different words.
        let nothing = mcp::Servers::default();
        let here = std::path::Path::new("/tmp/x");
        let nobody = crate::memory::Knowing::default();
        let planning = opening_instructions(here, &nothing, &nobody, "plan");
        assert!(planning.contains("PLAN, NOT THE WORK"), "{planning}");
        assert!(planning.contains("Change nothing"));

        let ordinary = opening_instructions(here, &nothing, &nobody, "ask");
        assert!(
            !ordinary.contains("PLAN, NOT THE WORK"),
            "an ordinary errand was told it was a plan"
        );
    }

    #[test]
    fn an_agent_with_a_name_of_its_own_is_not_first_told_it_is_called_errand() {
        // What actually happened: the identity rode in with the notes, so the
        // prompt opened "You are Errand." and said "You are Inbox Watch
        // (Mail)." two thousand characters later, under YOUR OWN NOTES. The
        // app's own naming question already knows a model takes the first
        // name it is given; this is the same fact from the other side.
        let nothing = mcp::Servers::default();
        let here = std::path::Path::new("/tmp/x");
        let knows = crate::memory::Knowing {
            identity: crate::memory::who_you_are("Inbox Watch", Some("Mail"), None),
            notes: "What you have already been told about this job:\n- where it goes: Telegram"
                .into(),
        };
        let said = opening_instructions(here, &nothing, &knows, "ask");
        let first = said.find("You are ").expect("it is told who it is");
        assert!(
            said[first..].starts_with("You are Inbox Watch (Mail)."),
            "the first name it reads is not its own:\n{said}"
        );
        assert!(!said.contains("You are Errand"), "{said}");
        assert!(said.starts_with("WHO YOU ARE"), "{said}");

        // And the notes stay under their own heading, after everything else,
        // with the list right after the heading rather than the identity
        // between them.
        let notes = said.find("YOUR OWN NOTES").expect("the notes heading");
        let list = said.find("where it goes: Telegram").expect("the notes");
        let identity = said.find("WHO YOU ARE").expect("who it is");
        assert!(identity < notes && notes < list, "{said}");
        assert!(
            !said[notes..list].contains("WHO YOU ARE"),
            "the identity is filed among the notes:\n{said}"
        );

        // Until it has a name, Errand is the one thing it can be called.
        let unnamed =
            opening_instructions(here, &nothing, &crate::memory::Knowing::default(), "ask");
        assert!(unnamed.starts_with("You are Errand."), "{unnamed}");
    }

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
        let bare = opening_instructions(
            std::path::Path::new("/tmp/x"),
            &nothing,
            &crate::memory::Knowing::default(),
            "ask",
        );
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

#[cfg(test)]
mod an_aside_leaves_no_trace {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A model server that answers anything, and keeps what it was asked.
    ///
    /// Enough of one to see what went on the wire, which is the only place the
    /// thing under test is visible: an aside is invisible in the conversation,
    /// in the store and on screen, and shows up solely as messages the next
    /// request does or does not carry.
    async fn a_server_that_remembers(asked: Arc<Mutex<Vec<String>>>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let where_it_is = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let asked = asked.clone();
                tokio::spawn(async move {
                    let mut got = Vec::new();
                    let mut buf = [0u8; 4096];
                    // Until the body is whole, which is when what follows the
                    // blank line is as long as Content-Length says.
                    loop {
                        let Ok(n) = stream.read(&mut buf).await else {
                            return;
                        };
                        if n == 0 {
                            break;
                        }
                        got.extend_from_slice(&buf[..n]);
                        let text = String::from_utf8_lossy(&got).to_string();
                        let Some(at) = text.find("\r\n\r\n") else {
                            continue;
                        };
                        let want: usize = text
                            .to_ascii_lowercase()
                            .split("content-length:")
                            .nth(1)
                            .and_then(|rest| rest.split("\r\n").next())
                            .and_then(|n| n.trim().parse().ok())
                            .unwrap_or(0);
                        if text.len() - (at + 4) >= want {
                            asked.lock().unwrap().push(text[at + 4..].to_string());
                            break;
                        }
                    }
                    let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"}}]}\n\n\
                                data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                                data: [DONE]\n\n";
                    let _ = stream
                        .write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                                 Content-Length: {}\r\n\r\n{body}",
                                body.len()
                            )
                            .as_bytes(),
                        )
                        .await;
                    let _ = stream.flush().await;
                });
            }
        });
        where_it_is
    }

    async fn until_it_finishes(events: &std::sync::mpsc::Receiver<Event>) {
        let waited = std::time::Instant::now();
        while waited.elapsed() < std::time::Duration::from_secs(20) {
            match events.try_recv() {
                Ok(Event::Done { .. }) | Ok(Event::Failed { .. }) => return,
                Ok(_) => {}
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            }
        }
        panic!("the turn never finished");
    }

    #[tokio::test]
    async fn an_agent_is_told_who_it_is_before_its_first_request_goes_out() {
        // The wire is the only place any of this is visible: an agent asked
        // for a code word its own job description held answered "unknown",
        // and nothing in the store or on screen showed why.
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let where_it_is = a_server_that_remembers(asked.clone()).await;
        let home = std::env::temp_dir().join("errand-who-you-are-test");
        std::fs::create_dir_all(&home).unwrap();

        let settings = LlmSettings {
            provider: "openai-compat".into(),
            base_url: where_it_is,
            model: "pretend".into(),
            ..Default::default()
        };
        let knows = crate::memory::Knowing {
            identity: crate::memory::who_you_are(
                "Inbox Watch",
                Some("Mail"),
                Some("Reads the unread post. The code word is TANGERINE-41."),
            ),
            notes: "What you have already been told about this job:\n- where it goes: Telegram"
                .into(),
        };
        let (mut engine, events) = Local::open(settings, home, "auto", &knows, Vec::new(), None)
            .expect("a conversation to talk to");

        engine.say("what is your code word", &[]).unwrap();
        until_it_finishes(&events).await;

        let seen = asked.lock().unwrap().clone();
        let first = seen.first().expect("one request went out");
        assert!(
            first.contains("You are Inbox Watch (Mail)."),
            "the agent was not told its own name:\n{first}"
        );
        assert!(
            first.contains("TANGERINE-41"),
            "the job description never reached the model:\n{first}"
        );
        let identity = first.find("WHO YOU ARE").expect("the identity is in there");
        let notes = first
            .find("YOUR OWN NOTES")
            .expect("the notes are in there");
        let request = first
            .find("what is your code word")
            .expect("the request is in there");
        assert!(
            identity < notes && notes < request,
            "the identity is not first, or the notes are not after it:\n{first}"
        );
        // And the one name it is told is its own. Two "You are" sentences in
        // one prompt is a model that picks the first, and the first was Errand.
        let named = first.find("You are ").expect("it is told who it is");
        assert!(
            first[named..].starts_with("You are Inbox Watch (Mail)."),
            "the first name on the wire is not its own:\n{first}"
        );
        assert!(!first.contains("You are Errand"), "{first}");
    }

    #[tokio::test]
    async fn what_the_app_asks_on_its_own_account_is_gone_by_the_next_turn() {
        // What actually happened: an agent finished its first errand, the app
        // asked it who it was so it could be named, and the person's next
        // message was answered in the shape of that hidden question. They
        // asked for a daily job and were shown a line of fields. No routine
        // was set and nothing said why.
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let where_it_is = a_server_that_remembers(asked.clone()).await;
        let home = std::env::temp_dir().join("errand-aside-test");
        std::fs::create_dir_all(&home).unwrap();

        let settings = LlmSettings {
            provider: "openai-compat".into(),
            base_url: where_it_is,
            model: "pretend".into(),
            ..Default::default()
        };
        let (mut engine, events) = Local::open(
            settings,
            home,
            "auto",
            &crate::memory::Knowing::default(),
            Vec::new(),
            None,
        )
        .expect("a conversation to talk to");

        engine.say("the first errand", &[]).unwrap();
        until_it_finishes(&events).await;
        engine.aside("WHO ARE YOU, in five fields").unwrap();
        until_it_finishes(&events).await;
        engine.say("do this once a day", &[]).unwrap();
        until_it_finishes(&events).await;

        let seen = asked.lock().unwrap().clone();
        assert_eq!(seen.len(), 3, "one request per turn");
        let last = &seen[2];
        assert!(
            !last.contains("WHO ARE YOU"),
            "the app's own question was still in front of the model:\n{last}"
        );
        assert!(
            last.contains("the first errand"),
            "the conversation itself was thrown away with the aside:\n{last}"
        );
        assert!(last.contains("do this once a day"));
    }
}
