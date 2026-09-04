//! What saying "always" actually allows.
//!
//! It allowed almost nothing. The engine suggests a rule, and for a shell
//! command the suggestion is the whole command line, so the rule stored was
//! `top -l 1 -n 15 -o mem -stats pid,command,mem` and the next `top` with one
//! flag different asked again. Somebody pressing "Always" six times in a row and
//! being asked a seventh has not been given a choice, they have been given a
//! speed bump, and the way that ends is that they stop reading the questions.
//!
//! So a simple command is narrowed to the program: say always to `top -l 1 …`
//! and any `top` is allowed afterwards. That is a real widening of what was
//! agreed to, which is exactly why the button says what it will do before it is
//! pressed. "Always · any top command" is a choice somebody can make. "Always"
//! on its own is not.
//!
//! A command that does more than one thing is never narrowed. `printf 'a' >
//! a.txt; rm -rf ~` starts with `printf`, and allowing every command that
//! starts with `printf` would be allowing the second half of that one too.

use serde::Serialize;

/// A rule, and what it allows in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Allowing {
    /// What to store. The beginning of what is allowed, matched a word at a
    /// time.
    pub rule: String,
    /// What that covers, for the button. Never a guess: what this says and what
    /// the rule does are the same sentence.
    pub in_words: String,
}

/// Anything that makes a command line more than one command.
///
/// Deliberately blunt, and it errs towards not narrowing. A command containing
/// any of these is stored whole, so "always" means only this exact thing.
const MORE_THAN_ONE_THING: &[&str] = &["&&", "||", ";", "|", "$(", "`", "\n", ">", "<", "&"];

/// What "always" would allow for this question.
///
/// `suggested` is what the engine proposed: nothing at all when it offered
/// nothing to remember, and an empty string when it offered to allow the whole
/// tool. Those are different answers and were briefly the same one, which took
/// the button away from every question about a tool that has no finer rule than
/// itself.
pub fn what_always_means(tool: &str, suggested: Option<&str>) -> Option<Allowing> {
    let suggested = suggested?.trim();
    if suggested.is_empty() {
        return Some(the_whole_tool(tool));
    }
    // A folder is not a tool the engine asks about: it widens the wall a
    // never-asking agent runs behind. Stored as the folder itself, and said as
    // what it is, because "anything starting /Volumes/Disk" reads like a
    // command rule and this is a place.
    if is_a_folder(tool) {
        return Some(Allowing {
            rule: suggested.to_string(),
            in_words: format!("writing anywhere inside {suggested}"),
        });
    }

    // Only shell commands are narrowed. Everything else the engine suggests is
    // already the useful shape: a path glob for a file tool covers a directory,
    // an address prefix covers a site.
    if !tool.eq_ignore_ascii_case("bash") {
        return Some(Allowing {
            rule: suggested.to_string(),
            in_words: format!("anything starting {suggested}"),
        });
    }

    match a_single_command(suggested) {
        Some(program) => Some(Allowing {
            rule: program.clone(),
            in_words: format!("any {program} command"),
        }),
        // Said plainly, because a button that means less than the one beside it
        // should not look the same as it.
        None => Some(Allowing {
            rule: suggested.to_string(),
            in_words: "only this exact command".to_string(),
        }),
    }
}

/// Allowing every use of a tool, which is what an empty rule means.
///
/// Said in full, because it is the widest thing any of these buttons does and
/// the word "always" hides that completely.
pub fn the_whole_tool(tool: &str) -> Allowing {
    Allowing {
        rule: String::new(),
        in_words: format!("anything this agent does with {tool}"),
    }
}

/// The program a command runs, when it runs exactly one.
///
/// Nothing when the line does more than one thing, redirects, or reads a
/// variable, because in all of those the first word is not what is being
/// allowed.
fn a_single_command(command: &str) -> Option<String> {
    if MORE_THAN_ONE_THING.iter().any(|c| command.contains(c)) {
        return None;
    }
    for word in command.split_whitespace() {
        // FOO=bar in front of a command names no program.
        if word.contains('=') && !word.starts_with('-') {
            continue;
        }
        // Running something as somebody else is not the thing being allowed,
        // and allowing every `sudo` would be allowing everything.
        if word == "sudo" || word == "env" || word == "command" {
            return None;
        }
        // A path to a program is allowed as that path, not as its last part: a
        // rule of `python` should not cover `/tmp/evil/python`.
        return Some(word.to_string());
    }
    None
}

/// What a rule already granted actually covers, in words.
///
/// For the list somebody reads when deciding what to take back. A rule stored
/// as a whole command line covers only that line and nothing else, which is
/// invisible from the line itself: it looks like a permission and behaves like
/// a one-off.
pub fn in_words(tool: &str, rule: &str) -> String {
    if rule.is_empty() {
        return the_whole_tool(tool).in_words;
    }
    if is_a_folder(tool) {
        return format!("writing anywhere inside {rule}");
    }
    if !tool.eq_ignore_ascii_case("bash") {
        return format!("anything starting {rule}");
    }
    match a_single_command(rule) {
        // A rule that is exactly a program name covers every use of it.
        Some(program) if program == rule => format!("any {program} command"),
        // Anything else is the beginning of one particular command, which in
        // practice means that command and nothing else.
        _ => "only this exact command".to_string(),
    }
}

/// The one kind of allowance that is a place rather than a tool.
pub const A_FOLDER: &str = "folder";

/// Whether an allowance is for a folder the agent may write in.
pub fn is_a_folder(tool: &str) -> bool {
    tool.eq_ignore_ascii_case(A_FOLDER)
}

