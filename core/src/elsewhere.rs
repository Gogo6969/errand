//! What the engine allows without being asked, which is not this app's to grant.
//!
//! Errand's Allowed list is what somebody agreed to here, in words, and every
//! line of it can be taken back here. That was never the whole answer and the
//! panel said it was. Claude Code reads permission rules of its own out of its
//! settings files, and those are in force for every errand this app runs: on
//! this machine there are nineteen of them, and not one appeared anywhere in
//! Errand.
//!
//! An allowlist you cannot read is not a boundary, which this app says out loud
//! about somebody else's arrangement while keeping half of one itself. So they
//! are read and shown, next to Errand's own and plainly marked as not Errand's,
//! with the file they live in so somebody can go and change them.
//!
//! Read and never written. Errand's "always" goes in Errand's store precisely
//! so that it can be shown and taken back; writing into the engine's settings
//! would be putting a rule somewhere this app cannot show and cannot revoke,
//! which is the thing it exists not to do.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// One rule, and where it is written down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rule {
    /// As the engine writes it: `Bash(awk *)`, `Read(//Users/me/**)`.
    pub rule: String,
    /// The file it is in, so somebody can go and change it.
    pub whose: String,
}

/// Everything the engine has been told, from all of its files at once.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Theirs {
    /// Allowed without asking anybody.
    pub allow: Vec<Rule>,
    /// Refused whatever anybody says, which is worth seeing for the opposite
    /// reason: it explains a refusal that otherwise looks like a fault.
    pub deny: Vec<Rule>,
    /// A mode set for every session, where one is set. `bypassPermissions` is
    /// the one that matters: it means nothing is ever asked, and somebody
    /// reading a list of three careful rules would never guess it.
    pub mode: Option<Rule>,
}

/// The files the engine reads rules from, in the order it reads them.
///
/// Named rather than searched for. Guessing at which files a program reads is
/// how a list like this quietly becomes wrong, and a list that is missing a file
/// is worse than no list: it says "this is everything" while something else is
/// in force.
pub fn where_they_live(home: &Path, working_in: &Path) -> Vec<PathBuf> {
    let all = [
        // Set by whoever administers the machine, and the only one that can
        // overrule the rest.
        PathBuf::from("/Library/Application Support/ClaudeCode/managed-settings.json"),
        home.join(".claude").join("settings.json"),
        home.join(".claude").join("settings.local.json"),
        working_in.join(".claude").join("settings.json"),
        working_in.join(".claude").join("settings.local.json"),
    ];
    // Once each. An agent whose folder is the home folder -- which is where an
    // agent with no folder of its own falls back to -- otherwise names the same
    // two files twice, and every rule in them was listed twice: thirty-eight
    // rules on a machine that has nineteen.
    let mut once = Vec::new();
    for file in all {
        if !once.contains(&file) {
            once.push(file);
        }
    }
    once
}

/// Everything those files say, for showing beside Errand's own list.
pub fn read(files: &[PathBuf]) -> Theirs {
    let mut theirs = Theirs::default();
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        // Unreadable is not the same as empty, and neither is worth an error
        // here: this is a description of somebody else's arrangement, and a
        // half-written settings file should not stop the panel drawing.
        let Ok(found) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let whose = pretty(file, home_of(files));
        let permissions = found.get("permissions");
        for (which, into) in [("allow", &mut theirs.allow), ("deny", &mut theirs.deny)] {
            let Some(list) = permissions
                .and_then(|p| p.get(which))
                .and_then(|l| l.as_array())
            else {
                continue;
            };
            into.extend(list.iter().filter_map(|one| one.as_str()).map(|rule| Rule {
                rule: rule.to_string(),
                whose: whose.clone(),
            }));
        }
        if let Some(mode) = permissions
            .and_then(|p| p.get("defaultMode"))
            .and_then(|m| m.as_str())
        {
            theirs.mode = Some(Rule {
                rule: mode.to_string(),
                whose: whose.clone(),
            });
        }
    }
    theirs
}

/// A path somebody can read, rather than one that fills the panel.
///
/// The home folder is the part every one of these shares and nobody needs to
/// read, so it becomes `~` the way it is written everywhere else.
fn pretty(file: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| file.strip_prefix(home).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => file.display().to_string(),
    }
}

/// The home folder these paths were built from, taken back off the one that
/// names it. Cheaper than passing it through every function that only needs it
/// to shorten a name.
fn home_of(files: &[PathBuf]) -> Option<&Path> {
    files
        .iter()
        .find_map(|f| f.parent().filter(|p| p.ends_with(".claude"))?.parent())
}

impl Theirs {
    /// Whether there is anything here worth showing at all.
    pub fn anything(&self) -> bool {
        !self.allow.is_empty() || !self.deny.is_empty() || self.mode.is_some()
    }

