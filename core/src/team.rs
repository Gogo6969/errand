//! One agent asking another.
//!
//! The thing in the walkthrough that looks like magic: a delegator hands the
//! research to one agent, which hands the writing to another, and you can open
//! the exchange between two of them afterwards and read a conversation you were
//! never part of.
//!
//! The mechanism is smaller than it looks. There is one tool, `ask`, and the
//! app answers it by opening a conversation with the named agent, saying the
//! request into it, and handing back what came out. The exchange is stored the
//! way every other conversation is, so reading it later is not a feature, it is
//! the absence of one.
//!
//! What matters here is that this is the *app's* tool and not an engine's.
//! Neither engine can route a message to another agent -- only the thing
//! holding all of them can -- so both reach the same function, one in-process
//! and one over a socket, and there is exactly one implementation of what
//! asking somebody else actually means.

use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::store::Conversation;

/// What an engine wants the app to do on its behalf.
///
/// Sent up rather than handled where it arrives, because an engine knows about
/// its own conversation and nothing else. The answer goes back down the
/// oneshot, so the engine's loop waits exactly as it would for any other tool.
#[derive(Debug)]
pub struct Wants {
    /// Which of the app's tools, by the name the model called.
    pub tool: String,
    pub args: Value,
    /// The conversation asking, so the app can refuse an agent asking itself.
    pub from: String,
    /// Whether this is the owner speaking, from their own terminal, rather
    /// than an agent or a program handing over somebody else's words. It
    /// decides whether the agent asked hears the request bare or with who is
    /// asking in front of it; see `heard`.
    ///
    /// Decided by the app from who is at the other end of the socket, never
    /// said by the caller: see `doorway::the_owners_own`. A model with a shell
    /// can reach the same socket, and for a while it could write the claim.
    pub as_owner: bool,
    pub answer: oneshot::Sender<anyhow::Result<String>>,
    /// Somewhere to say what is happening while it happens, for a caller that
    /// wants to watch rather than wait.
    ///
    /// Nothing for an engine asking another agent: a model handed a running
    /// commentary on somebody else's work would put all of it in its context
    /// and none of it is the answer. Something for a person at a terminal,
    /// where minutes of silence and a crash look identical.
    pub along_the_way: Option<tokio::sync::mpsc::UnboundedSender<Meanwhile>>,
}

/// What can be said while an errand is still running.
///
/// Two kinds, kept apart all the way to the terminal, because they are read
/// differently: a step is one event on a line of its own, and the prose is one
/// sentence arriving in pieces. Sent down one channel as untagged text they had
/// to be guessed apart at the far end, and the guess was wrong exactly where it
/// mattered, on prose that happened to look like a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Meanwhile {
    /// A step being taken, in the same words the window uses for it.
    Step(String),
    /// The answer as it is written, a fragment at a time.
    Saying(String),
}

