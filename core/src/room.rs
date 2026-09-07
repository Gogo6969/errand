//! Several agents on one problem, in one conversation.
//!
//! Handing work over is one-to-one: an agent asks another, the exchange goes
//! into a conversation of its own, and the person reads it afterwards if they
//! go looking. That is right for delegation and wrong for the other thing
//! people do with a team, which is put three of them in a room and talk to all
//! of them at once.
//!
//! A room here is an ordinary conversation, filed under its first member, with
//! a table naming everybody in it. Each thing the person says is taken to every
//! member in turn, or to the one member named with `@Name`, and each answer is
//! written back into the room under the member that gave it. Every member takes
//! part through a conversation of its own, named after the room and pointing
//! back at it, because an engine session is one agent's and a member has to
//! keep its own memory of the room between turns.
//!
//! In turn, never at once. The second member hears what the first one said,
//! which is the whole of what makes it a room rather than three answers to the
//! same question. No member can add members: nothing a member can call reaches
//! that table. And the room's audit trail is the conversation itself, in
//! order, with a name on every line.
//!
//! What is decided here is decided in words and nowhere else: who a message is
//! for, what a member reads, and what the app says when something goes wrong.
//! The plumbing that opens engines and waits on them is the app's.

use crate::store::{Line, Member};
use crate::watch::reads_as_a_name;

/// Who one thing said in a room was said to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Addressed<'a> {
    /// Everyone, in the order they joined.
    Everyone,
    /// The one member named at the front with `@`.
    One(&'a Member),
    /// Somebody was named and nobody in the room is called that. Carries the
    /// name as it was typed, so the answer can say it back.
    Nobody(String),
}

/// A room needs two different agents, or it is a conversation.
pub fn at_least_two(agents: &[String]) -> anyhow::Result<()> {
    let mut distinct: Vec<&str> = agents.iter().map(String::as_str).collect();
    distinct.sort_unstable();
    distinct.dedup();
    anyhow::ensure!(
        distinct.len() >= 2,
        "a room needs at least two different agents in it"
    );
    Ok(())
}

/// What a room is called when nobody has called it anything.
///
/// The members' names, joined the way somebody would say them. Unless one of
/// them is a sentence: an agent that has not settled on a name is called by its
/// first errand, and "Bitcoin Desk and Show me the latest Bitcoin news" is not
/// a name anybody can pick out of a list.
pub fn a_name_for(members: &[Member]) -> String {
    let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    match names.iter().all(|n| reads_as_a_name(n)) {
        true => listed(&names),
        false => format!("A room of {}", names.len()),
    }
}

/// What a member's own conversation for a room is called, in the picker.
pub fn called(room: &str) -> String {
    format!("In the room: {room}")
}

/// Who a message is for.
///
/// `@Name` at the very front picks one member, by the longest member name that
/// fits, so that `@Trend Scout` reaches Trend Scout in a room that also has a
/// Scout. Case does not matter. Anything else is for everyone, including an `@`
/// later in the sentence, which is somebody quoting an address.
pub fn addressed<'a>(said: &str, members: &'a [Member]) -> Addressed<'a> {
    let Some(rest) = said.trim_start().strip_prefix('@') else {
        return Addressed::Everyone;
    };
    let fits = |m: &Member| {
        let name = m.name.trim();
        !name.is_empty()
            && rest
                .get(..name.len())
                .is_some_and(|front| front.eq_ignore_ascii_case(name))
            && rest[name.len()..]
                .chars()
                .next()
                .is_none_or(|next| !next.is_alphanumeric())
    };
    match members
        .iter()
        .filter(|m| fits(m))
        .max_by_key(|m| m.name.trim().len())
    {
        Some(one) => Addressed::One(one),
        None => Addressed::Nobody(
            rest.split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches([',', ':', ';'])
                .to_string(),
        ),
    }
}

/// What the app answers when something is said into a room whose round is
/// still going.
///
/// Refused rather than queued or taken round on top of the first. Two rounds
/// at once in one room did not merely interleave: a member's own conversation
/// can be waited on by one caller, so the second round's waiting replaced the
/// first's, the first wrote "<member> stopped before it finished" while the
/// member was still answering, and the second wrote that member's answer to
/// the first message down as the answer to its own. One round at a time is
/// what a room is, and the words are kept in the box for when it ends.
pub const STILL_ANSWERING: &str =
    "The room is still answering. Wait for the round to end, then say it again.";

/// What the app says when a name at the front of a message is nobody's.
pub fn nobody_called(name: &str, members: &[Member]) -> String {
    let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    let first = names.first().copied().unwrap_or("Name");
    match name.is_empty() {
        true => format!(
            "There is no name after the @. This room has {}. Start with @{first} to speak to \
             one of them, or leave the @ off to speak to everyone.",
            listed(&names)
        ),
        false => format!(
            "Nobody in this room is called {name}. It has {}. Start with @{first} to speak to \
             one of them, or leave the @ off to speak to everyone.",
            listed(&names)
        ),
    }
}