/// Whether something already allowed covers what is being asked.
///
/// A word at a time, not a character at a time. `starts_with` alone means a
/// rule of `top` covers `topple-everything`, which nobody agreed to.
pub fn covers(rule: &str, doing: &str) -> bool {
    if rule.is_empty() {
        return true;
    }
    if doing == rule {
        return true;
    }
    // The boundary matters: a rule that ends mid-word is a rule that leaks.
    doing.starts_with(rule)
        && doing[rule.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_said_as_a_place_to_write_and_not_as_a_command_rule() {
        // "anything starting /Volumes/Disk" reads like a command. It is a
        // place, and the words have to say which.
        let said = what_always_means("folder", Some("/Volumes/Disk")).unwrap();
        assert_eq!(said.rule, "/Volumes/Disk");
        assert_eq!(said.in_words, "writing anywhere inside /Volumes/Disk");
        assert_eq!(in_words("folder", "/Volumes/Disk"), said.in_words);
    }

    #[test]
    fn saying_always_to_a_plain_command_allows_that_command_again() {
        // The whole complaint: six "always" answers and a seventh question,
        // because the rule stored was the entire line and the next line had one
        // flag different.
        let said = what_always_means("Bash", Some("top -l 1 -n 15 -o mem -stats pid,command,mem"))
            .expect("something to remember");
        assert_eq!(said.rule, "top");
        assert_eq!(said.in_words, "any top command");
        assert!(covers(&said.rule, "top -l 2"));
        assert!(covers(&said.rule, "top"));
    }

    #[test]
    fn a_command_that_does_more_than_one_thing_is_never_narrowed() {
        // `printf 'a' > a.txt; rm -rf ~` starts with printf, and allowing
        // everything that starts with printf would allow the rest of it.
        for dangerous in [
            "printf 'a' > a.txt; ls -l",
            "cd /tmp && rm -rf everything",
            "cat notes | mail somebody",
            "echo $(whoami)",
            "ls > /etc/passwd",
        ] {
            let said = what_always_means("Bash", Some(dangerous)).expect("something");
            assert_eq!(said.rule, dangerous, "{dangerous} was narrowed");
            assert_eq!(said.in_words, "only this exact command", "{dangerous}");
        }
    }

    #[test]
    fn running_something_as_somebody_else_is_not_narrowed_either() {
        // Allowing every `sudo` would be allowing everything there is.
        for it in ["sudo rm -rf /", "env FOO=1 rm -rf /", "command rm -rf /"] {
            let said = what_always_means("Bash", Some(it)).expect("something");
            assert_eq!(said.in_words, "only this exact command", "{it}");
        }
    }

    #[test]
    fn a_variable_in_front_of_a_command_is_not_the_command() {
        let said = what_always_means("Bash", Some("RUST_LOG=debug cargo test")).expect("something");
        assert_eq!(said.rule, "cargo");
    }

    #[test]
    fn a_rule_never_covers_a_word_it_only_begins() {
        // `starts_with` on its own says a rule of `top` covers
        // `topple-everything`, which nobody agreed to.
        assert!(!covers("top", "topple-everything"));
        assert!(covers("top", "top -l 1"));
        assert!(covers("top", "top"));

        // A path rule covers what is under it, and not a sibling that merely
        // begins the same way.
        assert!(covers("/Users/me/notes", "/Users/me/notes/today.txt"));
        assert!(!covers("/Users/me/notes", "/Users/me/notes-private"));
    }

    #[test]
    fn everything_that_is_not_a_shell_command_keeps_the_shape_the_engine_asked_for() {
        // A path glob for a file tool already covers a directory, and an
        // address prefix already covers a site. Narrowing those would be
        // inventing a rule nobody suggested.
        let said = what_always_means("Edit", Some("//Users/me/project/**")).expect("something");
        assert_eq!(said.rule, "//Users/me/project/**");
        assert!(said.in_words.contains("//Users/me/project/**"));
    }

    #[test]
    fn what_was_already_granted_says_how_much_it_really_covers() {
        // The list somebody reads to decide what to take back. A whole command
        // line stored as a rule looks like a permission and behaves like a
        // one-off, and nothing on the line says which.
        assert_eq!(
            in_words("Bash", "top -l 1 -n 15 -o mem"),
            "only this exact command"
        );
        assert_eq!(in_words("Bash", "top"), "any top command");
        assert_eq!(in_words("Bash", "vm_stat"), "any vm_stat command");
        assert_eq!(
            in_words("Bash", "printf 'a' > a.txt; ls -l"),
            "only this exact command"
        );
        assert_eq!(in_words("ask", ""), "anything this agent does with ask");
        assert_eq!(
            in_words("Edit", "//Users/me/**"),
            "anything starting //Users/me/**"
        );
    }

    #[test]
    fn nothing_to_remember_means_no_button() {
        // A button that quietly does nothing is worse than no button.
        assert_eq!(what_always_means("Bash", None), None);
    }

    #[test]
    fn a_tool_with_no_finer_rule_than_itself_can_still_be_allowed() {
        // The engine offers to allow the whole tool by suggesting a rule with
        // nothing in it, which is not the same as offering nothing -- and
        // briefly was, which took the button off every question of this kind.
        let said = what_always_means("ask", Some("")).expect("something to remember");
        assert_eq!(said.rule, "");
        assert_eq!(said.in_words, "anything this agent does with ask");
        // An empty rule covers everything, which is the point of it.
        assert!(covers(&said.rule, "whatever it likes"));
    }
}