    /// What a mode set for every session actually means, in words.
    ///
    /// Only the ones that change whether anybody is asked. A mode that asks is
    /// what everything here already assumes, and saying so would be noise.
    pub fn what_the_mode_means(&self) -> Option<String> {
        let mode = self.mode.as_ref()?;
        let said = match mode.rule.as_str() {
            "bypassPermissions" => {
                "The engine is set to ask nothing at all, so none of these lists \
                 decides anything: every tool runs."
            }
            "acceptEdits" => {
                "The engine is set to accept file edits without asking, whatever \
                 the lists below say."
            }
            "plan" => "The engine is set to plan rather than act, so nothing runs.",
            _ => return None,
        };
        Some(format!("{said} Set in {}.", mode.whose))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_folder(named: &str) -> PathBuf {
        let at = std::env::temp_dir().join(format!("errand-elsewhere-{named}"));
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(at.join(".claude")).expect("a folder");
        at
    }

    fn write(at: &Path, named: &str, what: &str) {
        std::fs::write(at.join(".claude").join(named), what).expect("written");
    }

    #[test]
    fn every_rule_the_engine_was_given_is_found_and_says_where_it_came_from() {
        // Nineteen of these were in force on the machine this was written on and
        // not one of them appeared anywhere in the app, while the panel beside
        // them said it was what the agent may do without asking.
        let home = a_folder("both");
        write(
            &home,
            "settings.json",
            r#"{"permissions":{"allow":["Bash(awk *)","Bash(xxd *)"],"deny":["Read(//etc/**)"]}}"#,
        );
        write(
            &home,
            "settings.local.json",
            r#"{"permissions":{"allow":["Bash(chmod +x:*)"]}}"#,
        );
        let theirs = read(&where_they_live(&home, Path::new("/nowhere")));

        assert_eq!(theirs.allow.len(), 3, "{theirs:#?}");
        assert_eq!(theirs.deny.len(), 1, "{theirs:#?}");
        // Where, so somebody can go and change it. The two files are different
        // arrangements and one of them is usually a surprise.
        assert!(
            theirs.allow[0].whose.ends_with("settings.json"),
            "{theirs:#?}"
        );
        assert!(
            theirs.allow[2].whose.ends_with("settings.local.json"),
            "{theirs:#?}"
        );
        // Shortened the way a path is written everywhere else.
        assert!(theirs.allow[0].whose.starts_with("~/"), "{theirs:#?}");
    }

    #[test]
    fn a_mode_that_asks_nobody_anything_is_the_thing_worth_saying_loudest() {
        // A list of three careful rules beside a setting that makes all of them
        // irrelevant is a lie told by arrangement rather than by sentence.
        let home = a_folder("bypass");
        write(
            &home,
            "settings.json",
            r#"{"permissions":{"defaultMode":"bypassPermissions","allow":["Bash(ls *)"]}}"#,
        );
        let theirs = read(&where_they_live(&home, Path::new("/nowhere")));
        let said = theirs.what_the_mode_means().expect("it says something");
        assert!(said.contains("nothing at all"), "{said}");
        assert!(said.contains("settings.json"), "{said}");
    }

    #[test]
    fn a_mode_that_asks_is_what_everything_already_assumes_and_is_not_mentioned() {
        let home = a_folder("default");
        write(
            &home,
            "settings.json",
            r#"{"permissions":{"defaultMode":"default"}}"#,
        );
        let theirs = read(&where_they_live(&home, Path::new("/nowhere")));
        assert_eq!(theirs.what_the_mode_means(), None);
    }

    #[test]
    fn a_settings_file_being_edited_does_not_stop_the_panel_drawing() {
        // Half-written JSON is an ordinary state for a file somebody is in the
        // middle of changing, and a panel that refuses to draw because of it
        // tells nobody anything about the rules that are in force.
        let home = a_folder("broken");
        write(
            &home,
            "settings.json",
            r#"{"permissions":{"allow":["Bash(ls *)"#,
        );
        write(
            &home,
            "settings.local.json",
            r#"{"permissions":{"allow":["Bash(awk *)"]}}"#,
        );
        let theirs = read(&where_they_live(&home, Path::new("/nowhere")));
        assert_eq!(theirs.allow.len(), 1);
        assert_eq!(theirs.allow[0].rule, "Bash(awk *)");
    }

    #[test]
    fn a_folder_that_is_the_home_folder_is_read_once_and_not_twice() {
        // What an agent with no folder of its own falls back to. Read twice,
        // every rule appeared twice, and a list that says a thing twice is a
        // list somebody stops trusting.
        let home = a_folder("same");
        write(
            &home,
            "settings.json",
            r#"{"permissions":{"allow":["Bash(awk *)","Bash(xxd *)"]}}"#,
        );
        let theirs = read(&where_they_live(&home, &home));
        assert_eq!(theirs.allow.len(), 2, "{theirs:#?}");
    }

    #[test]
    #[ignore = "reads this machine's own Claude Code settings"]
    fn what_this_machine_itself_allows() {
        // Against the real files rather than ones written here, because the
        // whole point of this module is what is actually in force on somebody's
        // machine, and a reader that only ever meets its own fixtures is a
        // reader that has never met a real settings file.
        //
        //     cargo test -p errand-core --lib elsewhere -- --ignored --nocapture
        let home = PathBuf::from(std::env::var("HOME").expect("a home folder"));
        let theirs = read(&where_they_live(&home, &home));
        println!("allowed without asking: {}", theirs.allow.len());
        for one in &theirs.allow {
            println!("  {}  ({})", one.rule, one.whose);
        }
        for one in &theirs.deny {
            println!("  refused: {}  ({})", one.rule, one.whose);
        }
        println!("mode: {:?}", theirs.mode);
    }

    #[test]
    fn nothing_anywhere_is_nothing_to_show() {
        let home = a_folder("empty");
        let theirs = read(&where_they_live(&home, Path::new("/nowhere")));
        assert!(!theirs.anything());
    }

    #[test]
    fn the_folder_an_agent_works_in_has_rules_of_its_own() {
        // A project's own settings are in force for the errands run in it, and
        // are the ones somebody is least likely to remember agreeing to.
        let home = a_folder("home");
        let project = a_folder("project");
        write(
            &project,
            "settings.json",
            r#"{"permissions":{"allow":["Bash(cargo *)"]}}"#,
        );
        let theirs = read(&where_they_live(&home, &project));
        assert_eq!(theirs.allow.len(), 1);
        assert_eq!(theirs.allow[0].rule, "Bash(cargo *)");
    }
}