/// What one member has not heard yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unheard<'a> {
    pub lines: &'a [Line],
    /// Whether it has answered in this room before, which decides whether the
    /// lines are introduced as everything so far or as what came since.
    pub spoken_before: bool,
}

/// The lines a member has not heard yet: everything after the last thing it
/// said in the room, or the whole room if it has never spoken.
///
/// Judged by what it said rather than by what it was sent, so that a member
/// which failed to answer hears the same lines again next time rather than
/// losing them. Nothing extra is written down to make this work: the
/// conversation already says who said what.
pub fn unheard<'a>(lines: &'a [Line], agent: &str) -> Unheard<'a> {
    let last = lines
        .iter()
        .rposition(|l| l.said_by.as_deref() == Some(agent) && l.kind == "said");
    match last {
        Some(at) => Unheard {
            lines: &lines[at + 1..],
            spoken_before: true,
        },
        None => Unheard {
            lines,
            spoken_before: false,
        },
    }
}

/// What one member reads when it is its turn.
///
/// Told what a room is and how it works, in the words that matter to it:
/// answer now rather than wait for anybody, because nobody else is running;
/// say which part is yours; hand a part over with `ask` if it belongs to
/// somebody else. Then the lines it has not heard, each with a name in front,
/// because a member's engine is its own and sees none of the room directly.
///
/// The person's lines are labelled "The person", which is what the rest of
/// the app calls them; the app's own notes are labelled "Errand".
pub fn what_they_hear(room: &str, members: &[Member], me: &Member, unheard: &Unheard) -> String {
    let others: Vec<&str> = members
        .iter()
        .filter(|m| m.agent != me.agent)
        .map(|m| m.name.as_str())
        .collect();
    let mut said = format!(
        "You are in a room called \"{room}\" with {}. The person and every member read \
         everything said here. Members answer one after another, never at the same time, so \
         do your part now rather than waiting for anybody; if a part belongs to another member, \
         hand it over with ask. Answer for the room, in your own words, and say plainly which \
         part you did and which you left.\n\n",
        listed(&others)
    );
    said.push_str(match unheard.spoken_before {
        true => "Said since you last spoke:\n",
        false => "Said so far:\n",
    });
    let name_of = |agent: &str| {
        members
            .iter()
            .find(|m| m.agent == agent)
            .map(|m| m.name.as_str())
            .unwrap_or("A member")
    };
    for line in unheard.lines {
        let who = match (line.kind.as_str(), line.said_by.as_deref()) {
            ("mine", _) => "The person",
            ("said", Some(agent)) => name_of(agent),
            ("said", None) => "A member",
            ("note", _) => "Errand",
            _ => continue,
        };
        said.push_str(&format!("\n{who}: {}\n", line.text.trim()));
    }
    said
}

