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
                "description":
                    "Set this conversation to run itself on a schedule, and say what it should \
                     do each time. Use it the moment somebody asks for something on a repeating \
                     basis -- every day, every morning, twice a week -- rather than telling \
                     them where to set it up. It replaces whatever this conversation was \
                     already set to do, and it appears under Repeat, where they can see it and \
                     stop it. Say nothing about it having been set: they will be told.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "when": {
                            "type": "string",
                            "description":
                                "`daily 09:00`, `weekly mon,thu 07:30`, or `every 30m`. \
                                 Local time, in the 24 hour clock."
                        },
                        "what": {
                            "type": "string",
                            "description":
                                "What to do each time, written in full as you would say it to \
                                 yourself tomorrow. It arrives with no other context."
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
                "description":
                    "Wake this conversation when a folder, a file or a web page changes, and \
                     say what to do then. Use it for `tell me when this changes` rather than \
                     checking over and over yourself. It appears under Watch, where they can \
                     see it and stop it. It only looks while Errand is open.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "watch": {
                            "type": "string",
                            "description": "A folder, a file, or a web address beginning http"
                        },
                        "how_often": {
                            "type": "string",
                            "description":
                                "`10m`, `1h`, `24h`. A folder may be looked at every 5 minutes \
                                 at the most often, a web page every 15."
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
                                 Access, `Privacy_Microphone`, `Privacy_Calendars`, \
                                 `Privacy_Contacts` or `Privacy_ScreenCapture`. Always send \
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
                "name": "who_else",
                "description":
                    "List the other agents you can hand work to, with what each one handles. \
                     Call this before deciding a job cannot be done here.",
                "parameters": { "type": "object", "properties": {} }
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                "who_else"
            ]
        );
        assert!(declared.iter().all(|d| d["type"] == "function"));
        assert!(
            ours("ask").is_some() && ours("who_else").is_some() && ours("run_command").is_none()
        );
    }
}