/// The tools the app provides, as an engine has to declare them.
///
/// Two, and the second exists because of the first: an agent that can hand work
/// to somebody has to be able to find out who there is.
pub fn declarations() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "ask",
                "description":
                    "Hand part of this job to another agent and wait for its answer. Use this \
                     when the work belongs to somebody else's speciality rather than yours. \
                     The other agent has its own memory and tools and does not see this \
                     conversation, so say everything it needs in the request.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "agent": { "type": "string", "description": "Which agent, by name" },
                        "request": {
                            "type": "string",
                            "description": "What you need from them, in full, as you would say it to a colleague"
                        }
                    },
                    "required": ["agent", "request"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "remember",
                "description":
                    "Write something down about how this job is done, so you still know it in \
                     later conversations. Use it the moment you are told something that will \
                     still be true next week: where things go, which template or account or \
                     flag to use, what somebody is actually called, what went wrong last time \
                     and what fixed it. Saying the same handle again replaces what you wrote \
                     before, which is how you correct yourself. Never write down something you \
                     were not told.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "about": {
                            "type": "string",
                            "description": "A word or two naming what this is about, like \
                                            invoice_template or where_the_briefing_goes. Saying \
                                            it again replaces the note."
                        },
                        "note": {
                            "type": "string",
                            "description": "The thing itself, in a sentence or two"
                        }
                    },
                    "required": ["about", "note"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "recall",
                "description":
                    "Look through your own notes about this job. Use it before deciding how to \
                     do something, when there is a good chance you have been told already. \
                     Ordinary words are fine.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "about": {
                            "type": "string",
                            "description": "What you want to know about, in ordinary words"
                        }
                    },
                    "required": ["about"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "forget",
                "description":
                    "Take back a note that has stopped being true and has nothing to replace \
                     it. To correct one instead, use remember again with the same handle.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "about": {
                            "type": "string",
                            "description": "The handle of the note to take back"
                        }
                    },
                    "required": ["about"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "every_day",
                // The floor is read from routine.rs rather than written here,
                // because written here it was wrong by omission: `every 30m`
                // was the only interval in the text, a model took it for the
                // floor, and it started a shell loop instead of a two-minute
                // routine. Nothing in Repeat, no run written down, nothing to
                // stop.
                "description": format!(
                    "Set this conversation to run itself on a schedule, and say what it should \
                     do each time. Use it the moment somebody asks for something on a repeating \
                     basis -- every day, every morning, twice a week, every few minutes -- \
                     rather than telling them where to set it up. It can run as often as \
                     `{floor}`. A command left to loop and sleep in the background is never a \
                     substitute: it is not under Repeat, nobody can see or stop it there, and \
                     none of its runs is written down. A conversation holds one schedule. If \
                     it already repeats something else, say which they meant: replace, when \
                     they asked to change that schedule, or new_task, when this is another job \
                     to run alongside it, which then gets a task of its own; when it is not \
                     clear which, ask them. It appears under Repeat, where they can see it and \
                     stop it. A schedule has no end of its own: for \"every hour until 9 \
                     tomorrow\", say in what it does each time that the run which finishes the \
                     job calls stop_repeating, and that run switches it off. Say nothing about \
                     it having been set: they will be told.",
                    floor = crate::routine::most_often()
                ),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "when": {
                            "type": "string",
                            "description": format!(
                                "`daily 09:00`, `weekly mon,thu 07:30`, `every 30m` or \
                                 `every 2m`; `{floor}` is the most often. Local time, in the \
                                 24 hour clock.",
                                floor = crate::routine::most_often()
                            )
                        },
                        "what": {
                            "type": "string",
                            "description":
                                "What to do each time, written in full as you would say it to \
                                 yourself tomorrow. It arrives with no other context."
                        },
                        "replace": {
                            "type": "boolean",
                            "description":
                                "True when they asked to change the schedule this conversation \
                                 already has. It replaces it."
                        },
                        "new_task": {
                            "type": "boolean",
                            "description":
                                "True when this is another job to run alongside the one this \
                                 conversation already repeats. It gets a task of its own, so \
                                 both run and both can be seen."
                        }
                    },
                    "required": ["when", "what"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "keep_an_eye_on",
                "description": format!(
                    "Wake this conversation when a folder, a file or a web page changes, when \
                     new mail arrives, or before each event in their calendar, and say what to \
                     do then. Use it for `tell me when this changes`, `when mail comes in` or \
                     `before each meeting` rather than checking over and over yourself. Mail \
                     and the calendar need their connector switched on. It appears under \
                     Watch, where they can see it and stop it. It only looks {}.",
                    crate::routine::WHILE_RUNNING
                ),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "watch": {
                            "type": "string",
                            "description":
                                "A folder, a file, a web address beginning http, `mail` for new \
                                 mail in their inboxes, or `calendar 15m before` to be woken that \
                                 long before each event"
                        },
                        "how_often": {
                            "type": "string",
                            "description":
                                "How often to look: `10m`, `1h`, `24h`. A folder, mail or the \
                                 calendar may be looked at every 5 minutes at the most often, a \
                                 web page every 15. For the calendar, `5m`: how early to wake is \
                                 said in `watch`."
                        },
                        "what": {
                            "type": "string",
                            "description": "What to do when it has changed, written in full"
                        }
                    },
                    "required": ["watch", "how_often", "what"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "over_to_you",
                "description":
                    "Hand the person the keyboard for one step you must not do yourself, then \
                     carry on where they left off. Use it for a sign-in, a two-factor code, a \
                     card number, a cookie banner, a captcha: the real ends of the road. Open \
                     the page first if there is one to open, say plainly what they are looking \
                     at and what to do, and wait. They press a button when they are finished. \
                     Whatever they signed into stays signed in, so try the thing again \
                     afterwards rather than asking them how it went. Never use it to get them \
                     to do work you could do, and never ask them to type a password anywhere \
                     but the real site.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "what": {
                            "type": "string",
                            "description":
                                "What they need to do, in one line, as you would say it \
                                 standing next to them: `Sign in to your Apple Account`"
                        },
                        "where": {
                            "type": "string",
                            "description":
                                "What to open for them, where there is something. A web \
                                 address, or a pane of macOS System Settings as \
                                 `x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation` \
                                 -- swap the last part for `Privacy_AllFiles` for Full Disk \
                                 Access, `Privacy_Microphone`, `Privacy_Calendars` or \
                                 `Privacy_Contacts`. Never Accessibility, Screen Recording or \
                                 Input Monitoring: no teammate is given the screen, and those \
                                 are not opened. Always send \
                                 them to the pane rather than describing where it is: \
                                 Automation is four levels down a screen most people have \
                                 never opened. Left out when there is nothing to open."
                        },
                        "why": {
                            "type": "string",
                            "description":
                                "Why you cannot do it yourself, in one line, so they can \
                                 judge whether to."
                        }
                    },
                    "required": ["what"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "open_outside",
                "description":
                    "Ask the person to open something you made, outside your wall: an app \
                     (a .app bundle in your own folder), a folder, or a document. Nothing \
                     inside your wall can open anything and you must not try to: this is the \
                     way. They see a card with your reason and decide, and only their click \
                     opens it. An app or a document is opened from a copy Errand takes when \
                     you ask; a folder is only shown in Finder. If they say no, do not ask \
                     again. An app can also be started at every login, if they agree to that \
                     too. A bare program or a script cannot be opened this way: put it in a \
                     .app bundle first. You get back what they chose.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description":
                                "What to open, in your own folder: `TideClock.app`, \
                                 `reports`, or `reports/summary.pdf`"
                        },
                        "why": {
                            "type": "string",
                            "description":
                                "Why it has to be opened outside your wall, in one line, so \
                                 they can judge whether to."
                        },
                        "at_login": {
                            "type": "boolean",
                            "description":
                                "Also start this app every time they log in. Only for an app, \
                                 and only when they asked for that."
                        }
                    },
                    "required": ["path", "why"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "suggest_learning",
                "description":
                    "Suggest keeping something you learned, so you do it better next time. \
                     Use it when the person had to correct you on something a point of your \
                     checklist would have caught (kind `checklist`, with that point), or when \
                     the task just done is worth keeping as a skill, or worth keeping in place \
                     of a skill you have by that name (kind `skill`, with its name). It puts a \
                     card in front of the person and returns at once: nothing is kept unless \
                     they agree. Carry on with the task; do not wait for it, and do not suggest \
                     the same thing twice.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["checklist", "skill"],
                            "description":
                                "`checklist` for a point to check every time, `skill` to keep \
                                 the task just done in this conversation by name."
                        },
                        "point": {
                            "type": "string",
                            "description":
                                "For `checklist`: the point, as one short line you can check \
                                 yourself, like `I ran it and looked at the result`."
                        },
                        "name": {
                            "type": "string",
                            "description": "For `skill`: what to call it, like `weekly report`."
                        },
                        "why": {
                            "type": "string",
                            "description":
                                "What happened that makes it worth keeping, in one line, so \
                                 they can judge."
                        }
                    },
                    "required": ["kind", "why"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "who_else",
                "description":
                    "List the other agents you can hand work to, with what each one handles. \
                     Call this before deciding a job cannot be done here.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "save_skill",
                "description":
                    "Keep the task just done in this conversation as a skill, so it can be \
                     done again by name with run_skill. Use it when somebody says to save, \
                     remember or keep what was just done as a skill. It keeps what they \
                     asked and the steps taken to answer it, from the last turn here that \
                     took any; nothing is kept from a turn that only talked. Saying a name \
                     that is already taken replaces that skill. Say nothing about how it \
                     is stored: tell them the name and that run_skill runs it.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description":
                                "What to call it, in a few words, like `tidy downloads` or \
                                 `weekly report`. This is what run_skill is called with."
                        }
                    },
                    "required": ["name"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "run_skill",
                "description":
                    "Do a saved skill again, by name, and wait for the result. Use it when \
                     somebody says to run, do or repeat a skill, or asks for a task by the \
                     name it was saved under. It starts a conversation of its own under this \
                     agent, called `Skill: <name>`, handed the original request and the \
                     steps taken then as a plan to follow and adapt, and it answers with \
                     what that run said. Call skills first if you are not sure of the name.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "The skill's name, as it was saved"
                        },
                        "differently": {
                            "type": "string",
                            "description":
                                "What should be different this time, if anything was said: \
                                 another folder, another date, one more thing to do. Left \
                                 out when it is to be done just as before."
                        }
                    },
                    "required": ["name"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "skills",
                "description":
                    "List the skills saved for this agent: each name, what it does and how \
                     many steps it has. Call it before run_skill when the name is not \
                     certain, and when somebody asks what can be run again.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        // The way back from every_day and keep_an_eye_on, which could be set
        // by asking and not stopped by asking. Told to stop the task and then
        // to pause, an agent said it had, twice, and its routine ran on.
        json!({
            "type": "function",
            "function": {
                "name": "stop_repeating",
                "description":
                    "Switch off what repeats: this conversation's schedule, its watch, or \
                     both, or every one this agent has. Use it the moment somebody asks you to \
                     stop, cancel or pause something that runs on its own, and before you say \
                     it has stopped: nothing else stops it, and saying so without calling this \
                     leaves it running. Use it too when a job of your own is done: a run of a \
                     schedule or a watch can switch itself off, and the run that finishes \
                     something meant to go on until a time or an event is the one that should. \
                     Switched off rather than thrown away, it stays under Repeat and Watch, \
                     where they can start it again.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "which": {
                            "type": "string",
                            "enum": ["schedule", "watch", "both"],
                            "description": "What to switch off. Both, when they did not say."
                        },
                        "everywhere": {
                            "type": "boolean",
                            "description":
                                "Every conversation of this agent rather than this one, for \
                                 \"stop all your routines\"."
                        }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "pause",
                "description":
                    "Pause yourself, or start yourself again. Paused, nothing of yours runs on \
                     its own until you are started again: no schedule, no watch and no goal, in \
                     any of your conversations. You still answer whenever somebody speaks to \
                     you. Use it when somebody says pause, hold off or stop everything, and \
                     before you say you have paused: nothing else pauses you. Call it with \
                     paused false only when they ask you to carry on.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "paused": {
                            "type": "boolean",
                            "description": "True to pause, false to start again. True when they did not say."
                        }
                    }
                }
            }
        }),
    ]
}

