//! Getting a conversation out of here, and going back to a point in one.
//!
//! Two things that sound like housekeeping and are really the same thing: a
//! conversation you cannot take out and cannot go back inside is a conversation
//! you have to be careful in. Careful is the opposite of what this app is for.
//! An errand you can undo is an errand you can start without deciding first
//! whether it was a good idea.
//!
//! Export is plain Markdown rather than the app's own format, because the point
//! of taking something out is that it is readable somewhere else. A file only
//! this app can open is not an export, it is a second copy of the problem.

use crate::store::{Agent, Conversation, Line};

/// A conversation, written out the way somebody would want to read it.
///
/// Steps are kept and marked, not dropped. What an agent did is most of what
/// happened, and a transcript with only the talking in it reads as though the
/// work was imagined.
pub fn as_markdown(agent: &Agent, talk: &Conversation, lines: &[Line]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", agent.name));

    let about: Vec<String> = [
        agent.title.clone(),
        agent.about.clone(),
        Some(talk.name.clone()).filter(|n| !n.is_empty()),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !s.trim().is_empty())
    .collect();
    if !about.is_empty() {
        out.push_str(&format!("*{}*\n\n", about.join(" · ")));
    }
    out.push_str(&format!("Working in `{}`\n\n---\n\n", agent.cwd));

    for line in lines {
        match line.kind.as_str() {
            "mine" => out.push_str(&format!("**You:** {}\n\n", line.text.trim())),
            "said" => out.push_str(&format!("{}\n\n", line.text.trim())),
            // A step is indented rather than quoted, because a quote is
            // something somebody said and this is something that was done.
            "doing" => {
                out.push_str(&format!("- `{}`", line.text.trim()));
                if let Some(outcome) = line.outcome.as_deref().map(str::trim) {
                    if !outcome.is_empty() {
                        out.push_str(&format!(" -> {}", one_line(outcome)));
                    }
                }
                out.push('\n');
            }
            "asking" => out.push_str(&format!(
                "- **asked:** {}{}\n",
                line.text.trim(),
                match line.outcome.as_deref().map(str::trim) {
                    Some(said) if !said.is_empty() => format!(" ({said})"),
                    _ => " (never answered)".to_string(),
                }
            )),
            "ended" => out.push_str(&format!("\n> {}\n\n", line.text.trim())),
            _ => {}
        }
    }
    out
}

/// A tool's outcome on one line, since a table of them is unreadable otherwise.
fn one_line(said: &str) -> String {
    let flat: String = said.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.chars().count() > 120 {
        false => flat,
        true => format!("{}…", flat.chars().take(119).collect::<String>()),
    }
}

