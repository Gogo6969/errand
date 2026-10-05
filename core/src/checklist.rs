//! How a teammate checks its own work before it says it is done.
//!
//! A role was a word on a card. "QA" said nothing to the model about what a
//! tester does, and nothing anywhere said when a task was finished, so a
//! teammate stopped when it felt finished and said so. A checklist is the
//! role made concrete: a few points, the person's to write, read into every
//! conversation the teammate has, and gone through, point by point, before it
//! may call anything done. A point that fails means not done yet.
//!
//! A role it recognises offers a starter list, which is only a start: the
//! person keeps, changes or drops each point.

/// A starter list for a kind of role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Starter {
    /// What the list is called on the card: "the Code checklist".
    pub called: &'static str,
    pub points: &'static [&'static str],
}

/// The most points a list holds. More than this and none of them is read.
pub const AT_MOST: usize = 12;

/// The longest a point may be, in characters.
pub const LONGEST: usize = 200;

/// Said in every list for work that can borrow from somebody else's: whose
/// it was and where it came from, so what a teammate makes can be checked and
/// passed on with the credit it owes.
pub const CREDITS_ITS_SOURCES: &str =
    "Anything taken from someone else's work credits its author and links where it came from";

/// Starter lists, each with the words in a role that ask for it. Checked in
/// this order, so "QA engineer" is a tester before it is a builder.
const STARTERS: &[(&[&str], Starter)] = &[
    (
        &[
            "qa", "test", "tester", "review", "reviewer", "verif", "check",
        ],
        Starter {
            called: "QA",
            points: &[
                "I tried the main path from start to finish myself",
                "I tried what happens when something goes wrong",
                "Every problem I found says how to make it happen again",
                "I said what I did not test",
            ],
        },
    ),
    (
        &["code", "dev", "engineer", "build", "program", "app"],
        Starter {
            called: "Code",
            points: &[
                "It builds without warnings",
                "Its tests pass, and there is a test for what changed",
                "I ran it and looked at the result myself",
                "Nothing in it is a secret or a real person's details",
                CREDITS_ITS_SOURCES,
            ],
        },
    ),
    (
        &["writ", "copy", "editor", "docs", "text", "content"],
        Starter {
            called: "Writing",
            points: &[
                "Every fact in it is checked against where it came from",
                "Names, dates and numbers are right",
                "It says what it means in plain words, with no filler",
                "It is the length that was asked for",
                CREDITS_ITS_SOURCES,
            ],
        },
    ),
    (
        &["research", "analys", "scout", "news", "market", "finance"],
        Starter {
            called: "Research",
            points: &[
                "Every claim says where it came from",
                "The sources are recent enough for the question",
                "Where sources disagree, I say so",
                "I say how sure I am",
                CREDITS_ITS_SOURCES,
            ],
        },
    ),
    (
        &["design", "ui", "ux", "visual"],
        Starter {
            called: "Design",
            points: &[
                "It works at the smallest and the largest window size",
                "It reads in light and in dark",
                "Every control says what it does",
                "It matches the rest of what it sits in",
            ],
        },
    ),
    (
        &["watch", "storage", "monitor", "ops", "disk", "backup"],
        Starter {
            called: "Watching",
            points: &[
                "I checked the thing itself, not a copy from earlier",
                "I said what is normal and what is not",
                "Anything that needs the person says what to do about it",
                "I changed nothing that I was only asked to watch",
            ],
        },
    ),
    (
        &["mail", "inbox", "email"],
        Starter {
            called: "Mail",
            points: &[
                "Nothing was sent without the person saying so",
                "Every message that needs an answer is named",
                "Nothing was deleted",
            ],
        },
    ),
];

/// The starter list a role asks for, if any word in it is one of a kind's.
pub fn starter_for(role: &str) -> Option<Starter> {
    let words: Vec<String> = role
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    STARTERS
        .iter()
        .find(|(asks, _)| {
            words.iter().any(|w| {
                asks.iter()
                    .any(|a| w == a || (a.len() >= 4 && w.starts_with(a)))
            })
        })
        .map(|(_, starter)| *starter)
}

