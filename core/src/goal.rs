//! Something to get to, rather than something to do.
//!
//! An errand is a thing you ask for once. A routine is the same thing asked for
//! every morning. A watch is the same thing asked for when the world changes.
//! All three are a request. A goal is not a request: it is a description of what
//! being finished looks like, and the steps are the agent's problem.
//!
//! The whole difficulty is knowing when to stop, and there are three ways to get
//! that wrong. Stopping too early leaves somebody with a goal that says it is
//! done and is not. Not stopping is a bill. Going round in circles is both at
//! once, and is what actually happens: an agent that cannot do the next step
//! says the same "what is left" again and again, each time in slightly different
//! words, and looks busy while nothing moves.
//!
//! So where it has got to is not inferred and not asked for in a second call.
//! The agent is told to end each turn with one line saying where it is, in a
//! fixed shape. That line costs nothing extra, it is the agent's own account of
//! its own work, and when it stops appearing that is itself worth knowing: an
//! agent that will not say where it is has stopped being steerable, and going on
//! spending money on it is not persistence.

use serde::{Deserialize, Serialize};

/// How many turns a goal gets before it stops and says so.
///
/// Not a target. Most goals worth setting are one or two turns, and the number
/// exists for the ones that are not: it is the difference between an agent that
/// keeps trying and a bill nobody agreed to.
pub const AT_MOST_TRIES: i64 = 8;

/// The line an agent is asked to end on.
const MARKER: &str = "GOAL:";

/// What to tell an agent that has a goal.
///
/// The shape is demanded rather than suggested, and the reason is given, because
/// an instruction whose reason is withheld is one a model talks itself out of.
pub fn how_to_report(what: &str, tries: i64, left: Option<&str>) -> String {
    let mut said = format!(
        "This conversation has a goal, which is not the same as a request. The goal is:\n\n\
         {what}\n\n\
         Work towards it. You decide the steps.\n\n\
         End every turn with one line, on its own, and nothing after it:\n\n\
         {MARKER} done\n\
         or\n\
         {MARKER} not yet - <what is still left, in one sentence>\n\n\
         That line is how anybody knows where this has got to, and it is the only \
         way this stops when it is finished. Without it the goal is assumed to be \
         unfinished and eventually gives up on you.\n\n\
         Say \"{MARKER} done\" only when the goal itself is met, not when a step is."
    );
    if tries > 0 {
        said.push_str(&format!(
            "\n\nThis is attempt {} of at most {AT_MOST_TRIES}.",
            tries + 1
        ));
    }
    if let Some(left) = left {
        said.push_str(&format!(
            "\n\nLast time, what was left was: {left}\n\n\
             If you are about to say the same thing again, do not. Say what is \
             actually stopping you instead, and say it as the thing left."
        ));
    }
    said
}

/// Where a goal has got to, after a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Next {
    /// The agent says the goal is met.
    Done,
    /// Not yet, and this is what is left.
    Keep(String),
    /// The same thing left as last time. Once is a coincidence; the second
    /// time is the pattern, so this is decided by comparing against the one
    /// before rather than by counting anything.
    Circling(String),
    /// It has had its turns.
    Enough,
    /// It did not say where it was, which is its own kind of answer.
    Silent,
}

impl Next {
    /// Whether this ends the goal.
    pub fn over(&self) -> bool {
        !matches!(self, Next::Keep(_))
    }

    /// What to put in the conversation, in the words somebody reads.
    pub fn in_plain_words(&self, what: &str) -> String {
        match self {
            Next::Done => format!("Done: {what}"),
            Next::Keep(left) => format!("Still going. What is left: {left}"),
            Next::Circling(left) => format!(
                "Stopped, because it said the same thing was left twice running: {left}\n\n\
                 That is not progress, it is a loop, and the next turn would have cost the \
                 same and got the same. Either the goal needs changing or whatever is \
                 blocking it needs somebody."
            ),
            Next::Enough => format!(
                "Stopped after {AT_MOST_TRIES} attempts without finishing. The goal was: {what}\n\n\
                 Nothing is wrong: this is the ceiling that stops a goal turning into a bill. \
                 Set it again to carry on, or change it to something nearer."
            ),
            Next::Silent => format!(
                "Stopped, because it stopped saying where it had got to. A goal keeps going \
                 on the strength of the agent's own account of its progress, and an agent \
                 that will not give one cannot be followed. The goal was: {what}"
            ),
        }
    }
}

/// Read the agent's last line, and decide.
///
/// `before` is what was left after the previous turn, which is the only way to
/// notice going round in circles.
pub fn read(said: &str, tries: i64, before: Option<&str>) -> Next {
    let Some(line) = last_marker(said) else {
        return Next::Silent;
    };
    let line = line.trim();

    // "done" and nothing else. A line that says done and then keeps talking is
    // an agent hedging, and hedging counts as not done.
    if line.eq_ignore_ascii_case("done") {
        return Next::Done;
    }

    let left = line
        .strip_prefix("not yet")
        .or_else(|| line.strip_prefix("Not yet"))
        .unwrap_or(line)
        .trim_start_matches([' ', '-', ':', '\u{2014}', '\u{2013}'])
        .trim();
    let left = match left.is_empty() {
        true => "it did not say".to_string(),
        false => left.to_string(),
    };

    if before.is_some_and(|b| same_thing(b, &left)) {
        return Next::Circling(left);
    }
    // Counted after the check, so a goal that is going round says so rather than
    // running out of turns and looking like it merely ran out of time.
    if tries + 1 >= AT_MOST_TRIES {
        return Next::Enough;
    }
    Next::Keep(left)
}