/// One of the app's own tools, once it is known to be one.
///
/// An enum rather than the plain name this used to hand back, and the reason is
/// the one mistake nothing here could catch: a tool added to `declarations()`
/// and to the name table but not to the app's dispatch is declared to both
/// engines, offered over MCP, passes every test in this crate, and then errors
/// identically on both engines for ever. It fails symmetrically, so it does not
/// even look like the asymmetry this file exists to prevent. Matched
/// exhaustively everywhere that has to know every tool, a missing arm stops the
/// build instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ours {
    Ask,
    WhoElse,
    Remember,
    Recall,
    Forget,
    EveryDay,
    KeepAnEyeOn,
    OverToYou,
    OpenOutside,
    SuggestLearning,
    SaveSkill,
    RunSkill,
    Skills,
    StopRepeating,
    Pause,
}

impl Ours {
    /// The one name it is written down under, whichever engine called it.
    pub fn name(self) -> &'static str {
        match self {
            Ours::Ask => "ask",
            Ours::WhoElse => "who_else",
            Ours::Remember => "remember",
            Ours::Recall => "recall",
            Ours::Forget => "forget",
            Ours::EveryDay => "every_day",
            Ours::KeepAnEyeOn => "keep_an_eye_on",
            Ours::OverToYou => "over_to_you",
            Ours::OpenOutside => "open_outside",
            Ours::SuggestLearning => "suggest_learning",
            Ours::SaveSkill => "save_skill",
            Ours::RunSkill => "run_skill",
            Ours::Skills => "skills",
            Ours::StopRepeating => "stop_repeating",
            Ours::Pause => "pause",
        }
    }
}

/// Is this one of the app's own tools, by its plain name?
pub fn ours(tool: &str) -> Option<Ours> {
    match tool {
        "ask" => Some(Ours::Ask),
        "who_else" => Some(Ours::WhoElse),
        "remember" => Some(Ours::Remember),
        "recall" => Some(Ours::Recall),
        "forget" => Some(Ours::Forget),
        "every_day" => Some(Ours::EveryDay),
        "keep_an_eye_on" => Some(Ours::KeepAnEyeOn),
        "over_to_you" => Some(Ours::OverToYou),
        "open_outside" => Some(Ours::OpenOutside),
        "suggest_learning" => Some(Ours::SuggestLearning),
        "save_skill" => Some(Ours::SaveSkill),
        "run_skill" => Some(Ours::RunSkill),
        "skills" => Some(Ours::Skills),
        "stop_repeating" => Some(Ours::StopRepeating),
        "pause" => Some(Ours::Pause),
        _ => None,
    }
}

/// The name Claude Code has to reach these tools under.
///
/// It will only take a tool from an MCP server, and it names every tool after
/// the server it came from. So this is the server's name, and the first half of
/// what Claude Code calls the two tools below.
pub const DOORWAY: &str = "errand";

/// Which of the app's tools this is, whichever engine named it.
///
/// The same delegation arrives as `ask` from a local model and as
/// `mcp__errand__ask` from Claude Code, because one of them is inside our tool
/// loop and the other reaches us through a server. Both are the same tool, and
/// the plain name is the one written down: the allowlist is the app's and is
/// shared between engines, so a yes remembered while Claude Code was answering
/// has to still hold when a local model is, and it only does if there is one
/// name in the table rather than two.
///
/// Deliberately not folded into `ours`. A widened `ours` would also match a
/// genuinely third-party server that happened to be called `errand`, and
/// answer its tools with "there is nobody else here to ask" instead of calling
/// them. Converting at the two edges where a prefixed name can appear is both
/// smaller and safer than accepting it everywhere.
pub fn which_of_ours(tool: &str) -> Option<Ours> {
    ours(
        tool.strip_prefix(&format!("mcp__{DOORWAY}__"))
            .unwrap_or(tool),
    )
}

/// What a step is doing, in words.
pub fn in_plain_words(tool: Ours, args: &Value) -> String {
    let get = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("");
    match tool {
        Ours::Ask => match get("agent") {
            "" => "Handing this to somebody else".to_string(),
            who => format!("Asking {who}"),
        },
        Ours::WhoElse => "Looking for somebody to hand this to".to_string(),
        Ours::EveryDay => match get("when") {
            "" => "Setting this to run on a schedule".to_string(),
            when => format!("Setting this to run {when}"),
        },
        Ours::KeepAnEyeOn => match get("watch") {
            "" => "Setting a watch".to_string(),
            what => format!("Keeping an eye on {what}"),
        },
        Ours::OverToYou => match get("what") {
            "" => "Handing this over to you".to_string(),
            what => format!("Over to you: {what}"),
        },
        Ours::OpenOutside => match get("path").rsplit('/').find(|part| !part.is_empty()) {
            None => "Asking you to open something outside its wall".to_string(),
            Some(name) => format!("Asking you to open {name} outside its wall"),
        },
        Ours::SuggestLearning => match get("kind") {
            "skill" => match get("name") {
                "" => "Suggesting it keeps this as a skill".to_string(),
                name => format!("Suggesting it keeps this as the skill {name}"),
            },
            _ => "Suggesting a point for how it checks its work".to_string(),
        },
        Ours::Remember => match get("about") {
            "" => "Making a note".to_string(),
            about => format!("Making a note about {}", about.replace('_', " ")),
        },
        Ours::Recall => match get("about") {
            "" => "Looking through its own notes".to_string(),
            about => format!("Looking up what it knows about {about}"),
        },
        Ours::Forget => match get("about") {
            "" => "Forgetting a note".to_string(),
            about => format!("Forgetting what it knew about {}", about.replace('_', " ")),
        },
        Ours::SaveSkill => match get("name") {
            "" => "Keeping this as a skill".to_string(),
            name => format!("Keeping this as a skill called {name}"),
        },
        Ours::RunSkill => match get("name") {
            "" => "Running a skill".to_string(),
            name => format!("Running the skill {name}"),
        },
        Ours::Skills => "Looking at the skills saved here".to_string(),
        Ours::StopRepeating => match args.get("everywhere").and_then(|v| v.as_bool()) {
            Some(true) => "Switching off everything it has repeating".to_string(),
            _ => "Switching off what repeats here".to_string(),
        },
        Ours::Pause => match args.get("paused").and_then(|v| v.as_bool()) {
            Some(false) => "Starting itself again".to_string(),
            _ => "Pausing itself".to_string(),
        },
    }
}