/// A list as it is kept: each point trimmed and on one line, empty and
/// repeated ones dropped, none too long, and no more than fit.
pub fn cleaned(points: &[String]) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for point in points {
        let one: String = point.split_whitespace().collect::<Vec<_>>().join(" ");
        let one: String = one.chars().take(LONGEST).collect();
        if one.is_empty() || kept.iter().any(|k| k.eq_ignore_ascii_case(&one)) {
            continue;
        }
        kept.push(one);
        if kept.len() == AT_MOST {
            break;
        }
    }
    kept
}

/// What the teammate reads: its points, and what they are for.
pub fn in_the_prompt(points: &[String]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let listed: Vec<String> = points
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{}. {p}", i + 1))
        .collect();
    format!(
        "Before you say a task is done, go through each of these and say in a line how it \
         went. If one does not apply, say why. If one fails, the task is not done yet: fix \
         it, or say plainly what is still wrong. When the person has to correct you on \
         something these points did not catch, suggest one that would have, with \
         suggest_learning.\n{}",
        listed.join("\n")
    )
}

/// Who it is, and then its checklist, under a heading of its own.
pub fn with_its_checklist(identity: String, points: &[String]) -> String {
    let said = in_the_prompt(points);
    match (identity.is_empty(), said.is_empty()) {
        (_, true) => identity,
        (true, false) => format!("HOW YOU CHECK YOUR WORK\n\n{said}"),
        (false, false) => format!("{identity}\n\nHOW YOU CHECK YOUR WORK\n\n{said}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_role_offers_the_starter_list_its_words_ask_for() {
        assert_eq!(starter_for("Code").unwrap().called, "Code");
        assert_eq!(starter_for("QA").unwrap().called, "QA");
        // A tester before a builder.
        assert_eq!(starter_for("QA engineer").unwrap().called, "QA");
        assert_eq!(starter_for("Writer").unwrap().called, "Writing");
        assert_eq!(starter_for("Markets").unwrap().called, "Research");
        assert_eq!(starter_for("Storage").unwrap().called, "Watching");
        assert_eq!(starter_for("Inbox").unwrap().called, "Mail");
        // A short word only as itself: "ui" is not in "build" or "guitar".
        assert_eq!(starter_for("Guitar"), None);
        assert_eq!(starter_for(""), None);
        assert_eq!(starter_for("Travel"), None);
    }

    #[test]
    fn work_that_can_borrow_credits_where_it_came_from() {
        for role in ["Code", "Writer", "Research"] {
            let starter = starter_for(role).expect("a starter list");
            assert!(
                starter.points.contains(&CREDITS_ITS_SOURCES),
                "{role}: {:?}",
                starter.points
            );
            assert!(starter.points.len() <= AT_MOST);
        }
        assert!(CREDITS_ITS_SOURCES.chars().count() <= LONGEST);
    }

    #[test]
    fn a_list_is_kept_tidy_and_short() {
        let given: Vec<String> = [
            "  It builds\n without warnings ",
            "",
            "it builds without WARNINGS",
            &"x".repeat(500),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let kept = cleaned(&given);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert_eq!(kept[0], "It builds without warnings");
        assert_eq!(kept[1].chars().count(), LONGEST);
        let many: Vec<String> = (0..30).map(|i| format!("point {i}")).collect();
        assert_eq!(cleaned(&many).len(), AT_MOST);
    }

    #[test]
    fn the_teammate_reads_its_points_as_what_done_means() {
        let points = vec!["It builds".to_string(), "Its tests pass".to_string()];
        let said = with_its_checklist("WHO YOU ARE\n\nYou are Ship Lead.".into(), &points);
        assert!(said.starts_with("WHO YOU ARE"), "{said}");
        assert!(said.contains("HOW YOU CHECK YOUR WORK"), "{said}");
        assert!(said.contains("1. It builds\n2. Its tests pass"), "{said}");
        assert!(said.contains("the task is not done yet"), "{said}");
        assert_eq!(with_its_checklist("x".into(), &[]), "x");
    }
}