/// A filename somebody could find again, from a name they chose.
///
/// Not the conversation's id. An id is what the machine calls it, and a folder
/// of uuids is a folder nobody opens twice.
pub fn as_filename(agent: &str, talk: &str) -> String {
    let mut name = format!("{agent} - {talk}");
    name = name
        .chars()
        .map(|c| match c {
            // The two the filesystem forbids, and the ones that make a name
            // awkward to type at a shell.
            '/' | ':' | '\\' | '"' | '\'' | '*' | '?' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // Long enough to say what it is, short enough for every filesystem.
    if name.chars().count() > 90 {
        name = name.chars().take(90).collect();
    }
    format!("{}.md", name.trim_matches(|c| c == '-' || c == ' '))
}

/// The conversation so far, for an engine that has no memory of it.
///
/// Used when a conversation carries on from a point in another one rather than
/// from its end. Claude Code can fork its own session, but only from a message
/// it named, and it does not name the point you go back to; a local model
/// keeps no session at all. Both are handed the same thing: what was said,
/// written out, so that continuing reads as continuing rather than as starting
/// again in a room with the same wallpaper.
///
/// Steps are folded in as prose rather than rebuilt as tool calls. A stored
/// step keeps the sentence and not the arguments, so a faithful call cannot be
/// reconstructed from it, and a result with no call to pair with is a message
/// most endpoints refuse outright.
pub fn as_a_reminder(lines: &[Line]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "WHERE THIS CARRIES ON FROM

         This conversation continues an earlier one. What follows is what was          said in it, up to the point it was carried on from. Treat it as          something you and this person already went through together, not as          something being said to you now.\n\n",
    );
    for line in lines {
        match line.kind.as_str() {
            "mine" => out.push_str(&format!("They said: {}\n\n", line.text.trim())),
            "said" => out.push_str(&format!("You said: {}\n\n", line.text.trim())),
            "doing" => {
                out.push_str(&format!("You did: {}", line.text.trim()));
                match line.outcome.as_deref().map(str::trim) {
                    Some(got) if !got.is_empty() => {
                        out.push_str(&format!(" and got: {}\n", one_line(got)));
                    }
                    _ => out.push('\n'),
                }
            }
            _ => {}
        }
    }
    out.push_str("\nThat is where it was carried on from. Carry on.\n");
    out
}

/// Where a fork or a rewind stops.
///
/// Given inclusively: everything up to and including this line is kept, which
/// is what somebody means when they point at a message and say "from here".
pub fn up_to(lines: &[Line], seq: i64) -> Vec<Line> {
    lines.iter().filter(|l| l.seq <= seq).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(seq: i64, kind: &str, text: &str, outcome: Option<&str>) -> Line {
        Line {
            seq,
            at: 0,
            kind: kind.into(),
            text: text.into(),
            call: None,
            tool: None,
            outcome: outcome.map(str::to_string),
            anchor: None,
            pictures: Vec::new(),
        }
    }

    fn agent() -> Agent {
        Agent {
            id: "a".into(),
            name: "Bitcoin Desk".into(),
            title: Some("Markets".into()),
            about: Some("The morning crypto briefing".into()),
            mark: None,
            hue: None,
            asks: "ask".into(),
            pinned: false,
            hidden: false,
            cwd: "/tmp/desk".into(),
            model: None,
            started_at: 0,
            spoke_at: 0,
            engine: "claude".into(),
            engine_settings: None,
        }
    }

    fn talk() -> Conversation {
        // Only what these tests are about. Everything else is whatever a
        // conversation is when nothing has been said about it, which is what
        // stops a new column breaking every fixture in the file.
        Conversation {
            id: "c".into(),
            agent: "a".into(),
            name: "First".into(),
            opened: true,
            ..Default::default()
        }
    }

    #[test]
    fn what_was_done_is_in_the_export_and_not_only_what_was_said() {
        // A transcript with the tool calls stripped out reads as though the
        // agent imagined the work, which is the opposite of what somebody
        // exporting it wants to show.
        let out = as_markdown(
            &agent(),
            &talk(),
            &[
                line(1, "mine", "What is bitcoin doing?", None),
                line(
                    2,
                    "doing",
                    "Looking something up on the web",
                    Some("results"),
                ),
                line(3, "said", "**BTC** is around $77,700.", None),
            ],
        );
        assert!(out.contains("**You:** What is bitcoin doing?"));
        assert!(out.contains("Looking something up on the web"));
        assert!(out.contains("**BTC** is around $77,700."));
        assert!(out.starts_with("# Bitcoin Desk"));
        assert!(out.contains("Markets · The morning crypto briefing · First"));
    }

    #[test]
    fn a_question_nobody_answered_says_so_rather_than_looking_like_one_that_was() {
        let out = as_markdown(
            &agent(),
            &talk(),
            &[line(1, "asking", "Run a command", None)],
        );
        assert!(out.contains("never answered"), "{out}");
    }

    #[test]
    fn a_very_long_outcome_is_cut_so_the_export_stays_readable() {
        let long = "x ".repeat(400);
        let out = as_markdown(
            &agent(),
            &talk(),
            &[line(1, "doing", "Reading", Some(&long))],
        );
        let line = out
            .lines()
            .find(|l| l.contains("Reading"))
            .expect("the step");
        assert!(
            line.chars().count() < 200,
            "it ran to {} chars",
            line.chars().count()
        );
    }

    #[test]
    fn a_filename_survives_a_name_with_a_slash_in_it() {
        // An agent names itself, and nothing stops it choosing something with a
        // slash in. On a filesystem that is a directory that does not exist.
        assert_eq!(
            as_filename("Mail/Drafts", "Re: tuesday"),
            "Mail-Drafts - Re- tuesday.md"
        );
        assert!(!as_filename("a".repeat(200).as_str(), "b").contains(char::is_whitespace));
        assert!(as_filename(&"a".repeat(200), "b").len() < 100);
    }

    #[test]
    fn a_reminder_reads_as_something_that_already_happened() {
        // Without that framing an engine treats the whole prefix as an
        // instruction it has just been given, and starts doing all of it again.
        let said = as_a_reminder(&[
            line(1, "mine", "Find the invoice", None),
            line(2, "doing", "Reading the folder", Some("three files")),
            line(3, "said", "It is invoice-42.pdf.", None),
        ]);
        assert!(said.contains("already went through together"), "{said}");
        assert!(said.contains("They said: Find the invoice"));
        assert!(said.contains("You did: Reading the folder and got: three files"));
        assert!(said.contains("You said: It is invoice-42.pdf."));
    }

    #[test]
    fn a_reminder_of_nothing_is_nothing_rather_than_a_heading() {
        // A block that says "here is what happened" above nothing reads as
        // history that has been lost.
        assert_eq!(as_a_reminder(&[]), "");
    }

    #[test]
    fn going_back_to_a_line_keeps_that_line_and_drops_what_came_after() {
        // Inclusive, because "from here" means including the message somebody
        // is pointing at.
        let lines: Vec<Line> = (1..=5).map(|n| line(n, "said", "x", None)).collect();
        let kept = up_to(&lines, 3);
        assert_eq!(kept.iter().map(|l| l.seq).collect::<Vec<_>>(), [1, 2, 3]);
    }
}