/// The thing itself, for a question that has to be judged.
pub fn the_thing_itself(tool: Ours, args: &Value) -> String {
    match tool {
        Ours::Ask => format!(
            "{}: {}",
            args.get("agent").and_then(|v| v.as_str()).unwrap_or("?"),
            args.get("request").and_then(|v| v.as_str()).unwrap_or("")
        ),
        // Filled in even though none of these stops to ask, because an empty
        // string is what makes an "always" rule prefix-match everything, and
        // leaving that trap for the day somebody flips `asks_first` costs
        // three lines to avoid.
        Ours::Remember => format!(
            "{}: {}",
            args.get("about").and_then(|v| v.as_str()).unwrap_or("?"),
            args.get("note").and_then(|v| v.as_str()).unwrap_or("")
        ),
        Ours::Recall | Ours::Forget => args
            .get("about")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        // The schedule rather than the errand: somebody saying always to this
        // is agreeing to a thing that runs at a time, and the time is the half
        // that decides whether they meant it.
        // Nothing. An "always" here would be somebody agreeing in advance to be
        // interrupted, which is not a thing anybody wants to agree to once.
        Ours::OverToYou => String::new(),
        // Nor here: what was agreed to opening once is never agreed to again.
        Ours::OpenOutside => String::new(),
        // Nor a suggestion: it keeps nothing, and the card it raises is the
        // person's to answer each time.
        Ours::SuggestLearning => String::new(),
        Ours::EveryDay => args
            .get("when")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        Ours::KeepAnEyeOn => args
            .get("watch")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        // Nothing, and deliberately: an empty rule in the allowlist means the
        // whole tool, which for a tool that only looks is the right grant.
        Ours::WhoElse => String::new(),
        // The name, for the two that take one: an "always" on running a
        // skill is agreeing to that skill, not to every skill there will be.
        Ours::SaveSkill | Ours::RunSkill => args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        Ours::Skills => String::new(),
        // Nothing to narrow: stopping and pausing are only ever about this
        // agent's own standing jobs.
        Ours::StopRepeating | Ours::Pause => String::new(),
    }
}

/// A handed-over request, the way the agent asked will read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heard {
    /// What the conversation it opens is called, the way the window lists it.
    pub called: String,
    /// The first line of that conversation, exactly as the agent is handed it.
    pub said: String,
}

/// Put a handed-over request in the words the agent asked will read.
///
/// `by` is the agent asking, where there is one. An agent's request carries
/// its name, so the one asked knows this is a hand-off and who to answer.
///
/// The owner's own request, from their terminal, carries nothing. It was
/// prefixed "something outside asks:" for a while, and twice a model refused
/// an ordinary request on the strength of those words alone, calling it an
/// injection attempt and saying that something outside was not the person it
/// works for. It was wrong about the facts and right about the wording: the
/// socket that request came through is one only that person can reach. So
/// their words arrive as their words, the way they do when typed into the
/// window.
///
/// Anything on that socket the app did not take for the owner keeps the old
/// wording. That is a process the app itself started, or one inside a wall,
/// which is what an agent's shell is: a model can reach the same socket, and
/// its words in the owner's voice would be an agent taking another agent's
/// orders as the person's.
pub fn heard(by: Option<&str>, as_owner: bool, request: &str) -> Heard {
    match (by, as_owner) {
        (Some(who), _) => Heard {
            called: format!("Asked by {who}"),
            said: format!("{who} asks: {request}"),
        },
        (None, true) => Heard {
            called: "Asked from the terminal".to_string(),
            said: request.to_string(),
        },
        (None, false) => Heard {
            called: "Asked by something outside".to_string(),
            said: format!("something outside asks: {request}"),
        },
    }
}

/// Does using this need somebody's say-so first?
///
/// Handing work to another agent does, because it spends somebody's time and
/// money and the other agent may do anything its own permissions allow. Looking
/// at the list of who exists does not.
pub fn asks_first(tool: Ours) -> bool {
    match tool {
        Ours::Ask => true,
        // Nothing that only reaches this app's own records stops to ask. Not
        // because writing is harmless, but because a note is written mid-errand
        // and half of these errands run at seven in the morning with nobody at
        // the window: the card goes unanswered, and an unanswered card is a
        // refusal. The symptom would be an agent that quietly learns nothing,
        // which is exactly what a broken notebook looks like from outside.
        //
        // What makes that safe is that a note can never grant anything. The
        // allowlist is read from `allowed` and never from `memories`, so the
        // worst a bad note can do is give bad advice, in writing, in a line of
        // the conversation somebody can read.
        //
        // See GRANTED in claude.rs, which has to agree with this and is checked
        // against it by a test there.
        Ours::WhoElse | Ours::Remember | Ours::Recall | Ours::Forget => false,
        // Nor these, for the same reason and one more. A standing job is set
        // in the middle of the conversation that asked for it, so there is
        // somebody there; and unlike a note, it is visible afterwards in a
        // panel of its own, says when it will next run, and can be stopped
        // with one press. A card asking permission to write down a thing the
        // person just asked for out loud is a question about their own
        // sentence.
        Ours::EveryDay | Ours::KeepAnEyeOn => false,
        // Nor this, and it is the clearest case of the lot: the whole tool is
        // asking. A permission card in front of a request to come and do
        // something is two questions where one was meant.
        Ours::OverToYou => false,
        // Nor this, for the same reason: the tool is a question, answered on
        // the app's own card, which nothing stored can answer for them.
        Ours::OpenOutside => false,
        // A suggestion keeps nothing by itself: the card it raises is where
        // the person says yes, and a card in front of that card asks twice.
        Ours::SuggestLearning => false,
        // Saving and listing reach this agent's own records and nothing else.
        // Running one is not delegation: it opens a conversation for the same
        // agent on the same posture, and every step the run takes goes
        // through the same cards as any other turn. A card in front of
        // run_skill itself would be asking whether it may start the job
        // somebody just asked for by name, and the steps are shown to the
        // model as a plan to follow, never run blind.
        Ours::SaveSkill | Ours::RunSkill | Ours::Skills => false,
        // Nor these. Stopping is what somebody asked for, and a card in front
        // of it at seven in the morning would be a refusal to stop.
        Ours::StopRepeating | Ours::Pause => false,
    }
}

