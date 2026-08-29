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
//!
//! Only one of the five goes the other way. A question is the one event with a
//! reply, and it is the reason the trait has more than `say` on it.

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

/// What the agent wants to do, and cannot do until somebody says so.
///
/// This is a question with the work already halted behind it. The agent has
/// decided on a step, the step needs permission, and nothing happens until
/// this is answered. So it carries everything needed to answer well: what it
/// wants to do in a sentence, the actual thing it would run, and the id to
/// answer with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsYou {
    /// What it wants to do, in a sentence.
    pub asking: String,
    /// The thing itself: the command, the address, whatever is concrete enough
    /// to judge. A question about a shell command that does not show you the
    /// command is a question nobody can answer honestly.
    pub detail: String,
    /// The tool underneath, for the mark beside the question.
    pub tool: String,
    /// What to answer with. Its own id, not the tool call's: one step can be
    /// asked about more than once.
    pub call: String,
    /// The step this is a question about.
    ///
    /// Two ids rather than one because they are two different things and one
    /// engine proves it: Claude Code's question carries its own request id and
    /// the tool call's id separately. This is the one that joins the question
    /// to the step in the timeline, so a question and the step it halted are
    /// one line rather than the same sentence written twice.
    pub step: String,
    /// Whether yes can be remembered, so the same question is not asked again.
    /// Only offered when the engine says there is a rule that would cover it.
    pub can_remember: bool,
    /// What remembering would actually allow: the beginning of the thing, as
    /// the engine describes it. Empty means any use of the tool.
    ///
    /// Kept because the app does the remembering, not the engine. A rule that
    /// says `curl -s https://example.com` covers fetching another page of that
    /// site and does not cover `curl`, and that distinction has to survive the
    /// trip from the engine to the list somebody can read.
    pub rule: String,
}

/// What a person says back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// Go ahead, this once.
    Yes,
    /// Go ahead, and stop asking me about this.
    Always,
    /// No. The step does not happen and the agent is told why.
    No,
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
    /// Stopped, and it is a person's turn. The work is halted until answered.
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
/// A picture somebody attached to what they said.
///
/// Carried as bytes rather than as a path, because the two engines want it in
/// two different shapes and neither of them wants a filename: Claude Code takes
/// a base64 block on its pipe, and a local model takes a data URL. A path would
/// mean each engine reading the file itself, which is two chances to disagree
/// about what happens when it is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// `image/png` and the like, as both engines label it.
    pub kind: String,
    pub base64: String,
}

impl Picture {
    /// The form a local model wants, which is the form a browser writes.
    pub fn as_data_url(&self) -> String {
        format!("data:{};base64,{}", self.kind, self.base64)
    }
}

pub trait Engine {
    /// Send a turn, with anything attached to it.
    ///
    /// Safe to call while the engine is working.
    fn say(&mut self, text: &str, pictures: &[Picture]) -> anyhow::Result<()>;
    /// Answer a question it stopped to ask. `call` is the one it came with.
    ///
    /// Nothing happens on the other side until this arrives, which is the
    /// whole point and also the thing to be careful about: a question nobody
    /// answers is a thread that waits for ever.
    fn answer(&mut self, call: &str, said: Answer) -> anyhow::Result<()>;
    /// Stop it, whatever it is doing.
    fn stop(&mut self) -> anyhow::Result<()>;
}
