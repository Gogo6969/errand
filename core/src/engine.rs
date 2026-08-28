//! What an engine says while it works, and what a thread can say back.
//!
//! One protocol, two engines behind it. Claude Code is driven as a process and
//! brings its own tools; a local model is driven by a loop of our own and
//! brings nothing but tokens. The window is not allowed to know which it is
//! talking to, because the day it knows is the day one of them grows a feature
//! the other cannot have and the two stop being interchangeable.
//!
//! The vocabulary is deliberately small. Everything an engine has to say is one
//! of five things, and each maps to something a person sees: a line of prose, a
//! step being taken, a question that stops the work, an ending, or a failure.
//! Anything an engine wants to say that does not fit is a thing the window
//! would not know how to show.

use serde::{Deserialize, Serialize};

/// A step the agent is taking, in the words a person would use.
///
/// Not the tool's name and not its arguments. "Bash: osascript -e ..." is the
/// truth and tells nobody anything; "Reading the mail on the Mac mini" is the
/// same truth said to the person whose mail it is. The engine translates, once,
/// here, so every surface says it the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// What is being done, as a sentence.
    pub what: String,
    /// The tool underneath, kept for the timeline and for when something goes
    /// wrong and somebody needs the real name of the thing that failed.
    pub tool: String,
    /// The call's own id, which the outcome will carry back when it arrives.
    ///
    /// A step and the answer to it are two events with time in between, and
    /// without something they share there is no way to put the second onto the
    /// first. The first version of this carried the tool's name here and the
    /// call id there, which looks like a pair and is not one: two steps using
    /// the same tool were indistinguishable, and a reloaded thread showed every
    /// step as though it had never answered.
    pub call: String,
}

/// What the agent needs from a person before it can go on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsYou {
    /// The question, in one sentence.
    pub asking: String,
    /// Where it got stuck, when there is something to look at.
    pub screenshot: Option<String>,
    /// Whether the person can take the controls and hand them back. A locked
    /// door somebody can walk through themselves is not the same kind of stop
    /// as a question only they can answer.
    pub can_take_over: bool,
}

/// Everything an engine is allowed to say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// The thread is live and this is what is answering it.
    Started { session: String, model: String },
    /// A line for the person to read, as it is written.
    ///
    /// `settled` is false while the words are still arriving. A window shows
    /// both; a log keeps only the settled ones, or it keeps every prefix of
    /// every sentence.
    Said { text: String, settled: bool },
    /// A step being taken.
    Doing(Step),
    /// The step finished, with what it produced where that is worth showing.
    ///
    /// `call` is the id of the step this belongs to, not the tool's name.
    Did { call: String, outcome: String },
    /// Stopped, and it is a person's turn.
    NeedsYou(NeedsYou),
    /// The turn is over and this is what came of it.
    Done { said: String },
    /// The turn ended badly. Not the same as a question: nobody was asked.
    Failed { why: String },
}

impl Event {
    /// Does this end the turn, whatever else it says?
    pub fn ends_the_turn(&self) -> bool {
        matches!(self, Event::Done { .. } | Event::Failed { .. })
    }
}

/// A live conversation with something that can do the work.
///
/// `say` may be called while the engine is mid-step. What happens then is the
/// engine's business and worth knowing: Claude Code takes the message at the
/// next turn boundary rather than interrupting the tool in flight, which is the
/// same thing the person sees in any chat with somebody who is concentrating.
pub trait Engine {
    /// Send a turn. Safe to call while the engine is working.
    fn say(&mut self, text: &str) -> anyhow::Result<()>;
    /// Stop it, whatever it is doing.
    fn stop(&mut self) -> anyhow::Result<()>;
}