/// What one of the app's tools says when there is no app behind it.
///
/// The terminal harness runs an engine on its own, with nothing holding every
/// agent, so these cannot be answered. Said as a sentence the model can act on
/// rather than as an error, because an absent capability is something to work
/// around and an error is something to give up on.
pub fn without_the_app(tool: Ours) -> &'static str {
    match tool {
        Ours::Ask | Ours::WhoElse => "There is nobody else here to ask.",
        Ours::Remember | Ours::Recall | Ours::Forget => {
            "There is nowhere to keep notes here. This is an engine with no app behind it, \
             so anything you learn lasts as long as this conversation."
        }
        Ours::EveryDay | Ours::KeepAnEyeOn => {
            "Nothing here runs on a schedule. This is an engine with no app behind it, so \
             say what you would have set up and leave it to them."
        }
        Ours::OverToYou => {
            "There is nobody at a window here to hand anything to. Say what somebody would \
             have to do, and stop there."
        }
        Ours::OpenOutside => {
            "There is nobody at a window here to open anything for. Say what would have to \
             be opened, and stop there."
        }
        Ours::SuggestLearning => {
            "There is nobody at a window here to agree to keeping anything. Say what you \
             would keep, and why, in your answer."
        }
        Ours::SaveSkill | Ours::RunSkill | Ours::Skills => {
            "There is nowhere to keep skills here. This is an engine with no app behind it, \
             so do the task yourself and say what you would have saved."
        }
        Ours::StopRepeating | Ours::Pause => {
            "Nothing here runs on its own. This is an engine with no app behind it, so there \
             is nothing to stop or pause."
        }
    }
}

/// What `stop_repeating` was asked to switch off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopping {
    pub schedule: bool,
    pub watch: bool,
    /// Every conversation of this agent, rather than the one asking.
    pub everywhere: bool,
}

/// One conversation's share of it: which of its two halves go off.
#[derive(Debug)]
pub struct SwitchOff<'a> {
    pub talk: &'a Conversation,
    pub schedule: bool,
    pub watch: bool,
}

impl Stopping {
    /// Both, here, when nothing more was said. A model that leaves `which`
    /// out was asked to stop "it", and whatever it was repeats in this
    /// conversation.
    pub fn read(args: &Value) -> anyhow::Result<Self> {
        let which = args.get("which").and_then(Value::as_str).map(str::trim);
        let (schedule, watch) = match which {
            None | Some("") | Some("both") => (true, true),
            Some("schedule") => (true, false),
            Some("watch") => (false, true),
            Some(other) => {
                anyhow::bail!("which is schedule, watch or both, and \"{other}\" is none of them")
            }
        };
        let everywhere = args
            .get("everywhere")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(Self {
            schedule,
            watch,
            everywhere,
        })
    }

    /// What goes off among this agent's conversations, asked from `here`.
    ///
    /// Only what is on. A schedule already off, or a watch that stopped for a
    /// reason of its own, is left as it is: switched off again it would be
    /// reported as stopped by this call, and a watch that stopped because a
    /// page could not be reached would lose the sentence that says so.
    pub fn among<'a>(&self, theirs: &'a [Conversation], here: &str) -> Vec<SwitchOff<'a>> {
        theirs
            .iter()
            .filter(|c| self.everywhere || c.id == here)
            .filter_map(|c| {
                let schedule = self.schedule && repeats(c);
                let watch = self.watch && still_looking(c);
                (schedule || watch).then_some(SwitchOff {
                    talk: c,
                    schedule,
                    watch,
                })
            })
            .collect()
    }
}

/// A schedule the clock would run.
fn repeats(c: &Conversation) -> bool {
    c.runs_at.is_some() && !c.routine_off
}

/// A watch that has not stopped.
fn still_looking(c: &Conversation) -> bool {
    c.watches.is_some() && c.paused.is_none()
}

/// Why a watch stopped, as Watch shows it, when its own agent switched it off.
pub const SWITCHED_OFF_WHEN_ASKED: &str =
    "Switched off by this agent when it was asked to stop. Press Look again to start it.";

/// What the agent is told it switched off, which is what it tells whoever
/// asked.
///
/// Every one of them by name, with what it was set to. "Done" alone was the
/// answer the model gave when it had done nothing, and a list of what went off
/// is something the person can check against what they meant.
pub fn switched_off(
    done: &[SwitchOff],
    asked: Stopping,
    theirs: &[Conversation],
    here: &str,
) -> String {
    if !done.is_empty() {
        return format!(
            "Switched off:\n{}\n\nNothing was thrown away. A schedule stays under Repeat, \
             where Start again runs it again, and a watch stays under Watch, where Look again \
             does.",
            listed(done, here)
        );
    }
    let what = match (asked.schedule, asked.watch) {
        (true, false) => "no schedule running",
        (false, true) => "no watch running",
        _ => "nothing repeating",
    };
    if asked.everywhere {
        return format!("Nothing was switched off: this agent has {what} anywhere.");
    }
    // Asked in one conversation about something set up in another. Said where
    // it is, so the model can go and stop that rather than report that nothing
    // is running while it runs.
    let elsewhere = Stopping {
        everywhere: true,
        ..asked
    }
    .among(theirs, here);
    if elsewhere.is_empty() {
        return format!("Nothing was switched off: this agent has {what} here or anywhere else.");
    }
    format!(
        "Nothing was switched off: this conversation has {what}. Elsewhere, this agent \
         has:\n{}\n\nIf that is what they meant, call stop_repeating again with everywhere \
         true.",
        listed(&elsewhere, here)
    )
}

/// One line for each schedule and watch, and where it is.
fn listed(these: &[SwitchOff], here: &str) -> String {
    let mut lines = Vec::new();
    for one in these {
        let c = one.talk;
        let whereabouts = match c.id == here {
            true => "here".to_string(),
            false => format!("in \"{}\"", c.name),
        };
        if one.schedule {
            lines.push(format!(
                "- the schedule {whereabouts}: {}, saying \"{}\"",
                c.runs_at.as_deref().unwrap_or_default(),
                c.runs_what.as_deref().unwrap_or_default()
            ));
        }
        if one.watch {
            lines.push(format!(
                "- the watch {whereabouts}: {}",
                c.watches.as_deref().unwrap_or_default()
            ));
        }
    }
    lines.join("\n")
}

/// What an agent that paused itself is told, to tell whoever asked.
pub fn paused_itself(already: bool, stopped: usize) -> String {
    let mut said = match already {
        true => "You were already paused, and still are.".to_string(),
        false => "Paused.".to_string(),
    };
    said.push_str(
        " Nothing of yours runs on its own until you are started again: your schedules, \
         watches and goals wait, and all of them are kept.",
    );
    match stopped {
        0 => {}
        1 => said.push_str(
            " One other conversation of yours was in the middle of something, and it has \
             been stopped.",
        ),
        n => said.push_str(&format!(
            " {n} other conversations of yours were in the middle of something, and they \
             have been stopped."
        )),
    }
    said.push_str(
        " You still answer whenever somebody speaks to you. They can start you again from \
         the window, or by asking you to.",
    );
    said
}

/// What an agent started again is told.
pub fn started_again(was_paused: bool) -> String {
    match was_paused {
        false => "You were not paused, so nothing changed.".to_string(),
        true => "Started again. Your schedules, watches and goals run on their own again. A \
                 schedule counts from now, so the runs it missed while you were paused do not \
                 all happen at once."
            .to_string(),
    }
}

