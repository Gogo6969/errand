//! What changed, for somebody who has just installed this over the last one.
//!
//! There is no updater and nowhere to update from, so every copy of Errand
//! arrives by hand, sometimes three times in an evening. That makes "what is
//! different about this one" a real question with no way to answer it, and the
//! answer is usually the difference between a bug worth reporting and one that
//! was fixed two builds ago.
//!
//! The notes are compiled in rather than bundled beside the app. A file that
//! ships next to the binary is a file that can be missing, and notes that are
//! sometimes there are worse than none: they turn "nothing changed" and "the
//! file did not come with it" into the same empty panel.

use serde::Serialize;

/// The notes themselves, as written.
const WRITTEN: &str = include_str!("../../CHANGES.md");

/// Which Errand this is.
pub const THIS_ONE: &str = env!("CARGO_PKG_VERSION");

/// One version, and what a person would notice about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notes {
    pub version: String,
    /// One line each, as sentences rather than as commit subjects.
    pub lines: Vec<String>,
}

/// Every version in the notes, newest first, as the file is written.
pub fn all() -> Vec<Notes> {
    let mut found: Vec<Notes> = Vec::new();
    for line in WRITTEN.lines() {
        let line = line.trim_end();
        if let Some(version) = line.strip_prefix("## ") {
            found.push(Notes {
                version: version.trim().to_string(),
                lines: Vec::new(),
            });
            continue;
        }
        let Some(here) = found.last_mut() else {
            // Everything above the first version is the note about the notes.
            continue;
        };
        match line.strip_prefix("- ") {
            Some(said) => here.lines.push(said.trim().to_string()),
            // A line wrapped in the file is one sentence in the app. Written
            // as it wraps, the notes read as though somebody had put a full
            // stop in the middle of every second sentence.
            None if !line.trim().is_empty() => {
                if let Some(last) = here.lines.last_mut() {
                    last.push(' ');
                    last.push_str(line.trim());
                }
            }
            _ => {}
        }
    }
    found
}

/// What is new in the version running, if anything is written down for it.
pub fn this_one() -> Option<Notes> {
    all().into_iter().find(|n| n.version == THIS_ONE)
}

/// Where the last version somebody was shown is remembered.
///
/// A file rather than a row, because it is one word about the app rather than
/// anything about an agent, and because it has to survive a store being thrown
/// away and rebuilt.
pub fn where_it_is_remembered(here: &std::path::Path) -> std::path::PathBuf {
    here.join("seen-version")
}

/// Whether this version's notes have been put in front of anybody yet.
pub fn already_seen(here: &std::path::Path) -> bool {
    std::fs::read_to_string(where_it_is_remembered(here))
        .map(|seen| seen.trim() == THIS_ONE)
        .unwrap_or(false)
}

/// Remember that they have.
///
/// Best effort. Being unable to write this means the notes are shown once
/// more, which is a far better failure than refusing to show the app.
pub fn seen(here: &std::path::Path) {
    let _ = std::fs::write(where_it_is_remembered(here), THIS_ONE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_running_has_notes_written_for_it() {
        // The one that goes stale silently: a version bumped without a section
        // added leaves the panel empty and nothing says why.
        let notes = this_one().unwrap_or_else(|| {
            panic!("no notes for version {THIS_ONE}. Add a `## {THIS_ONE}` section to CHANGES.md")
        });
        assert!(!notes.lines.is_empty(), "{notes:?}");
    }

    #[test]
    fn a_sentence_wrapped_in_the_file_is_one_sentence_in_the_app() {
        // Written as it wraps, the notes read as though somebody had put a full
        // stop in the middle of every second sentence.
        let notes = this_one().expect("notes");
        assert!(
            notes.lines.iter().all(|line| !line.contains('\n')),
            "{notes:?}"
        );
        assert!(
            notes.lines.iter().any(|line| line.len() > 90),
            "every line is short enough that the wrapping was never tested: {notes:?}"
        );
    }

    #[test]
    fn what_is_above_the_first_version_is_not_a_note_about_a_version() {
        // The file starts with a paragraph about the notes themselves, which
        // belongs to nobody's version.
        let all = all();
        assert!(!all.is_empty());
        assert!(
            all.iter().all(|n| !n.version.is_empty()),
            "a section with no version: {all:?}"
        );
        assert!(
            !all.iter()
                .any(|n| n.lines.iter().any(|l| l.contains("Written for somebody"))),
            "the note about the notes was read as a version's note: {all:?}"
        );
    }

    #[test]
    fn what_was_last_shown_is_remembered_and_read_back() {
        let here = std::env::temp_dir().join("errand-changes-seen");
        let _ = std::fs::remove_dir_all(&here);
        std::fs::create_dir_all(&here).expect("a folder");

        assert!(!already_seen(&here), "nothing has been shown yet");
        seen(&here);
        assert!(already_seen(&here));

        // A different version is not this one, however recently that one was
        // shown, which is the whole point of writing it down.
        std::fs::write(where_it_is_remembered(&here), "0.0.1-something-else").expect("written");
        assert!(!already_seen(&here));
    }
}