/// The last line beginning with the marker, wherever it is.
///
/// Wherever, rather than the last line of the message, because a model that has
/// been told to end on a line will sometimes add a pleasantry after it, and
/// throwing away a perfectly good report over a "hope that helps" would be
/// pedantry that costs another turn.
fn last_marker(said: &str) -> Option<&str> {
    said.lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(MARKER))
}

/// Whether two accounts of what is left are the same thing said twice.
///
/// Compared loosely, because the same obstacle described twice is rarely
/// described in the same words, and comparing exactly would mean a loop is only
/// ever caught when the model is being lazy about it.
fn same_thing(one: &str, two: &str) -> bool {
    let plain = |s: &str| {
        let mut words: Vec<String> = s
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .map(str::to_string)
            .collect();
        words.sort();
        words.dedup();
        words
    };
    let (a, b) = (plain(one), plain(two));
    if a.is_empty() || b.is_empty() {
        return one.trim().eq_ignore_ascii_case(two.trim());
    }
    let shared = a.iter().filter(|w| b.contains(w)).count();
    // Most of the meaningful words in common, measured against the shorter of
    // the two so that padding one of them out cannot hide the repetition.
    shared * 4 >= a.len().min(b.len()) * 3
}

/// What a goal is, in numbers, before anybody agrees to it.
pub fn what_it_means(what: &str) -> String {
    format!(
        "The agent works towards this on its own and says at the end of every turn whether \
         it is done. It gets at most {AT_MOST_TRIES} turns, it stops early if it says the same \
         thing is left twice running, and it stops if it stops reporting at all. It only \
         runs while Errand is open. The goal is: {what}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_saying_it_is_done_ends_the_goal() {
        assert_eq!(read("all sorted\nGOAL: done", 0, None), Next::Done);
        assert_eq!(read("GOAL: DONE", 3, Some("something")), Next::Done);
    }

    #[test]
    fn an_agent_hedging_after_done_has_not_said_done() {
        // "done, apart from" is the shape of a goal that reports success and
        // leaves somebody to find out otherwise.
        let said = read("GOAL: done, apart from the tests", 0, None);
        assert!(matches!(said, Next::Keep(_)), "{said:?}");
    }

    #[test]
    fn a_pleasantry_after_the_line_does_not_throw_the_line_away() {
        // Models add one. Discarding a good report over it costs a whole turn
        // to learn nothing.
        assert_eq!(read("GOAL: done\n\nHope that helps!", 0, None), Next::Done);
    }

    #[test]
    fn what_is_left_is_kept_whatever_punctuation_it_arrives_with() {
        for line in [
            "GOAL: not yet - the tests do not pass",
            "GOAL: not yet: the tests do not pass",
            "GOAL: Not yet \u{2014} the tests do not pass",
            "GOAL: the tests do not pass",
        ] {
            assert_eq!(
                read(line, 0, None),
                Next::Keep("the tests do not pass".to_string()),
                "{line}"
            );
        }
    }

    #[test]
    fn an_agent_that_says_the_same_thing_is_left_twice_is_stopped() {
        // The most common way this fails and the hardest to see: every turn
        // looks like work, and nothing moves.
        let said = read(
            "GOAL: not yet - I still cannot reach the server",
            1,
            Some("I cannot reach the server still"),
        );
        assert!(matches!(said, Next::Circling(_)), "{said:?}");

        // And something genuinely different is not mistaken for it.
        let moved = read(
            "GOAL: not yet - the server answers now but the schema is wrong",
            1,
            Some("I cannot reach the server"),
        );
        assert!(matches!(moved, Next::Keep(_)), "{moved:?}");
    }

    #[test]
    fn going_round_in_circles_is_said_as_that_rather_than_as_running_out_of_turns() {
        // Two different things that both end the goal, and telling somebody the
        // wrong one sends them to set it going again for another eight turns of
        // the same.
        let said = read(
            "GOAL: not yet - same as before",
            AT_MOST_TRIES - 1,
            Some("same as before"),
        );
        assert!(matches!(said, Next::Circling(_)), "{said:?}");
    }

    #[test]
    fn a_goal_that_has_had_its_turns_stops() {
        let said = read("GOAL: not yet - a bit more", AT_MOST_TRIES - 1, None);
        assert_eq!(said, Next::Enough);
        assert!(said.over());
    }

    #[test]
    fn an_agent_that_stops_saying_where_it_is_is_not_carried_on_regardless() {
        // A goal runs on the agent's own account of its progress. Without one
        // there is nothing to steer by, and spending more on it is not
        // persistence.
        assert_eq!(read("I did some things.", 0, None), Next::Silent);
        assert!(read("I did some things.", 0, None).over());
    }

    #[test]
    fn everything_that_ends_a_goal_says_why_in_words_somebody_can_act_on() {
        for ending in [
            Next::Done,
            Next::Circling("the server is down".into()),
            Next::Enough,
            Next::Silent,
        ] {
            let said = ending.in_plain_words("get the tests passing");
            assert!(said.len() > 20, "{ending:?} said almost nothing: {said}");
            if ending != Next::Done {
                assert!(
                    said.contains("Stopped"),
                    "{ending:?} did not say it had stopped: {said}"
                );
            }
        }
    }

    #[test]
    fn what_it_will_cost_is_said_in_numbers_before_anybody_agrees_to_it() {
        let said = what_it_means("get the tests passing");
        assert!(said.contains(&AT_MOST_TRIES.to_string()), "{said}");
        assert!(said.contains("only runs while Errand is open"), "{said}");
    }
}
