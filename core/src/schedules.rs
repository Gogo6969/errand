//! Schedules that are not Errand's.
//!
//! There are two schedulers in play and only one of them is this app's. Errand
//! has Repeat: a conversation, a time, and something to say, which the clock
//! here runs and which somebody can see and change. Claude Code has its own,
//! and an agent asked to do something every morning will reach for it, because
//! it is the tool in front of the model and the model has no idea Errand has
//! one too.
//!
//! What that looks like from outside is the worst kind of working. Somebody
//! says "do this every morning", the agent says "scheduled, every day at 7:02",
//! and it is true -- and Errand's Repeat panel is empty, nothing in this app
//! knows the job exists, and it is scoped to a session that ends. Asking for it
//! again later makes a second one. This is the shape of a promise the app
//! cannot keep and does not know it made.
//!
//! It cannot be taken over: the engine's scheduler is the engine's, and pulling
//! its jobs into this list would mean claiming to run things this app does not
//! run. What it can do is notice, and say so, at the moment it happens.

/// The engine's own scheduling tools.
///
/// Named rather than pattern-matched, because guessing from the shape of a name
/// is how a tool called `SynchroniseCronies` becomes a scheduling tool.
const THE_ENGINES_OWN: &[&str] = &["CronCreate", "CronDelete", "CronList"];

/// Whether this tool makes a schedule the engine owns.
///
/// Only the one that creates. Listing and deleting are somebody sorting out
/// what they already have, and saying this about those would be noise on top of
/// somebody already dealing with it.
pub fn makes_one_of_its_own(tool: &str) -> bool {
    tool.eq_ignore_ascii_case("CronCreate")
}

/// Whether this tool has anything to do with the engine's scheduler.
pub fn belongs_to_the_engine(tool: &str) -> bool {
    THE_ENGINES_OWN
        .iter()
        .any(|known| known.eq_ignore_ascii_case(tool))
}

/// What to say when an agent schedules something for itself.
///
/// Three things, because each of them is separately surprising: it is not in
/// Repeat, this app does not run it, and it does not outlive the session. The
/// third is the one that costs somebody a morning.
pub fn what_that_means() -> String {
    "That schedule belongs to the engine, not to Errand. It will not appear \
     under Repeat, nothing here runs it or can stop it, and it lasts only as \
     long as this conversation's session does. To have Errand keep it, set it \
     under Repeat as well."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tool_that_makes_a_schedule_is_the_one_worth_saying_something_about() {
        // Listing and deleting are somebody sorting out what they have. Saying
        // this over those is noise on top of somebody already dealing with it.
        assert!(makes_one_of_its_own("CronCreate"));
        assert!(!makes_one_of_its_own("CronList"));
        assert!(!makes_one_of_its_own("CronDelete"));
        assert!(!makes_one_of_its_own("Bash"));
    }

    #[test]
    fn a_tool_that_merely_sounds_like_one_is_not_one() {
        // Named rather than guessed at from the shape of the name.
        assert!(!belongs_to_the_engine("SynchroniseCronies"));
        assert!(!belongs_to_the_engine("Chronometer"));
        assert!(belongs_to_the_engine("CronList"));
    }

    #[test]
    fn what_is_said_covers_all_three_surprises() {
        // Each of these is separately surprising, and the last one is the one
        // that costs somebody a morning.
        let said = what_that_means();
        assert!(said.contains("Repeat"), "{said}");
        assert!(said.contains("nothing here runs it"), "{said}");
        assert!(
            said.contains("as long as this conversation's session"),
            "{said}"
        );
    }
}