/// Names the way somebody would say them: "A", "A and B", "A, B and C".
fn listed(names: &[&str]) -> String {
    match names {
        [] => "nobody else".to_string(),
        [one] => (*one).to_string(),
        [front @ .., last] => format!("{} and {last}", front.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(agent: &str, name: &str) -> Member {
        Member {
            agent: agent.into(),
            name: name.into(),
            talk: None,
        }
    }

    fn line(seq: i64, kind: &str, text: &str, said_by: Option<&str>) -> Line {
        Line {
            seq,
            at: seq,
            kind: kind.into(),
            text: text.into(),
            call: None,
            tool: None,
            outcome: None,
            anchor: None,
            pictures: Vec::new(),
            said_by: said_by.map(str::to_string),
        }
    }

    fn three() -> Vec<Member> {
        vec![
            member("a", "Trend Scout"),
            member("b", "Scout"),
            member("c", "Disk Watch"),
        ]
    }

    #[test]
    fn a_message_beginning_with_a_members_name_goes_to_that_member_alone() {
        let members = three();
        assert_eq!(
            addressed("@Disk Watch, how full is it?", &members),
            Addressed::One(&members[2])
        );
        // Case is not something anybody types carefully.
        assert_eq!(
            addressed("@disk watch how full is it", &members),
            Addressed::One(&members[2])
        );
    }

    #[test]
    fn the_longest_name_that_fits_wins_so_scout_does_not_take_trend_scouts_messages() {
        // "Scout" is a prefix of nothing here, but "Trend Scout" is what was
        // typed and a room can have both. The one with the longer name is the
        // one meant.
        let members = three();
        assert_eq!(
            addressed("@Trend Scout what is trending", &members),
            Addressed::One(&members[0])
        );
        assert_eq!(
            addressed("@Scout what is trending", &members),
            Addressed::One(&members[1])
        );
        // "@Scouting" is not Scout.
        assert_eq!(
            addressed("@Scouting report please", &members),
            Addressed::Nobody("Scouting".into())
        );
    }

    #[test]
    fn a_message_to_nobody_in_particular_goes_to_everyone() {
        let members = three();
        assert_eq!(
            addressed("How are we doing?", &members),
            Addressed::Everyone
        );
        // An address in the middle of a sentence is a quotation, not a name.
        assert_eq!(
            addressed("Mail me at me@example.com", &members),
            Addressed::Everyone
        );
    }

    #[test]
    fn a_name_nobody_has_is_said_back_with_who_is_there() {
        let members = three();
        assert_eq!(
            addressed("@Pixel Hand draw it", &members),
            Addressed::Nobody("Pixel".into())
        );
        let said = nobody_called("Pixel", &members);
        assert!(
            said.starts_with("Nobody in this room is called Pixel."),
            "{said}"
        );
        assert!(
            said.contains("Trend Scout, Scout and Disk Watch"),
            "it does not say who is there: {said}"
        );
        assert!(said.contains("@Trend Scout"), "it does not say how: {said}");
        // An @ on its own is a slip, and the answer says what to do about it.
        assert_eq!(addressed("@", &members), Addressed::Nobody(String::new()));
        assert!(nobody_called("", &members).starts_with("There is no name after the @."));
    }

    #[test]
    fn a_member_hears_only_what_was_said_since_it_last_spoke() {
        let lines = vec![
            line(1, "mine", "Is the disk full?", None),
            line(2, "said", "Not yet.", Some("c")),
            line(3, "said", "Nothing trending about disks.", Some("a")),
            line(4, "mine", "And now?", None),
        ];
        let since = unheard(&lines, "c");
        let heard: Vec<i64> = since.lines.iter().map(|l| l.seq).collect();
        assert_eq!(heard, [3, 4], "Disk Watch heard its own answer again");
        assert!(since.spoken_before);
        // A note under a member's name is not it speaking: a member whose turn
        // failed hears the same lines again next time rather than losing them.
        let mut with_a_failure = lines.clone();
        with_a_failure.push(line(5, "note", "Disk Watch could not answer.", Some("c")));
        let heard: Vec<i64> = unheard(&with_a_failure, "c")
            .lines
            .iter()
            .map(|l| l.seq)
            .collect();
        assert_eq!(heard, [3, 4, 5]);
    }

    #[test]
    fn the_first_time_a_member_hears_the_whole_room() {
        let lines = vec![
            line(1, "mine", "Is the disk full?", None),
            line(2, "said", "Not yet.", Some("c")),
        ];
        let first = unheard(&lines, "a");
        assert_eq!(first.lines.len(), 2);
        assert!(!first.spoken_before);
    }

    #[test]
    fn what_a_member_reads_says_who_said_each_line_and_how_a_room_works() {
        let members = three();
        let lines = vec![
            line(1, "mine", "Is the disk full?", None),
            line(2, "said", "Not yet: 40% used.", Some("c")),
            line(
                3,
                "note",
                "Scout could not answer: the model server is not answering.",
                Some("b"),
            ),
            // A step is the member's own business and is not read out.
            line(4, "doing", "Reading df", None),
        ];
        let said = what_they_hear("Disk day", &members, &members[0], &unheard(&lines, "a"));
        assert!(
            said.contains("room called \"Disk day\" with Scout and Disk Watch"),
            "{said}"
        );
        assert!(said.contains("\nThe person: Is the disk full?\n"), "{said}");
        assert!(
            said.contains("\nDisk Watch: Not yet: 40% used.\n"),
            "{said}"
        );
        assert!(said.contains("\nErrand: Scout could not answer"), "{said}");
        assert!(!said.contains("Reading df"), "a step was read out: {said}");
        assert!(said.contains("Said so far:"), "{said}");
        // How it works, in the words that matter to a model deciding what to
        // do: nobody else is running, so answer now; hand a part over with ask.
        assert!(said.contains("one after another"), "{said}");
        assert!(said.contains("hand it over with ask"), "{said}");
        assert!(
            said.contains("which part you did and which you left"),
            "{said}"
        );
    }

    #[test]
    fn a_member_that_has_spoken_is_told_these_are_the_lines_since() {
        let members = three();
        let lines = vec![
            line(2, "said", "Not yet.", Some("c")),
            line(4, "mine", "And now?", None),
        ];
        let said = what_they_hear("Disk day", &members, &members[2], &unheard(&lines, "c"));
        assert!(
            !said.contains("Not yet."),
            "it heard its own answer: {said}"
        );
        assert!(said.contains("Said since you last spoke:"), "{said}");
    }

    #[test]
    fn a_room_needs_two_different_agents() {
        assert!(at_least_two(&["a".into(), "b".into()]).is_ok());
        assert!(at_least_two(&["a".into()]).is_err());
        assert!(
            at_least_two(&["a".into(), "a".into()]).is_err(),
            "the same agent twice is one agent"
        );
        assert!(at_least_two(&[]).is_err());
    }

    #[test]
    fn a_room_is_named_after_its_members_unless_one_of_them_is_a_sentence() {
        assert_eq!(a_name_for(&three()), "Trend Scout, Scout and Disk Watch");
        assert_eq!(a_name_for(&three()[..2]), "Trend Scout and Scout");
        let unnamed = vec![
            member("a", "Trend Scout"),
            member("b", "Show me the latest Bitcoin news, please."),
        ];
        assert_eq!(a_name_for(&unnamed), "A room of 2");
        assert_eq!(called("Disk day"), "In the room: Disk day");
    }
}