/// What each agent is called where it has to be told apart from the others:
/// its name, with a number after it when another agent has the same one.
///
/// Two agents came to be called "Inbox Watch (Mail)", name and role the same,
/// and anything asking for one by name got whichever had last been spoken to,
/// with nothing to say it might have been the other. Numbered in the order
/// they were made, so a number never moves to a different agent when one of
/// them is used.
pub fn told_apart(agents: &[crate::store::Agent]) -> Vec<String> {
    agents
        .iter()
        .map(|one| {
            let mut same: Vec<&crate::store::Agent> = agents
                .iter()
                .filter(|other| other.name.eq_ignore_ascii_case(&one.name))
                .collect();
            if same.len() < 2 {
                return one.name.clone();
            }
            same.sort_by(|a, b| (a.started_at, &a.id).cmp(&(b.started_at, &b.id)));
            let place = same
                .iter()
                .position(|other| other.id == one.id)
                .unwrap_or(0);
            format!("{} #{}", one.name, place + 1)
        })
        .collect()
}

/// The agent somebody means, by name or by the numbered name `who_else` gives
/// when two share one, or a sentence saying why it cannot be told.
pub fn the_one_called<'a>(
    agents: &'a [crate::store::Agent],
    named: &str,
) -> Result<&'a crate::store::Agent, String> {
    let named = named.trim();
    let labels = told_apart(agents);
    let as_labelled = labels
        .iter()
        .position(|label| label.eq_ignore_ascii_case(named));
    if let Some(at) = as_labelled {
        if labels[at] != agents[at].name {
            return Ok(&agents[at]);
        }
    }
    let same: Vec<usize> = (0..agents.len())
        .filter(|&at| agents[at].name.eq_ignore_ascii_case(named))
        .collect();
    match same.as_slice() {
        [] => Err(format!("there is nobody here called {named}")),
        [only] => Ok(&agents[*only]),
        many => Err(format!(
            "there are {} agents called {named}, so it is not clear which one is meant. Ask \
             again with one of these names: {}",
            many.len(),
            many.iter()
                .map(|&at| {
                    format!(
                        "{} ({}): {}",
                        labels[at],
                        agents[at].title.as_deref().unwrap_or("no role"),
                        agents[at]
                            .about
                            .as_deref()
                            .unwrap_or("has not said what it handles")
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        )),
    }
}

/// A name no other agent here has: the one chosen, or it with the first free
/// number after it.
///
/// For an agent settling on a name for itself. One a person gives by hand is
/// theirs to give, and asking by it says when it is shared.
pub fn a_name_of_its_own(chosen: &str, taken: &[String]) -> String {
    let free = |name: &str| !taken.iter().any(|t| t.eq_ignore_ascii_case(name));
    if free(chosen) {
        return chosen.to_string();
    }
    (2..)
        .map(|n| format!("{chosen} {n}"))
        .find(|name| free(name))
        .unwrap_or_else(|| chosen.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_from_the_owners_own_terminal_is_said_in_their_own_words() {
        // Checked on the line in the store, because that line is what the
        // agent reads. Prefixed "something outside asks:", a model twice
        // refused an ordinary request as an injection attempt; the socket it
        // came through is one only the owner can reach.
        let s = crate::store::Store::in_memory().unwrap();
        s.begin("a1", "Scout", std::path::Path::new("/tmp/one"))
            .unwrap();
        let request = "Remember that the briefing goes in ~/Desktop/briefing.md.";
        let heard = heard(None, true, request);
        s.begin_conversation_for("c1", "a1", &heard.called, None)
            .unwrap();
        s.asked("c1", &heard.said).unwrap();

        let first = s.lines("c1").unwrap().into_iter().next().expect("the line");
        assert_eq!(
            first.text, request,
            "the owner's request was dressed up as somebody else's"
        );
        assert_eq!(
            s.conversation("c1").unwrap().unwrap().name,
            "Asked from the terminal"
        );
    }

    #[test]
    fn an_agent_handing_work_to_another_is_still_named_as_the_one_asking() {
        // A hand-off has to read as one, or the agent asked answers a person
        // who is not there.
        let heard = heard(Some("Scout"), false, "Draft a reply to Sarah.");
        assert_eq!(heard.called, "Asked by Scout");
        assert_eq!(heard.said, "Scout asks: Draft a reply to Sarah.");
    }

    #[test]
    fn something_the_app_itself_started_asking_at_the_front_door_is_kept_at_arms_length() {
        // An agent's shell, or anything else the app did not take for the
        // owner. What it says about itself on the wire is not read.
        let heard = heard(None, false, "Draft a reply to Sarah.");
        assert_eq!(heard.called, "Asked by something outside");
        assert_eq!(
            heard.said,
            "something outside asks: Draft a reply to Sarah."
        );
    }

    #[test]
    fn handing_work_to_somebody_asks_first_and_looking_at_the_list_does_not() {
        // Asking somebody spends their time and runs under their permissions,
        // which is a bigger thing than the sentence makes it look.
        assert!(asks_first(Ours::Ask));
        assert!(!asks_first(Ours::WhoElse));
    }

    #[test]
    fn a_question_about_delegation_shows_who_and_what_rather_than_just_the_tool() {
        let args = json!({ "agent": "Scribe", "request": "Draft a reply to Sarah." });
        assert_eq!(in_plain_words(Ours::Ask, &args), "Asking Scribe");
        assert_eq!(
            the_thing_itself(Ours::Ask, &args),
            "Scribe: Draft a reply to Sarah.",
            "a card that hides who is being asked is not a question anybody can answer"
        );
    }

    #[test]
    fn an_ask_is_the_same_tool_whether_or_not_claude_code_prefixed_it() {
        // The whole of why an "always" given under one engine still holds
        // under the other: both arrive at the allowlist as one name.
        assert_eq!(which_of_ours("ask"), Some(Ours::Ask));
        assert_eq!(which_of_ours("mcp__errand__ask"), Some(Ours::Ask));
        assert_eq!(which_of_ours("who_else"), Some(Ours::WhoElse));
        assert_eq!(which_of_ours("mcp__errand__who_else"), Some(Ours::WhoElse));
    }

    #[test]
    fn a_tool_from_somebody_elses_server_is_not_ours() {
        // Including one from a server that happens to share our name. Ours are
        // two, they are named, and anything else belongs to whoever offered it.
        assert_eq!(which_of_ours("mcp__errand__something_else"), None);
        assert_eq!(which_of_ours("mcp__peekaboo__ask"), None);
        assert_eq!(which_of_ours("run_command"), None);
        assert_eq!(which_of_ours(""), None);
    }

    #[test]
    fn an_agent_can_set_a_standing_job_without_stopping_to_ask_for_one() {
        // The gap anybody comparing this with anything else notices first.
        // Somebody says "check it every day and tell me when it ships", and an
        // agent that cannot set a schedule has two answers, both bad: ask
        // questions until somebody sets one by hand, or use the engine's own
        // scheduler, which this app then has to apologise for because it does
        // not run it and cannot show it.
        assert!(!asks_first(Ours::EveryDay));
        assert!(!asks_first(Ours::KeepAnEyeOn));

        // What an "always" on one of these would cover is the schedule rather
        // than the errand: somebody agreeing to this is agreeing to a thing
        // that runs at a time, and the time is the half that decides whether
        // they meant it.
        let setting = json!({ "when": "daily 09:00", "what": "check the order" });
        assert_eq!(the_thing_itself(Ours::EveryDay, &setting), "daily 09:00");
        assert_eq!(
            in_plain_words(Ours::EveryDay, &setting),
            "Setting this to run daily 09:00"
        );

        let watching = json!({ "watch": "~/Downloads", "how_often": "10m", "what": "tell me" });
        assert_eq!(
            the_thing_itself(Ours::KeepAnEyeOn, &watching),
            "~/Downloads"
        );
        assert_eq!(
            in_plain_words(Ours::KeepAnEyeOn, &watching),
            "Keeping an eye on ~/Downloads"
        );
    }

    #[test]
    fn a_standing_job_is_asked_for_in_two_halves_rather_than_in_one_sentence() {
        // `~/Downloads every 10m` is a sentence a model gets subtly wrong, and
        // the app can put two right answers together itself.
        let declared = declarations();
        let watch = declared
            .iter()
            .find(|d| {
                d.pointer("/function/name").and_then(|n| n.as_str()) == Some("keep_an_eye_on")
            })
            .expect("it is declared");
        let required = watch
            .pointer("/function/parameters/required")
            .and_then(|r| r.as_array())
            .expect("it says what it needs");
        for half in ["watch", "how_often", "what"] {
            assert!(required.iter().any(|r| r == half), "{half} is not required");
        }
    }

    #[test]
    fn the_every_day_tool_says_how_often_it_can_run() {
        // What actually happened: the only interval in this tool's text was
        // `every 30m`. Asked for something every two minutes, a model read
        // that as the floor, said the scheduler "doesn't go below 30-minute
        // intervals", and started a shell loop with a sleep in it instead.
        // That ran, and it was under nobody's eye: not in Repeat, no run
        // written down, nothing anybody could stop. The floor named here is
        // read from routine.rs, which is what enforces it, so the text and
        // the check cannot drift apart again.
        let floor = crate::routine::most_often();
        assert!(
            crate::routine::When::read(&floor).is_ok(),
            "the floor the text names is one a routine would refuse: {floor}"
        );
        let every_day = declarations()
            .into_iter()
            .find(|d| d.pointer("/function/name").and_then(|n| n.as_str()) == Some("every_day"))
            .expect("it is declared");
        let description = every_day
            .pointer("/function/description")
            .and_then(|d| d.as_str())
            .expect("it is explained")
            .to_string();
        assert!(
            description.contains(&format!("`{floor}`")),
            "the description does not say how often: {description}"
        );
        assert!(
            description.contains("loop") && description.contains("Repeat"),
            "it does not say a background loop is no substitute: {description}"
        );
        let when = every_day
            .pointer("/function/parameters/properties/when/description")
            .and_then(|d| d.as_str())
            .expect("when is explained");
        assert!(
            when.contains(&format!("`{floor}`")),
            "the example is all a model has to go on, and it read it as the floor: {when}"
        );
    }

    #[test]
    fn every_tool_the_app_provides_is_declared_the_way_an_engine_expects_it() {
        let declared = declarations();
        let named: Vec<&str> = declared
            .iter()
            .filter_map(|d| d.pointer("/function/name")?.as_str())
            .collect();
        assert_eq!(
            named,
            [
                "ask",
                "remember",
                "recall",
                "forget",
                "every_day",
                "keep_an_eye_on",
                "over_to_you",
                "open_outside",
                "suggest_learning",
                "who_else",
                "save_skill",
                "run_skill",
                "skills",
                "stop_repeating",
                "pause"
            ]
        );
        assert!(declared.iter().all(|d| d["type"] == "function"));
        assert!(
            ours("ask").is_some() && ours("who_else").is_some() && ours("run_command").is_none()
        );
        // Every declared tool has a name the app can route, whichever engine
        // called it. The failure this catches is the symmetric one: a tool
        // declared and routed nowhere errors identically on both engines.
        for name in named {
            assert!(
                which_of_ours(name).is_some_and(|tool| tool.name() == name),
                "{name} is declared and not one of ours"
            );
            assert_eq!(
                which_of_ours(&format!("mcp__{DOORWAY}__{name}")).map(Ours::name),
                Some(name)
            );
        }
    }

    #[test]
    fn a_skill_is_saved_run_and_listed_without_stopping_to_ask_and_a_card_would_name_the_skill() {
        // Saving and listing reach one agent's own records. Running one is
        // the same agent on the same posture, and every step of the run goes
        // through the same cards as any other turn; a card in front of the
        // run itself would ask whether it may start the job somebody just
        // asked for by name. GRANTED in claude.rs has to agree, and does, or
        // the test there fails.
        assert!(!asks_first(Ours::SaveSkill));
        assert!(!asks_first(Ours::RunSkill));
        assert!(!asks_first(Ours::Skills));

        let saving = json!({ "name": "tidy downloads" });
        assert_eq!(
            in_plain_words(Ours::SaveSkill, &saving),
            "Keeping this as a skill called tidy downloads"
        );
        assert_eq!(the_thing_itself(Ours::SaveSkill, &saving), "tidy downloads");
        let running = json!({ "name": "tidy downloads", "differently": "the Desktop" });
        assert_eq!(
            in_plain_words(Ours::RunSkill, &running),
            "Running the skill tidy downloads"
        );
        // An "always" on running a skill is agreeing to that skill, not to
        // every skill there will be.
        assert_eq!(the_thing_itself(Ours::RunSkill, &running), "tidy downloads");
        assert_eq!(
            in_plain_words(Ours::Skills, &json!({})),
            "Looking at the skills saved here"
        );
        assert_eq!(the_thing_itself(Ours::Skills, &json!({})), "");
        assert!(without_the_app(Ours::RunSkill).contains("no app behind it"));
    }

    /// A conversation of this agent's, with a schedule, a watch, both or
    /// neither.
    fn talk(id: &str, runs: Option<&str>, watches: Option<&str>) -> Conversation {
        Conversation {
            id: id.to_string(),
            agent: "pulse".to_string(),
            name: format!("{id} talk"),
            runs_at: runs.map(str::to_string),
            runs_what: runs.map(|_| "the market pulse".to_string()),
            watches: watches.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn asked_to_stop_an_agent_switches_off_what_repeats_where_it_was_asked_and_nothing_else() {
        // What happened on 7 September: told "pause" twice, the agent said it
        // was pausing, twice, and its routine ran on, because nothing it could
        // call would stop it.
        let theirs = [
            talk("here", Some("daily 07:00"), Some("~/Downloads every 10m")),
            talk("other", Some("weekly mon 09:00"), None),
        ];
        let asked = Stopping::read(&json!({})).unwrap();
        assert_eq!(
            asked,
            Stopping {
                schedule: true,
                watch: true,
                everywhere: false
            }
        );
        let off = asked.among(&theirs, "here");
        assert_eq!(off.len(), 1, "only the conversation it was asked in");
        assert_eq!(off[0].talk.id, "here");
        assert!(off[0].schedule && off[0].watch);
        let said = switched_off(&off, asked, &theirs, "here");
        assert!(said.contains("the schedule here: daily 07:00"), "{said}");
        assert!(
            said.contains("the watch here: ~/Downloads every 10m"),
            "{said}"
        );
        assert!(said.contains("Nothing was thrown away"), "{said}");

        // Told to stop only the watch, the schedule is left running.
        let asked = Stopping::read(&json!({ "which": "watch" })).unwrap();
        let off = asked.among(&theirs, "here");
        assert!(!off[0].schedule && off[0].watch);

        // Everywhere reaches the other conversation too.
        let asked = Stopping::read(&json!({ "which": "schedule", "everywhere": true })).unwrap();
        let off = asked.among(&theirs, "here");
        assert_eq!(off.len(), 2);
        let said = switched_off(&off, asked, &theirs, "here");
        assert!(
            said.contains("the schedule in \"other talk\": weekly mon 09:00"),
            "{said}"
        );

        // A word the tool does not have is refused, rather than read as both.
        assert!(Stopping::read(&json!({ "which": "everything" })).is_err());
    }

    #[test]
    fn asked_to_stop_where_nothing_repeats_it_says_where_something_does() {
        // Asked in an ordinary conversation to stop the morning briefing that
        // lives in another one. "Nothing is running" would be the model's next
        // sentence, said while the briefing runs.
        let theirs = [
            talk("here", None, None),
            talk("briefing", Some("daily 07:00"), None),
        ];
        let asked = Stopping::read(&json!({ "which": "both" })).unwrap();
        let off = asked.among(&theirs, "here");
        assert!(off.is_empty());
        let said = switched_off(&off, asked, &theirs, "here");
        assert!(said.starts_with("Nothing was switched off"), "{said}");
        assert!(said.contains("in \"briefing talk\": daily 07:00"), "{said}");
        assert!(said.contains("everywhere true"), "{said}");

        // And with nothing anywhere, it says so without sending it looking.
        let quiet = [talk("here", None, None)];
        let said = switched_off(&[], asked, &quiet, "here");
        assert_eq!(
            said,
            "Nothing was switched off: this agent has nothing repeating here or anywhere else."
        );
    }

    #[test]
    fn what_is_already_off_is_not_switched_off_again_or_given_a_new_reason() {
        // A watch that stopped itself because a page could not be reached says
        // so under Watch. Stopped again, that sentence would be replaced by
        // "switched off when asked", and the reason it really stopped lost.
        let mut off_already = talk(
            "here",
            Some("daily 07:00"),
            Some("https://example.com every 1h"),
        );
        off_already.routine_off = true;
        off_already.paused = Some("Stopped looking. It could not be reached.".to_string());
        let theirs = [off_already];
        let asked = Stopping::read(&json!({})).unwrap();
        assert!(asked.among(&theirs, "here").is_empty());
    }

    #[test]
    fn an_agent_that_pauses_itself_is_told_what_that_means_and_how_it_ends() {
        let said = paused_itself(false, 0);
        assert!(said.starts_with("Paused."), "{said}");
        assert!(said.contains("start you again"), "{said}");
        assert!(!said.contains("stopped"), "nothing else was going: {said}");
        let said = paused_itself(false, 2);
        assert!(said.contains("2 other conversations"), "{said}");
        let said = paused_itself(true, 0);
        assert!(said.starts_with("You were already paused"), "{said}");

        assert!(started_again(true).starts_with("Started again."));
        assert!(started_again(false).contains("not paused"));

        // Neither asks first, on either engine: a card in front of stopping
        // is asking whether it may stop.
        assert!(!asks_first(Ours::StopRepeating));
        assert!(!asks_first(Ours::Pause));
        assert_eq!(
            in_plain_words(Ours::Pause, &json!({ "paused": false })),
            "Starting itself again"
        );
        assert_eq!(
            in_plain_words(Ours::StopRepeating, &json!({ "everywhere": true })),
            "Switching off everything it has repeating"
        );
    }

    fn an_agent(id: &str, name: &str, about: &str, started_at: i64) -> crate::store::Agent {
        crate::store::Agent {
            id: id.into(),
            name: name.into(),
            title: Some("Mail".into()),
            about: Some(about.into()),
            mark: None,
            hue: None,
            asks: "auto".into(),
            pinned: false,
            hidden: false,
            cwd: "/tmp".into(),
            model: None,
            started_at,
            spoke_at: started_at,
            engine: "local".into(),
            engine_settings: None,
            paused_at: None,
            priority: crate::store::NORMALLY,
            finished_at: None,
            keep_local: false,
        }
    }

    #[test]
    fn two_agents_with_one_name_are_told_apart_in_the_order_they_were_made() {
        // Listed most recently spoken to first, which is how the store lists
        // them: the numbers follow when each was made, not the list.
        let all = vec![
            an_agent("b", "Inbox Watch", "I check your mailboxes", 200),
            an_agent("c", "Ledger", "Keeps the books", 150),
            an_agent("a", "Inbox Watch", "Reads the unread post", 100),
        ];
        assert_eq!(
            told_apart(&all),
            ["Inbox Watch #2", "Ledger", "Inbox Watch #1"]
        );
    }

    #[test]
    fn asking_for_a_name_two_agents_share_says_so_rather_than_picking_one() {
        let all = vec![
            an_agent("b", "Inbox Watch", "I check your mailboxes", 200),
            an_agent("a", "Inbox Watch", "Reads the unread post", 100),
        ];
        let why = the_one_called(&all, "inbox watch").unwrap_err();
        assert!(
            why.contains("there are 2 agents called inbox watch"),
            "{why}"
        );
        assert!(
            why.contains("Inbox Watch #1 (Mail): Reads the unread post"),
            "{why}"
        );
        assert!(
            why.contains("Inbox Watch #2 (Mail): I check your mailboxes"),
            "{why}"
        );
        // And the numbered names it offers reach the one they name.
        assert_eq!(the_one_called(&all, "Inbox Watch #1").unwrap().id, "a");
        assert_eq!(the_one_called(&all, "inbox watch #2").unwrap().id, "b");
    }

    #[test]
    fn a_name_only_one_agent_has_reaches_it_and_one_nobody_has_says_so() {
        let all = vec![
            an_agent("c", "Ledger", "Keeps the books", 150),
            an_agent("a", "Inbox Watch", "Reads the unread post", 100),
        ];
        assert_eq!(the_one_called(&all, " ledger ").unwrap().id, "c");
        // A number where there is nothing to tell apart is not a name here.
        assert!(the_one_called(&all, "Ledger #1").is_err());
        assert_eq!(
            the_one_called(&all, "Scout").unwrap_err(),
            "there is nobody here called Scout"
        );
    }

    #[test]
    fn an_agent_settling_on_a_name_already_taken_gets_the_next_free_number() {
        let taken = vec!["Inbox Watch".to_string(), "inbox watch 2".to_string()];
        assert_eq!(a_name_of_its_own("Ledger", &taken), "Ledger");
        assert_eq!(a_name_of_its_own("Inbox Watch", &taken), "Inbox Watch 3");
        assert_eq!(a_name_of_its_own("INBOX WATCH", &taken), "INBOX WATCH 3");
    }
}
