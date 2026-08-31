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
    /// A mode set for every session, where one is set.
    ///
    /// Which file it came from decides whether it means anything at all, which
    /// is why it is not simply the last one found: Errand puts
    /// `--permission-mode` on every command line, and a command-line argument
    /// outranks this setting in every file except the one an administrator
    /// controls.
    pub mode: Option<Mode>,
}

/// A mode set for every session, and whether it decides anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mode {
    /// The engine's own word: `bypassPermissions`, `acceptEdits`, `plan`.
    pub rule: String,
    /// The file it is in.
    pub whose: String,
    /// Set by whoever administers this Mac, which is the one place this
    /// outranks what Errand puts on the command line.
    pub managed: bool,
}

/// Where an administrator's settings live, which outrank everything else.
const WHOEVER_ADMINISTERS_THIS_MAC: &str =
    "/Library/Application Support/ClaudeCode/managed-settings.json";

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
        PathBuf::from(WHOEVER_ADMINISTERS_THIS_MAC),
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
            // By its name rather than its whole path: exactly one file in the
            // list above is called this, and it is the administrator's. The
            // path it lives at cannot be written by anything that would like to
            // check this behaves, which is a poor reason to leave it unchecked.
            let managed = file
                .file_name()
                .is_some_and(|named| named == "managed-settings.json");
            // Not simply the last one found. An administrator's file is read
            // first and outranks every file after it, so taking the last would
            // throw away the one mode that is genuinely in force and report one
            // that is not, naming a file somebody would then go and edit for
            // nothing.
            let already_settled = theirs.mode.as_ref().is_some_and(|m| m.managed);
            if !already_settled {
                theirs.mode = Some(Mode {
                    rule: mode.to_string(),
                    whose: whose.clone(),
                    managed,
                });
            }
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

    /// What a mode set for every session actually means here, in words.
    ///
    /// `posture` is what this agent is set to ask, which Errand puts on the
    /// command line every time it starts the engine. That matters more than
    /// anything in these files, and leaving it out was how the loudest line on
    /// the whole screen came to be the one line on it that was not true: a
    /// settings file saying `plan` was reported as "nothing runs" for an agent
    /// running everything unasked, in red, directly under a line saying it
    /// never asks.
    ///
    /// Only the modes that change whether anybody is asked. A mode that asks is
    /// what everything here already assumes, and saying so would be noise.
    pub fn what_the_mode_means(&self, posture: &str) -> Option<String> {
        let mode = self.mode.as_ref()?;
        let said = match mode.rule.as_str() {
            "bypassPermissions" => "ask nothing at all: every tool runs",
            "acceptEdits" => "accept file edits without asking",
            "plan" => "plan rather than act, so nothing runs",
            _ => return None,
        };
        // The one file Errand cannot overrule, so the one whose mode is simply
        // true.
        if mode.managed {
            return Some(format!(
                "Whoever administers this Mac has set the engine to {said}, in {}. \
                 That outranks anything Errand asks for.",
                mode.whose
            ));
        }
        // Everywhere else it is overruled, every time, by the posture above.
        // Worth saying rather than hiding: somebody who put it there is
        // entitled to know it is doing nothing here.
        let ours = crate::claude::the_mode_for(posture);
        if ours == mode.rule {
            // It agrees with what this agent already does, so it changes
            // nothing and describes nothing new.
            return None;
        }
        Some(format!(
            "{} sets the engine to {said}. That does not apply here: Errand \
             starts this agent as {ours}, on the command line, which overrules it.",
            mode.whose
        ))
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
    fn a_mode_in_somebodys_own_settings_is_overruled_and_says_so() {
        // The loudest line on the screen, and it was the one line on it that
        // was not true. Errand puts `--permission-mode` on every command line
        // and that outranks this file, so an agent set to never ask was told in
        // red that nothing runs.
        let home = a_folder("bypass");
        write(
            &home,
            "settings.json",
            r#"{"permissions":{"defaultMode":"bypassPermissions","allow":["Bash(ls *)"]}}"#,
        );
        let theirs = read(&where_they_live(&home, Path::new("/nowhere")));

        // An agent that asks: the file says ask nothing, and it is overruled.
        let said = theirs
            .what_the_mode_means("ask")
            .expect("it says something");
        assert!(said.contains("does not apply here"), "{said}");
        assert!(said.contains("settings.json"), "{said}");
        assert!(said.contains("default"), "{said}");

        // And an agent that already never asks agrees with the file, so there
        // is nothing to say that the posture above it does not say already.
        assert_eq!(theirs.what_the_mode_means("auto"), None);
    }

    #[test]
    fn the_one_file_errand_cannot_overrule_is_said_as_fact() {
        // An administrator's settings outrank the command line, so this is the
        // one mode in any of these files that is simply true.
        let theirs = Theirs {
            mode: Some(Mode {
                rule: "bypassPermissions".into(),
                whose: WHOEVER_ADMINISTERS_THIS_MAC.into(),
                managed: true,
            }),
            ..Default::default()
        };
        let said = theirs
            .what_the_mode_means("ask")
            .expect("it says something");
        assert!(said.contains("administers this Mac"), "{said}");
        assert!(said.contains("outranks"), "{said}");
        assert!(!said.contains("does not apply"), "{said}");
    }

    #[test]
    fn an_administrators_mode_is_not_thrown_away_by_a_later_file() {
        // Read first because it outranks everything after it, and overwritten
        // by every file after it: the one mode genuinely in force was the one
        // being discarded, and a file that decides nothing was named in its
        // place for somebody to go and edit.
        let home = a_folder("both-modes");
        write(
            &home,
            "settings.json",
            r#"{"permissions":{"defaultMode":"default"}}"#,
        );
        // Standing in for the administrator's file, which no test can write:
        // the same place in the list, read under the name it is known by.
        let managed = home.join("managed-settings.json");
        std::fs::write(
            &managed,
            r#"{"permissions":{"defaultMode":"bypassPermissions"}}"#,
        )
        .expect("written");

        let theirs = read(&[managed, home.join(".claude").join("settings.json")]);
        assert_eq!(
            theirs.mode.as_ref().map(|m| m.rule.as_str()),
            Some("bypassPermissions"),
            "a later file overwrote it: {theirs:#?}"
        );
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
        assert_eq!(theirs.what_the_mode_means("ask"), None);
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
